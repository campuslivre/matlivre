//! Fused evaluation of elementwise expressions on large arrays.
//!
//! `exp(-x.^2) .* sin(20*x) + sqrt(x+1)` evaluated operator by operator reads and writes a full
//! temporary array for every operator, and on arrays of millions of elements the time goes to
//! memory traffic. Here the whole expression is evaluated in blocks small enough to stay in the
//! cache, on several threads, and only the final result is written to memory.
//!
//! Only expressions whose meaning is plain double arithmetic are fused: variables holding real
//! double arrays (all of one size, or scalars, or rows and columns that expand over a matrix),
//! numeric literals, `+ - .* ./ .^`, unary minus, `*` and `/` with a scalar operand, the
//! comparisons, `&`, `|` and `~`, and a short list of elementwise functions. `find(expression)`
//! of such an expression is fused too: the indices are collected block by block without ever
//! building the logical array. The arithmetic of every operator is the same as in `ops`, so
//! results are identical. Whenever something does not fit (another class, complex results such
//! as `sqrt` of a negative number or a negative base with a fractional exponent, NaN in a
//! logical operator, incompatible sizes) the function gives up and the ordinary evaluator runs,
//! so behaviour and error messages are unchanged. `MATLIVRE_NOFUSE=1` turns fusion off.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use rayon::prelude::*;

use crate::ast::{BinOp, Expr, UnOp};
use crate::interp::Interpreter;
use crate::ops::PAR_MIN;
use crate::value::{Kind, Matrix, Value};

/// Elements per block: 4096 doubles per temporary stay in the first-level caches.
const BLOCK: usize = 4096;
/// Fewest operators (calls count) for which fusion pays.
const MIN_OPS: usize = 3;
/// Fewest rows for which expanding a row or column over a matrix is done in segments.
const MIN_ROWS: usize = 64;

#[derive(Clone, Copy, PartialEq)]
enum Func {
    Sin,
    Cos,
    Tan,
    Sinh,
    Cosh,
    Tanh,
    Exp,
    Log,
    Log10,
    Log2,
    Sqrt,
    Atan,
    Abs,
    Round,
    Floor,
    Ceil,
}

impl Func {
    fn parse(name: &str) -> Option<Func> {
        Some(match name {
            "sin" => Func::Sin,
            "cos" => Func::Cos,
            "tan" => Func::Tan,
            "sinh" => Func::Sinh,
            "cosh" => Func::Cosh,
            "tanh" => Func::Tanh,
            "exp" => Func::Exp,
            "log" => Func::Log,
            "log10" => Func::Log10,
            "log2" => Func::Log2,
            "sqrt" => Func::Sqrt,
            "atan" => Func::Atan,
            "abs" => Func::Abs,
            "round" => Func::Round,
            "floor" => Func::Floor,
            "ceil" => Func::Ceil,
            _ => return None,
        })
    }

    /// True for the functions that leave the real line for some real inputs.
    fn has_domain(self) -> bool {
        matches!(self, Func::Log | Func::Log10 | Func::Log2 | Func::Sqrt)
    }

    #[inline]
    fn apply(self, x: f64) -> f64 {
        match self {
            Func::Sin => x.sin(),
            Func::Cos => x.cos(),
            Func::Tan => x.tan(),
            Func::Sinh => x.sinh(),
            Func::Cosh => x.cosh(),
            Func::Tanh => x.tanh(),
            Func::Exp => x.exp(),
            Func::Log => x.ln(),
            Func::Log10 => x.log10(),
            Func::Log2 => x.log2(),
            Func::Sqrt => x.sqrt(),
            Func::Atan => x.atan(),
            Func::Abs => x.abs(),
            Func::Round => x.round(),
            Func::Floor => x.floor(),
            Func::Ceil => x.ceil(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Bop {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
    Ne,
    And,
    Or,
}

impl Bop {
    /// Comparisons and the logical operators: the result is logical (0 or 1).
    fn is_logical(self) -> bool {
        matches!(self, Bop::Gt | Bop::Lt | Bop::Ge | Bop::Le | Bop::Eq | Bop::Ne | Bop::And | Bop::Or)
    }

    /// `&` and `|` refuse NaN operands.
    fn rejects_nan(self) -> bool {
        matches!(self, Bop::And | Bop::Or)
    }
}

enum Node {
    Const(f64),
    /// Index into the list of variables.
    Leaf(usize),
    Neg(usize),
    Not(usize),
    Bin(Bop, usize, usize),
    Call(Func, usize),
}

struct Plan {
    nodes: Vec<Node>,
    vars: Vec<String>,
    ops: usize,
    /// Matrix products and divisions (node, scalar operand may be on either side): elementwise
    /// only when the sizes turn out to make them so.
    markers: Vec<(usize, bool)>,
}

fn disabled() -> bool {
    static OFF: OnceLock<bool> = OnceLock::new();
    *OFF.get_or_init(|| std::env::var_os("MATLIVRE_NOFUSE").is_some())
}

fn bop_of(op: BinOp) -> Option<Bop> {
    Some(match op {
        BinOp::Add => Bop::Add,
        BinOp::Sub => Bop::Sub,
        BinOp::Mul | BinOp::MatMul => Bop::Mul,
        BinOp::Div | BinOp::MatDiv => Bop::Div,
        BinOp::Pow => Bop::Pow,
        BinOp::Gt => Bop::Gt,
        BinOp::Lt => Bop::Lt,
        BinOp::Ge => Bop::Ge,
        BinOp::Le => Bop::Le,
        BinOp::Eq => Bop::Eq,
        BinOp::Ne => Bop::Ne,
        BinOp::And => Bop::And,
        BinOp::Or => Bop::Or,
        _ => return None,
    })
}

/// Appends the nodes of `e`; `None` when `e` is not a fusable expression.
fn build(e: &Expr, it: &Interpreter, plan: &mut Plan) -> Option<usize> {
    let node = match e {
        Expr::Num(n) => Node::Const(*n),
        Expr::Ident(name) if it.get_var(name).is_none() => {
            // A few constants that are written as function calls without arguments.
            if it.user_function_defined(name) {
                return None;
            }
            Node::Const(match name.as_str() {
                "pi" => std::f64::consts::PI,
                "Inf" | "inf" => f64::INFINITY,
                "NaN" | "nan" => f64::NAN,
                "eps" => f64::EPSILON,
                _ => return None,
            })
        }
        Expr::Ident(name) => {
            let id = match plan.vars.iter().position(|v| v == name) {
                Some(i) => i,
                None => {
                    plan.vars.push(name.clone());
                    plan.vars.len() - 1
                }
            };
            Node::Leaf(id)
        }
        Expr::Unary(UnOp::Plus, x) => return build(x, it, plan),
        Expr::Unary(UnOp::Neg, x) => {
            let a = build(x, it, plan)?;
            plan.ops += 1;
            Node::Neg(a)
        }
        Expr::Unary(UnOp::Not, x) => {
            let a = build(x, it, plan)?;
            plan.ops += 1;
            Node::Not(a)
        }
        Expr::Binary(op, l, r) => {
            // `*` and `/` are elementwise only with a scalar operand, which is checked once the
            // sizes are known; the matrix products are left to the ordinary evaluator.
            let bop = bop_of(*op)?;
            let a = build(l, it, plan)?;
            let b = build(r, it, plan)?;
            plan.ops += 1;
            if matches!(op, BinOp::MatMul | BinOp::MatDiv) {
                plan.markers.push((plan.nodes.len(), *op == BinOp::MatMul));
            }
            Node::Bin(bop, a, b)
        }
        Expr::Index(base, args) if args.len() == 1 => {
            let Expr::Ident(name) = base.as_ref() else { return None };
            let f = Func::parse(name)?;
            if it.get_var(name).is_some() || it.user_function_defined(name) {
                return None;
            }
            let a = build(&args[0], it, plan)?;
            plan.ops += 1;
            Node::Call(f, a)
        }
        _ => return None,
    };
    plan.nodes.push(node);
    Some(plan.nodes.len() - 1)
}

/// Cheap structural test, before any variable is looked up: enough operators, and only the
/// node kinds that `build` accepts.
fn shape_ok(e: &Expr, ops: &mut usize) -> bool {
    match e {
        Expr::Num(_) | Expr::Ident(_) => true,
        Expr::Unary(UnOp::Plus, x) => shape_ok(x, ops),
        Expr::Unary(UnOp::Neg | UnOp::Not, x) => {
            *ops += 1;
            shape_ok(x, ops)
        }
        Expr::Binary(op, l, r) => {
            if bop_of(*op).is_none() {
                return false;
            }
            *ops += 1;
            shape_ok(l, ops) && shape_ok(r, ops)
        }
        Expr::Index(base, args) if args.len() == 1 => match base.as_ref() {
            Expr::Ident(name) if Func::parse(name).is_some() => {
                *ops += 1;
                shape_ok(&args[0], ops)
            }
            _ => false,
        },
        _ => false,
    }
}

/// What the fused evaluation produces.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// The array itself.
    Array,
    /// `find` of the array: the 1-based positions of its nonzero elements.
    Find,
}

/// Evaluates `e` fused, or returns `None` to let the ordinary evaluator handle it.
pub fn try_eval(it: &Interpreter, e: &Expr) -> Option<Value> {
    if disabled() {
        return None;
    }
    let (inner, mode, min_ops) = match e {
        Expr::Index(base, args) if args.len() == 1 && matches!(base.as_ref(), Expr::Ident(n) if n == "find") => {
            if it.get_var("find").is_some() || it.user_function_defined("find") {
                return None;
            }
            (&args[0], Mode::Find, 1)
        }
        _ => (e, Mode::Array, MIN_OPS),
    };
    let mut ops = 0;
    if !shape_ok(inner, &mut ops) || ops < min_ops {
        return None;
    }
    let mut plan = Plan { nodes: Vec::new(), vars: Vec::new(), ops: 0, markers: Vec::new() };
    let root = build(inner, it, &mut plan)?;
    if plan.ops < min_ops {
        return None;
    }
    run(it, &plan, root, mode)
}

/// How a node's values are laid out over the result.
#[derive(Clone, Copy, PartialEq)]
enum Class {
    /// One value for every element.
    Scalar,
    /// One value per column (a row vector expanded down the rows).
    Row,
    /// One value per row (a column vector expanded across the columns).
    Col,
    /// One value per element.
    Full,
}

fn combine(a: Class, b: Class) -> Class {
    match (a, b) {
        (Class::Scalar, x) | (x, Class::Scalar) => x,
        (Class::Full, _) | (_, Class::Full) => Class::Full,
        (Class::Row, Class::Row) => Class::Row,
        (Class::Col, Class::Col) => Class::Col,
        _ => Class::Full,
    }
}

/// Everything the block evaluator reads.
struct Ctx<'a> {
    nodes: &'a [Node],
    root: usize,
    mats: Vec<&'a Matrix>,
    class: Vec<Class>,
    /// Nodes whose value is logical (0 or 1), so known not to be NaN.
    logical: Vec<bool>,
    cval: Vec<f64>,
    /// Values of the nodes of class Row or Col that are not variables.
    small: Vec<Vec<f64>>,
    rows: usize,
    broadcast: bool,
}

impl Ctx<'_> {
    /// The per-column or per-row values of a node of class Row or Col.
    fn vals(&self, j: usize) -> &[f64] {
        if let Node::Leaf(id) = self.nodes[j] {
            &self.mats[id].data
        } else {
            &self.small[j]
        }
    }
}

fn run(it: &Interpreter, plan: &Plan, root: usize, mode: Mode) -> Option<Value> {
    let nodes = &plan.nodes[..];
    // The variables: real double arrays or scalars.
    let mut mats: Vec<&Matrix> = Vec::with_capacity(plan.vars.len());
    for name in &plan.vars {
        match it.get_var(name)? {
            Value::Num(m) if m.kind == Kind::Double && m.im.is_none() && !m.sparse => mats.push(m),
            _ => return None,
        }
    }
    // The shape of the result: all arrays of one size, or 2-D rows and columns that expand.
    let mut dims: Option<&Vec<usize>> = None;
    let mut same = true;
    for m in &mats {
        if m.numel() != 1 {
            match dims {
                None => dims = Some(&m.dims),
                Some(d) if *d == m.dims => {}
                _ => same = false,
            }
        }
    }
    let first = dims?;
    let (mut rows, mut cols) = (0usize, 0usize);
    let mut out_dims = first.clone();
    let mut broadcast = false;
    if !same {
        if mats.iter().any(|m| m.numel() != 1 && m.dims.len() != 2) {
            return None;
        }
        for m in mats.iter().filter(|m| m.numel() != 1) {
            rows = rows.max(m.rows());
            cols = cols.max(m.cols());
        }
        if mats.iter().any(|m| m.numel() != 1 && !((m.rows() == rows || m.rows() == 1) && (m.cols() == cols || m.cols() == 1))) {
            return None;
        }
        if rows < MIN_ROWS {
            return None;
        }
        out_dims = vec![rows, cols];
        broadcast = true;
    }
    let n: usize = out_dims.iter().product();
    if n < PAR_MIN {
        return None;
    }
    let var_class = |m: &Matrix| -> Class {
        if m.numel() == 1 {
            Class::Scalar
        } else if !broadcast || (m.rows() == rows && m.cols() == cols) {
            Class::Full
        } else if m.rows() == 1 {
            Class::Row
        } else {
            Class::Col
        }
    };
    let mut class = vec![Class::Scalar; nodes.len()];
    for (k, node) in nodes.iter().enumerate() {
        class[k] = match node {
            Node::Const(_) => Class::Scalar,
            Node::Leaf(id) => var_class(mats[*id]),
            Node::Neg(a) | Node::Not(a) | Node::Call(_, a) => class[*a],
            Node::Bin(_, a, b) => combine(class[*a], class[*b]),
        };
    }
    let scalar: Vec<bool> = class.iter().map(|c| *c == Class::Scalar).collect();
    let mut logical = vec![false; nodes.len()];
    for (k, node) in nodes.iter().enumerate() {
        logical[k] = match node {
            Node::Not(_) => true,
            Node::Bin(op, _, _) => op.is_logical(),
            _ => false,
        };
    }
    for &(at, either) in &plan.markers {
        let Node::Bin(_, a, b) = &nodes[at] else { return None };
        // `A*B` is elementwise with a scalar on either side; `A/B` with a scalar divisor.
        if !(scalar[*b] || (either && scalar[*a])) {
            return None;
        }
    }
    // A power must have a scalar exponent: whether the result is real then depends on one value.
    for node in nodes {
        if let Node::Bin(Bop::Pow, _, b) = node {
            if !scalar[*b] {
                return None;
            }
        }
    }
    // Constant subtrees are folded once.
    let mut cval = vec![0.0f64; nodes.len()];
    for (k, node) in nodes.iter().enumerate() {
        if !scalar[k] {
            continue;
        }
        cval[k] = match node {
            Node::Const(c) => *c,
            Node::Leaf(id) => mats[*id].data[0],
            Node::Neg(a) => -cval[*a],
            Node::Not(a) => {
                if !logical[*a] && cval[*a].is_nan() {
                    return None;
                }
                bool_f64(cval[*a] == 0.0)
            }
            Node::Call(f, a) => {
                if f.has_domain() && cval[*a] < 0.0 {
                    return None;
                }
                f.apply(cval[*a])
            }
            Node::Bin(op, a, b) => {
                let (x, y) = (cval[*a], cval[*b]);
                if *op == Bop::Pow && x < 0.0 && y.fract() != 0.0 && y.is_finite() {
                    return None;
                }
                if op.rejects_nan() && ((!logical[*a] && x.is_nan()) || (!logical[*b] && y.is_nan())) {
                    return None;
                }
                scalar_bin(*op, x, y)
            }
        };
    }
    if class[root] != Class::Full {
        return None;
    }
    // Nodes that depend only on rows or only on columns are computed once, outside the blocks.
    let mut small: Vec<Vec<f64>> = vec![Vec::new(); nodes.len()];
    if broadcast {
        for k in 0..nodes.len() {
            if !matches!(class[k], Class::Row | Class::Col) || matches!(nodes[k], Node::Leaf(_)) {
                continue;
            }
            let len = if class[k] == Class::Row { cols } else { rows };
            let v = {
                let vals_of = |j: usize| -> Option<&[f64]> {
                    match class[j] {
                        Class::Row | Class::Col => Some(if let Node::Leaf(id) = nodes[j] { &mats[id].data[..] } else { &small[j][..] }),
                        _ => None,
                    }
                };
                let get = |j: usize, t: usize| -> f64 { vals_of(j).map_or(cval[j], |v| v[t]) };
                let mut v = Vec::with_capacity(len);
                for t in 0..len {
                    v.push(match &nodes[k] {
                        Node::Neg(a) => -get(*a, t),
                        Node::Not(a) => {
                            let x = get(*a, t);
                            if !logical[*a] && x.is_nan() {
                                return None;
                            }
                            bool_f64(x == 0.0)
                        }
                        Node::Call(f, a) => {
                            let x = get(*a, t);
                            if f.has_domain() && x < 0.0 {
                                return None;
                            }
                            f.apply(x)
                        }
                        Node::Bin(op, a, b) => {
                            let (x, y) = (get(*a, t), get(*b, t));
                            if *op == Bop::Pow && x < 0.0 && y.fract() != 0.0 && y.is_finite() {
                                return None;
                            }
                            if op.rejects_nan() && ((!logical[*a] && x.is_nan()) || (!logical[*b] && y.is_nan())) {
                                return None;
                            }
                            scalar_bin(*op, x, y)
                        }
                        Node::Const(_) | Node::Leaf(_) => unreachable!(),
                    });
                }
                v
            };
            small[k] = v;
        }
    }
    let cx = Ctx { nodes, root, mats, class, logical, cval, small, rows, broadcast };
    let abort = AtomicBool::new(false);
    match mode {
        Mode::Array => {
            let mut out = vec![0.0f64; n];
            out.par_chunks_mut(BLOCK).enumerate().for_each_init(
                || Scratch::new(nodes.len()),
                |sc, (ci, chunk)| {
                    if abort.load(Ordering::Relaxed) {
                        return;
                    }
                    if !eval_block(&cx, ci * BLOCK, sc, chunk) {
                        abort.store(true, Ordering::Relaxed);
                    }
                },
            );
            if abort.load(Ordering::Relaxed) {
                return None;
            }
            let m = Matrix::from_dims(out_dims, out);
            Some(Value::Num(if cx.logical[root] { m.with_kind(Kind::Logical) } else { m }))
        }
        Mode::Find => {
            let blocks = n.div_ceil(BLOCK);
            let parts: Vec<Vec<f64>> = (0..blocks)
                .into_par_iter()
                .map_init(
                    || (Scratch::new(nodes.len()), vec![0.0f64; BLOCK]),
                    |(sc, buf), ci| {
                        if abort.load(Ordering::Relaxed) {
                            return Vec::new();
                        }
                        let start = ci * BLOCK;
                        let len = BLOCK.min(n - start);
                        if !eval_block(&cx, start, sc, &mut buf[..len]) {
                            abort.store(true, Ordering::Relaxed);
                            return Vec::new();
                        }
                        let mut hits = Vec::new();
                        for (t, &v) in buf[..len].iter().enumerate() {
                            if v != 0.0 {
                                hits.push((start + t + 1) as f64);
                            }
                        }
                        hits
                    },
                )
                .collect();
            if abort.load(Ordering::Relaxed) {
                return None;
            }
            let total: usize = parts.iter().map(|p| p.len()).sum();
            let mut idx = Vec::with_capacity(total);
            for p in parts {
                idx.extend(p);
            }
            // A row gives a row, anything else a column.
            let d = if out_dims.len() == 2 && out_dims[0] == 1 { vec![1, total] } else { vec![total, 1] };
            Some(Value::Num(Matrix::from_dims(d, idx)))
        }
    }
}

#[inline]
fn bool_f64(b: bool) -> f64 {
    if b {
        1.0
    } else {
        0.0
    }
}

#[inline]
fn scalar_bin(op: Bop, x: f64, y: f64) -> f64 {
    match op {
        Bop::Add => x + y,
        Bop::Sub => x - y,
        Bop::Mul => x * y,
        Bop::Div => x / y,
        // The ordinary evaluator squares with a multiplication.
        Bop::Pow => {
            if y == 2.0 {
                x * x
            } else {
                x.powf(y)
            }
        }
        Bop::Gt => bool_f64(x > y),
        Bop::Lt => bool_f64(x < y),
        Bop::Ge => bool_f64(x >= y),
        Bop::Le => bool_f64(x <= y),
        Bop::Eq => bool_f64(x == y),
        Bop::Ne => bool_f64(x != y),
        Bop::And => bool_f64(x != 0.0 && y != 0.0),
        Bop::Or => bool_f64(x != 0.0 || y != 0.0),
    }
}

/// An operand of a block: one value for all elements, or a slice.
#[derive(Clone, Copy)]
enum Opd<'a> {
    C(f64),
    S(&'a [f64]),
}

#[inline]
fn zip2(out: &mut [f64], a: Opd, b: Opd, f: impl Fn(f64, f64) -> f64) {
    match (a, b) {
        (Opd::S(x), Opd::S(y)) => {
            for ((o, &p), &q) in out.iter_mut().zip(x).zip(y) {
                *o = f(p, q);
            }
        }
        (Opd::S(x), Opd::C(c)) => {
            for (o, &p) in out.iter_mut().zip(x) {
                *o = f(p, c);
            }
        }
        (Opd::C(c), Opd::S(y)) => {
            for (o, &q) in out.iter_mut().zip(y) {
                *o = f(c, q);
            }
        }
        (Opd::C(c), Opd::C(d)) => out.fill(f(c, d)),
    }
}

/// Per-thread buffers: one block of values for every node, and the segments of the block.
struct Scratch {
    bufs: Vec<Vec<f64>>,
    /// `(column, first row, end row, offset in the block)` of each run of one column.
    segs: Vec<(usize, usize, usize, usize)>,
}

impl Scratch {
    fn new(nodes: usize) -> Scratch {
        Scratch { bufs: vec![vec![0.0f64; BLOCK]; nodes], segs: Vec::new() }
    }
}

/// Evaluates all nodes for the elements `start..start + chunk.len()`, writing the root into
/// `chunk`. Returns false when the block needs a complex result or a logical operator meets NaN.
fn eval_block(cx: &Ctx, start: usize, sc: &mut Scratch, chunk: &mut [f64]) -> bool {
    let len = chunk.len();
    let Scratch { bufs, segs } = sc;
    segs.clear();
    if cx.broadcast {
        // Runs inside one column, so a row operand is a constant and a column operand a slice.
        let end = start + len;
        let mut pos = start;
        while pos < end {
            let (c, r0) = (pos / cx.rows, pos % cx.rows);
            let r1 = (r0 + (end - pos)).min(cx.rows);
            segs.push((c, r0, r1, pos - start));
            pos += r1 - r0;
        }
    } else {
        segs.push((0, 0, len, 0));
    }
    for k in 0..=cx.root {
        if cx.class[k] != Class::Full || matches!(cx.nodes[k], Node::Leaf(_)) {
            continue;
        }
        let (before, rest) = bufs.split_at_mut(k);
        let before: &[Vec<f64>] = before;
        let out_all: &mut [f64] = if k == cx.root { &mut *chunk } else { &mut rest[0][..len] };
        for &(c, r0, r1, off) in segs.iter() {
            let sl = r1 - r0;
            let operand = |j: usize| -> Opd {
                match cx.class[j] {
                    Class::Scalar => Opd::C(cx.cval[j]),
                    Class::Row => Opd::C(cx.vals(j)[c]),
                    Class::Col => Opd::S(&cx.vals(j)[r0..r1]),
                    Class::Full => {
                        if let Node::Leaf(id) = cx.nodes[j] {
                            Opd::S(&cx.mats[id].data[start + off..start + off + sl])
                        } else {
                            Opd::S(&before[j][off..off + sl])
                        }
                    }
                }
            };
            if !apply(cx, k, &mut out_all[off..off + sl], &operand) {
                return false;
            }
        }
    }
    true
}

/// True when the operand holds a NaN (a logical operand never does).
fn has_nan(o: Opd) -> bool {
    match o {
        Opd::C(c) => c.is_nan(),
        Opd::S(s) => s.iter().any(|p| p.is_nan()),
    }
}

/// One node over one run of elements.
fn apply<'a>(cx: &Ctx, k: usize, out: &mut [f64], operand: &dyn Fn(usize) -> Opd<'a>) -> bool {
    match &cx.nodes[k] {
        Node::Neg(a) => match operand(*a) {
            Opd::S(x) => {
                for (o, &p) in out.iter_mut().zip(x) {
                    *o = -p;
                }
            }
            Opd::C(c) => out.fill(-c),
        },
        Node::Not(a) => {
            let x = operand(*a);
            if !cx.logical[*a] && has_nan(x) {
                return false;
            }
            zip2(out, x, Opd::C(0.0), |p, _| bool_f64(p == 0.0));
        }
        Node::Call(f, a) => {
            let Opd::S(x) = operand(*a) else { return true };
            if f.has_domain() && x.iter().any(|&p| p < 0.0) {
                return false;
            }
            match f {
                Func::Sin => map1(out, x, f64::sin),
                Func::Cos => map1(out, x, f64::cos),
                Func::Tan => map1(out, x, f64::tan),
                Func::Sinh => map1(out, x, f64::sinh),
                Func::Cosh => map1(out, x, f64::cosh),
                Func::Tanh => map1(out, x, f64::tanh),
                Func::Exp => map1(out, x, f64::exp),
                Func::Log => map1(out, x, f64::ln),
                Func::Log10 => map1(out, x, f64::log10),
                Func::Log2 => map1(out, x, f64::log2),
                Func::Sqrt => map1(out, x, f64::sqrt),
                Func::Atan => map1(out, x, f64::atan),
                Func::Abs => map1(out, x, f64::abs),
                Func::Round => map1(out, x, f64::round),
                Func::Floor => map1(out, x, f64::floor),
                Func::Ceil => map1(out, x, f64::ceil),
            }
        }
        Node::Bin(op, a, b) => {
            let (x, y) = (operand(*a), operand(*b));
            if op.rejects_nan() && ((!cx.logical[*a] && has_nan(x)) || (!cx.logical[*b] && has_nan(y))) {
                return false;
            }
            match op {
                Bop::Add => zip2(out, x, y, |p, q| p + q),
                Bop::Sub => zip2(out, x, y, |p, q| p - q),
                Bop::Mul => zip2(out, x, y, |p, q| p * q),
                Bop::Div => zip2(out, x, y, |p, q| p / q),
                Bop::Gt => zip2(out, x, y, |p, q| bool_f64(p > q)),
                Bop::Lt => zip2(out, x, y, |p, q| bool_f64(p < q)),
                Bop::Ge => zip2(out, x, y, |p, q| bool_f64(p >= q)),
                Bop::Le => zip2(out, x, y, |p, q| bool_f64(p <= q)),
                Bop::Eq => zip2(out, x, y, |p, q| bool_f64(p == q)),
                Bop::Ne => zip2(out, x, y, |p, q| bool_f64(p != q)),
                Bop::And => zip2(out, x, y, |p, q| bool_f64(p != 0.0 && q != 0.0)),
                Bop::Or => zip2(out, x, y, |p, q| bool_f64(p != 0.0 || q != 0.0)),
                Bop::Pow => {
                    // The exponent is a scalar (checked when planning).
                    let Opd::C(e) = y else { return false };
                    if e == 2.0 {
                        zip2(out, x, y, |p, _| p * p);
                    } else {
                        if e.fract() != 0.0 && e.is_finite() {
                            if let Opd::S(s) = x {
                                if s.iter().any(|&p| p < 0.0) {
                                    return false;
                                }
                            }
                        }
                        zip2(out, x, y, |p, q| p.powf(q));
                    }
                }
            }
        }
        Node::Const(_) | Node::Leaf(_) => {}
    }
    true
}

#[inline]
fn map1(out: &mut [f64], x: &[f64], f: impl Fn(f64) -> f64) {
    for (o, &p) in out.iter_mut().zip(x) {
        *o = f(p);
    }
}
