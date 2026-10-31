//! Symbolic Math: `sym` arrays, operators, display and toolbox functions.

pub mod algebra;
pub mod calculus;
pub mod expr;
pub mod extra;
pub mod mp;
pub mod print;

use std::cell::RefCell;
use std::collections::HashSet;

pub use expr::Expr;
use expr::{Const, Rel};

use crate::ast::{BinOp, UnOp};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::nd::{self, Dims};
use crate::value::*;

type R<T> = Result<T, LangError>;

thread_local! {
    /// Variables declared `real` (or positive) with `syms x real` / `assume`.
    static REAL_VARS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

fn is_real_var(v: &str) -> bool {
    REAL_VARS.with(|r| r.borrow().contains(v))
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymArray {
    pub dims: Dims,
    pub data: Vec<Expr>,
    /// The input arguments of a symbolic function (`f(x, y) = ...`, class symfun); empty for
    /// a plain sym array.
    pub args: Vec<String>,
}

impl SymArray {
    pub fn empty() -> Self {
        Self { dims: vec![0, 0], data: Vec::new(), args: Vec::new() }
    }

    pub fn scalar(e: Expr) -> Self {
        Self { dims: vec![1, 1], data: vec![e], args: Vec::new() }
    }

    pub fn from_dims(dims: Dims, data: Vec<Expr>) -> Self {
        Self { dims: nd::normalize(dims), data, args: Vec::new() }
    }

    pub fn from_value(v: &Value) -> R<SymArray> {
        Ok(match v {
            Value::Sym(s) => s.clone(),
            Value::Num(m) if m.kind == Kind::Char => SymArray::scalar(parse_sym_text(&m.to_string_lossy())?),
            Value::Num(m) => SymArray::from_dims(
                m.dims.clone(),
                (0..m.numel())
                    .map(|i| {
                        let re = Expr::from_f64(m.data[i]);
                        let im = m.im_at(i);
                        if im == 0.0 { re } else { Expr::add(vec![re, Expr::mul(vec![Expr::from_f64(im), Expr::Const(Const::I)])]) }
                    })
                    .collect(),
            ),
            Value::Str(s) if s.is_scalar() => SymArray::scalar(parse_sym_text(s.data[0].as_deref().unwrap_or(""))?),
            Value::Cell(c) => {
                let parts = c.data.iter().map(SymArray::from_value).collect::<R<Vec<_>>>()?;
                if parts.iter().all(|p| p.data.len() == 1) {
                    SymArray::from_dims(c.dims.clone(), parts.into_iter().map(|p| p.data.into_iter().next().unwrap()).collect())
                } else {
                    return rt("Conversion to sym from a cell array of non-scalars is not possible.");
                }
            }
            other => return rt(format!("Conversion to sym from {} is not possible.", other.class_name())),
        })
    }

    pub fn gather(&self, pos: &[usize], dims: Dims) -> SymArray {
        SymArray::from_dims(dims, pos.iter().map(|&p| self.data[p].clone()).collect())
    }

    pub fn map(&self, f: impl Fn(&Expr) -> Expr) -> SymArray {
        SymArray { dims: self.dims.clone(), data: self.data.iter().map(f).collect(), args: self.args.clone() }
    }

    /// `double(s)`.
    pub fn to_matrix(&self) -> R<Matrix> {
        let mut re = Vec::with_capacity(self.data.len());
        let mut im = Vec::with_capacity(self.data.len());
        for e in &self.data {
            // Real values are evaluated exactly and rounded once, as MATLAB's double(sym) does.
            if let Some(x) = mp::to_f64(e) {
                re.push(x);
                im.push(0.0);
                continue;
            }
            let z = e.eval_c(&|_| None).ok_or_else(|| LangError::new("Unable to convert expression containing symbolic variables into double array. Apply 'subs' function first to substitute values for variables.", 0))?;
            re.push(z.re);
            im.push(z.im);
        }
        Ok(Matrix::complex(self.dims.clone(), re, im))
    }

    /// Truth of a symbolic condition (`if x == y`): decidable relations only.
    pub fn truth(&self) -> R<bool> {
        for e in &self.data {
            let ok = match e {
                Expr::Rel(op, l, r) => {
                    let d = algebra::simplify(&Expr::sub((**l).clone(), (**r).clone()));
                    match (op, d.to_f64()) {
                        (Rel::Eq, _) if d.is_zero() => true,
                        (Rel::Ne, _) if d.is_zero() => false,
                        (_, Some(x)) => match op {
                            Rel::Eq => x == 0.0,
                            Rel::Ne => x != 0.0,
                            Rel::Lt => x < 0.0,
                            Rel::Le => x <= 0.0,
                            Rel::Gt => x > 0.0,
                            Rel::Ge => x >= 0.0,
                        },
                        (Rel::Eq, None) => false,
                        (Rel::Ne, None) => true,
                        _ => return rt("Unable to prove the condition. Use 'isAlways' to test the statement mathematically."),
                    }
                }
                other => match other.to_f64() {
                    Some(x) => x != 0.0,
                    None => return rt("Unable to evaluate a symbolic expression as a logical value."),
                },
            };
            if !ok {
                return Ok(false);
            }
        }
        Ok(!self.data.is_empty())
    }

    pub fn summary(&self) -> String {
        if self.data.len() == 1 {
            let s = self.data[0].to_string();
            if s.chars().count() > 48 { format!("1×1 sym") } else { s }
        } else {
            format!("{} sym", nd::size_text(&self.dims))
        }
    }

    fn rows_text(&self) -> String {
        if self.dims.len() > 2 {
            return format!("{} sym array", nd::size_text(&self.dims));
        }
        let (r, c) = (self.dims[0], self.dims[1]);
        let cells: Vec<String> = self.data.iter().map(|e| e.to_string()).collect();
        // Column vectors print one element per line (MATLAB's sym display).
        if c == 1 {
            // Right-aligned, as MATLAB lines up the elements.
            let w = cells.iter().map(|s| s.chars().count()).max().unwrap_or(0);
            return cells.iter().map(|s| format!("{s:>w$}")).collect::<Vec<_>>().join("\n");
        }
        let widths: Vec<usize> = (0..c).map(|j| (0..r).map(|i| cells[j * r + i].chars().count()).max().unwrap_or(0)).collect();
        (0..r)
            .map(|i| {
                let items: Vec<String> = (0..c).map(|j| format!("{:>w$}", cells[j * r + i], w = widths[j])).collect();
                format!("[{}]", items.join(", "))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn display(&self, name: &str) -> String {
        // A symbolic function shows its signature: f(x, y) = ...
        let signature;
        let name = if self.args.is_empty() {
            name
        } else {
            signature = format!("{name}({})", self.args.join(", "));
            &signature
        };
        if self.data.len() == 1 {
            return format!("{name} = {}", self.data[0]);
        }
        if self.data.is_empty() {
            return format!("{name} = \n[ empty sym ]");
        }
        format!("{name} = \n{}", self.rows_text())
    }

    pub fn disp(&self) -> String {
        if self.data.len() == 1 {
            return self.data[0].to_string();
        }
        self.rows_text()
    }
}

/// The symbolic variable named by a value, if it is exactly one bare variable (`syms x`).
pub fn bare_var(v: &Value) -> Option<String> {
    match v {
        Value::Sym(s) if s.data.len() == 1 && s.args.is_empty() => match &s.data[0] {
            Expr::Var(name) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// `f(x, y) = body`: the symbolic function with those inputs.
pub fn define_symfun(args: Vec<String>, body: &Value) -> R<Value> {
    let mut f = SymArray::from_value(body)?;
    f.args = args;
    Ok(Value::Sym(f))
}

/// `f(a, b)` for a symbolic function: its body with the inputs replaced by the arguments,
/// as a sym (not a symfun). An array argument evaluates the function elementwise.
pub fn call_symfun(f: &SymArray, args: Vec<Value>) -> R<Value> {
    if args.len() != f.args.len() {
        return rt(format!("Symbolic function expected {} input arguments but received {}.", f.args.len(), args.len()));
    }
    let vals = args.iter().map(SymArray::from_value).collect::<R<Vec<_>>>()?;
    let n = vals.iter().map(|v| v.data.len()).max().unwrap_or(1);
    if vals.iter().any(|v| v.data.len() != 1 && v.data.len() != n) {
        return rt("Arguments of a symbolic function call must be scalars or arrays of the same size.");
    }
    // Inputs are first renamed to placeholders, so f(y, x) swaps rather than chains.
    let hold: Vec<String> = (0..f.args.len()).map(|k| format!("\u{0}arg{k}")).collect();
    let renamed = f.map(|e| f.args.iter().zip(&hold).fold(e.clone(), |acc, (a, h)| acc.subs(a, &Expr::var(h))));
    let at = |e: &Expr, k: usize| hold.iter().zip(&vals).fold(e.clone(), |acc, (h, v)| acc.subs(h, &v.data[if v.data.len() == 1 { 0 } else { k }]));
    let mut out = if f.data.len() == 1 && n > 1 {
        let dims = vals.iter().find(|v| v.data.len() == n).unwrap().dims.clone();
        SymArray::from_dims(dims, (0..n).map(|k| at(&renamed.data[0], k)).collect())
    } else {
        renamed.map(|e| at(e, 0))
    };
    out.args.clear();
    Ok(Value::Sym(out))
}

/// Parses text such as `'x^2 + 1'` into a symbolic expression.
pub(crate) fn parse_sym_text(text: &str) -> R<Expr> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(Expr::zero());
    }
    if t.chars().all(|c| c.is_alphanumeric() || c == '_') && t.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
        return Ok(match t {
            "pi" => Expr::Const(Const::Pi),
            "Inf" | "inf" => Expr::Const(Const::Inf),
            "NaN" | "nan" => Expr::Const(Const::NaN),
            "i" | "j" => Expr::Const(Const::I),
            _ => Expr::var(t),
        });
    }
    let ast = crate::parser::parse_expr(t)?;
    ast_to_sym(&ast)
}

/// Converts a parsed expression (numbers, names, operators, calls) to a symbolic one.
pub(crate) fn ast_to_sym(e: &crate::ast::Expr) -> R<Expr> {
    use crate::ast::Expr as A;
    Ok(match e {
        A::Num(n) => Expr::from_f64(*n),
        A::Imag(n) => Expr::mul(vec![Expr::from_f64(*n), Expr::Const(Const::I)]),
        A::Ident(n) => parse_sym_text(n)?,
        A::Unary(UnOp::Neg, x) => Expr::neg(ast_to_sym(x)?),
        A::Unary(UnOp::Plus, x) => ast_to_sym(x)?,
        A::Binary(op, a, b) => {
            let (x, y) = (ast_to_sym(a)?, ast_to_sym(b)?);
            match op {
                BinOp::Add => Expr::add(vec![x, y]),
                BinOp::Sub => Expr::sub(x, y),
                BinOp::Mul | BinOp::MatMul => Expr::mul(vec![x, y]),
                BinOp::Div | BinOp::MatDiv => Expr::div(x, y),
                BinOp::Pow | BinOp::MatPow => Expr::pow(x, y),
                BinOp::Eq => Expr::rel(Rel::Eq, x, y),
                BinOp::Ne => Expr::rel(Rel::Ne, x, y),
                BinOp::Lt => Expr::rel(Rel::Lt, x, y),
                BinOp::Le => Expr::rel(Rel::Le, x, y),
                BinOp::Gt => Expr::rel(Rel::Gt, x, y),
                BinOp::Ge => Expr::rel(Rel::Ge, x, y),
                _ => return rt("Unsupported operator in symbolic text."),
            }
        }
        A::Index(f, args) => {
            let A::Ident(name) = f.as_ref() else { return rt("Invalid symbolic expression.") };
            let a = args.iter().map(ast_to_sym).collect::<R<Vec<_>>>()?;
            Expr::func(name, a)
        }
        _ => return rt("Invalid symbolic expression."),
    })
}

// ---- operators ------------------------------------------------------------------------------

fn elementwise(a: &SymArray, b: &SymArray, f: impl Fn(&Expr, &Expr) -> Expr) -> R<SymArray> {
    let (dims, pairs) = nd::broadcast_pairs(&a.dims, &b.dims)?;
    Ok(SymArray::from_dims(dims, pairs.into_iter().map(|(i, j)| f(&a.data[i], &b.data[j])).collect()))
}

fn matmul(a: &SymArray, b: &SymArray) -> R<SymArray> {
    if a.dims.len() > 2 || b.dims.len() > 2 || a.dims[1] != b.dims[0] {
        return rt("Inner matrix dimensions must agree.");
    }
    let (n, m, p) = (a.dims[0], a.dims[1], b.dims[1]);
    let mut out = Vec::with_capacity(n * p);
    for j in 0..p {
        for i in 0..n {
            let terms = (0..m).map(|k| Expr::mul(vec![a.data[k * n + i].clone(), b.data[j * m + k].clone()])).collect();
            out.push(Expr::add(terms));
        }
    }
    Ok(SymArray::from_dims(vec![n, p], out))
}

fn sym_det(a: &SymArray) -> R<Expr> {
    if a.dims.len() != 2 || a.dims[0] != a.dims[1] {
        return rt("Matrix must be square.");
    }
    let n = a.dims[0];
    let at = |i: usize, j: usize| a.data[j * n + i].clone();
    Ok(match n {
        0 => Expr::one(),
        1 => at(0, 0),
        2 => Expr::sub(Expr::mul(vec![at(0, 0), at(1, 1)]), Expr::mul(vec![at(0, 1), at(1, 0)])),
        _ => {
            // Laplace expansion along the first row.
            let mut terms = Vec::new();
            for j in 0..n {
                let mut minor = Vec::new();
                for c in (0..n).filter(|&c| c != j) {
                    for r in 1..n {
                        minor.push(at(r, c));
                    }
                }
                let d = sym_det(&SymArray::from_dims(vec![n - 1, n - 1], minor))?;
                let sign = if j % 2 == 0 { Expr::one() } else { Expr::int(-1) };
                terms.push(Expr::mul(vec![sign, at(0, j), d]));
            }
            algebra::simplify(&Expr::add(terms))
        }
    })
}

fn sym_inv(a: &SymArray) -> R<SymArray> {
    let n = a.dims[0];
    if a.dims.len() != 2 || n != a.dims[1] {
        return rt("Matrix must be square.");
    }
    let d = sym_det(a)?;
    if algebra::simplify(&d).is_zero() {
        return rt("Matrix is singular.");
    }
    // Adjugate / determinant.
    let mut out = vec![Expr::zero(); n * n];
    for i in 0..n {
        for j in 0..n {
            let mut minor = Vec::new();
            for c in (0..n).filter(|&c| c != j) {
                for r in (0..n).filter(|&r| r != i) {
                    minor.push(a.data[c * n + r].clone());
                }
            }
            let m = if n == 1 { Expr::one() } else { sym_det(&SymArray::from_dims(vec![n - 1, n - 1], minor))? };
            let sign = if (i + j) % 2 == 0 { Expr::one() } else { Expr::int(-1) };
            // inverse(j, i) = cofactor(i, j) / det
            out[i * n + j] = algebra::simplify(&Expr::div(Expr::mul(vec![sign, m]), d.clone()));
        }
    }
    Ok(SymArray::from_dims(vec![n, n], out))
}

fn conj(e: &Expr) -> Expr {
    let mut vars = Vec::new();
    e.free_vars(&mut vars);
    if vars.iter().all(|v| is_real_var(v)) && !contains_i(e) {
        return e.clone();
    }
    if !vars.is_empty() {
        return Expr::Fn("conj".into(), vec![e.clone()]);
    }
    e.map(&|x| if *x == Expr::Const(Const::I) { Some(Expr::neg(Expr::Const(Const::I))) } else { None })
}

fn contains_i(e: &Expr) -> bool {
    match e {
        Expr::Const(Const::I) => true,
        Expr::Add(x) | Expr::Mul(x) | Expr::Fn(_, x) => x.iter().any(contains_i),
        Expr::Pow(a, b) | Expr::Rel(_, a, b) => contains_i(a) || contains_i(b),
        _ => false,
    }
}

pub fn binary(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    let x = SymArray::from_value(a)?;
    let y = SymArray::from_value(b)?;
    let scalar = x.data.len() == 1 || y.data.len() == 1;
    let out = match op {
        BinOp::Add => elementwise(&x, &y, |p, q| Expr::add(vec![p.clone(), q.clone()]))?,
        BinOp::Sub => elementwise(&x, &y, |p, q| Expr::sub(p.clone(), q.clone()))?,
        BinOp::Mul => elementwise(&x, &y, |p, q| Expr::mul(vec![p.clone(), q.clone()]))?,
        BinOp::Div => elementwise(&x, &y, |p, q| Expr::div(p.clone(), q.clone()))?,
        BinOp::LeftDiv => elementwise(&x, &y, |p, q| Expr::div(q.clone(), p.clone()))?,
        BinOp::Pow => elementwise(&x, &y, |p, q| Expr::pow(p.clone(), q.clone()))?,
        BinOp::MatMul if scalar => elementwise(&x, &y, |p, q| Expr::mul(vec![p.clone(), q.clone()]))?,
        BinOp::MatMul => matmul(&x, &y)?,
        BinOp::MatDiv if y.data.len() == 1 => elementwise(&x, &y, |p, q| Expr::div(p.clone(), q.clone()))?,
        BinOp::MatDiv => matmul(&x, &sym_inv(&y)?)?,
        BinOp::MatLeftDiv if x.data.len() == 1 => elementwise(&x, &y, |p, q| Expr::div(q.clone(), p.clone()))?,
        BinOp::MatLeftDiv => matmul(&sym_inv(&x)?, &y)?,
        BinOp::MatPow if x.data.len() == 1 && y.data.len() == 1 => elementwise(&x, &y, |p, q| Expr::pow(p.clone(), q.clone()))?,
        BinOp::MatPow => {
            let k = y.data.first().and_then(|e| e.as_num()).and_then(|n| n.as_int()).ok_or_else(|| LangError::new("Matrix power requires an integer exponent.", 0))?;
            let base = if k < 0 { sym_inv(&x)? } else { x.clone() };
            let n = x.dims[0];
            let mut acc = SymArray::from_dims(vec![n, n], (0..n * n).map(|i| if i % (n + 1) == 0 { Expr::one() } else { Expr::zero() }).collect());
            for _ in 0..k.abs() {
                acc = matmul(&acc, &base)?;
            }
            acc
        }
        BinOp::Eq => elementwise(&x, &y, |p, q| Expr::rel(Rel::Eq, p.clone(), q.clone()))?,
        BinOp::Ne => elementwise(&x, &y, |p, q| Expr::rel(Rel::Ne, p.clone(), q.clone()))?,
        BinOp::Lt => elementwise(&x, &y, |p, q| Expr::rel(Rel::Lt, p.clone(), q.clone()))?,
        BinOp::Le => elementwise(&x, &y, |p, q| Expr::rel(Rel::Le, p.clone(), q.clone()))?,
        BinOp::Gt => elementwise(&x, &y, |p, q| Expr::rel(Rel::Gt, p.clone(), q.clone()))?,
        BinOp::Ge => elementwise(&x, &y, |p, q| Expr::rel(Rel::Ge, p.clone(), q.clone()))?,
        _ => return rt(format!("Operator '{}' is not supported for symbolic inputs.", op.symbol())),
    };
    Ok(Value::Sym(out))
}

pub fn unary(op: UnOp, v: &Value) -> R<Value> {
    let Value::Sym(s) = v else { unreachable!() };
    Ok(Value::Sym(match op {
        UnOp::Neg => s.map(|e| Expr::neg(e.clone())),
        UnOp::Plus => s.clone(),
        UnOp::Transpose | UnOp::CTranspose => {
            let (d, data) = nd::transpose(&s.dims, &s.data)?;
            let t = SymArray::from_dims(d, data);
            if op == UnOp::CTranspose { t.map(conj) } else { t }
        }
        UnOp::Not => return rt("Symbolic NOT is not supported."),
    }))
}

// ---- builtins -------------------------------------------------------------------------------

/// Functions that exist only for symbolic inputs.
pub const NAMES: &[&str] = &[
    "syms", "sym", "symvar", "subs", "int", "limit", "taylor", "simplify", "expand", "collect", "numden", "coeffs", "sym2poly", "poly2sym", "solve",
    "vpa", "digits", "latex", "pretty", "matlabFunction", "fplot", "isAlways", "jacobian", "lhs", "rhs", "assume", "assumeAlso", "symfun", "children",
];

/// Elementwise functions that also accept numbers (dispatched here only for sym inputs).
const ELEMENTWISE: &[&str] = &[
    "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "exp", "log", "sqrt", "abs", "sign", "floor",
    "ceil", "round", "fix", "heaviside", "dirac", "erf", "erfc",
];

fn sym_arg(args: &[Value], i: usize) -> R<SymArray> {
    SymArray::from_value(args.get(i).ok_or_else(|| LangError::new("Not enough input arguments.", 0))?)
}

fn one(v: Value) -> R<Option<Vec<Value>>> {
    Ok(Some(vec![v]))
}

fn sym_val(e: Expr) -> Value {
    Value::Sym(SymArray::scalar(e))
}

/// MATLAB's default variable: `x` if present, else the one closest to `x` alphabetically.
pub fn default_var(e: &Expr) -> Option<String> {
    let mut vars = Vec::new();
    e.free_vars(&mut vars);
    if vars.is_empty() {
        return None;
    }
    vars.sort_by_key(|v| {
        let c = v.chars().next().unwrap_or('x') as i32;
        let d = (c - 'x' as i32).abs() * 2 + if c > 'x' as i32 { 1 } else { 0 };
        (d, v.clone())
    });
    vars.into_iter().next()
}

fn var_name(v: &Value) -> R<String> {
    let s = SymArray::from_value(v)?;
    match s.data.first() {
        Some(Expr::Var(n)) if s.data.len() == 1 => Ok(n.clone()),
        _ => rt("The variable must be a symbolic variable."),
    }
}

fn var_list(v: &Value) -> R<Vec<String>> {
    let s = SymArray::from_value(v)?;
    s.data
        .iter()
        .map(|e| match e {
            Expr::Var(n) => Ok(n.clone()),
            _ => rt("Variables must be symbolic variables."),
        })
        .collect()
}

fn expr_of_scalar(v: &Value) -> R<Expr> {
    let s = SymArray::from_value(v)?;
    if s.data.len() != 1 {
        return rt("A scalar symbolic expression is required.");
    }
    Ok(s.data.into_iter().next().unwrap())
}

/// The equation `l == r` as the expression `l - r`.
fn as_zero(e: &Expr) -> Expr {
    match e {
        Expr::Rel(Rel::Eq, l, r) => Expr::sub((**l).clone(), (**r).clone()),
        other => other.clone(),
    }
}

fn is_sym_name(s: &str) -> bool {
    s.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

pub fn call(it: &mut Interpreter, name: &str, args: Vec<Value>, nargout: usize) -> R<Option<Vec<Value>>> {
    if extra::NAMES.contains(&name) {
        if let Some(r) = extra::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    let any_sym = args.iter().any(|a| matches!(a, Value::Sym(_)));
    if ELEMENTWISE.contains(&name) {
        if !any_sym {
            return Ok(None);
        }
        let s = sym_arg(&args, 0)?;
        return one(Value::Sym(s.map(|e| Expr::func(name, vec![e.clone()]))));
    }
    match name {
        "log10" | "log2" if any_sym => {
            let base = if name == "log10" { 10 } else { 2 };
            let s = sym_arg(&args, 0)?;
            return one(Value::Sym(s.map(|e| Expr::div(Expr::func("log", vec![e.clone()]), Expr::func("log", vec![Expr::int(base)])))));
        }
        "mod" | "rem" if any_sym => {
            let a = sym_arg(&args, 0)?;
            let m = expr_of_scalar(args.get(1).ok_or_else(|| LangError::new("Not enough input arguments.", 0))?)?;
            return one(Value::Sym(a.map(|e| Expr::func(name, vec![e.clone(), m.clone()]))));
        }
        "real" | "imag" | "conj" if any_sym => {
            let s = sym_arg(&args, 0)?;
            return one(Value::Sym(s.map(|e| match name {
                "conj" => conj(e),
                _ => {
                    let z = e.eval_c(&|_| None);
                    match z {
                        Some(z) => Expr::from_f64(if name == "real" { z.re } else { z.im }),
                        None => Expr::Fn(name.into(), vec![e.clone()]),
                    }
                }
            })));
        }
        "det" | "inv" | "transpose" | "trace" | "sum" | "prod" | "diff" | "factor" | "double" | "char" | "string" | "isequal" if !any_sym => {
            return Ok(None);
        }
        _ => {}
    }
    match name {
        "syms" => {
            let mut names = Vec::new();
            let mut real = false;
            for a in &args {
                let t = a.text()?;
                match t.as_str() {
                    "real" | "positive" | "integer" | "rational" => real = true,
                    "clear" | "complex" => {}
                    _ => {
                        if t.contains('(') {
                            return rt("Symbolic functions such as 'syms f(x)' are not supported in this version.");
                        }
                        if !is_sym_name(&t) {
                            return rt(format!("Invalid variable name '{t}'."));
                        }
                        names.push(t);
                    }
                }
            }
            for n in &names {
                REAL_VARS.with(|r| {
                    if real {
                        r.borrow_mut().insert(n.clone());
                    } else {
                        r.borrow_mut().remove(n);
                    }
                });
                it.set_var(n, sym_val(Expr::var(n)));
            }
            Ok(Some(Vec::new()))
        }
        "assume" | "assumeAlso" => {
            let vars = var_list(args.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?).unwrap_or_default();
            let real = args.get(1).and_then(|v| v.text().ok()).map(|t| matches!(t.as_str(), "real" | "positive" | "integer" | "rational")).unwrap_or(false);
            for v in vars {
                REAL_VARS.with(|r| {
                    if real {
                        r.borrow_mut().insert(v);
                    }
                });
            }
            Ok(Some(Vec::new()))
        }
        "sym" => {
            let a = args.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?;
            if let (true, Some(size)) = (a.is_text(), args.get(1)) {
                // sym('a', [1 3]) → [a1, a2, a3]
                if let Ok(m) = size.to_matrix() {
                    if m.kind != Kind::Char {
                        let base = a.text()?;
                        let dims: Dims = if m.numel() == 1 { vec![m.data[0] as usize, m.data[0] as usize] } else { m.data.iter().map(|&x| x as usize).collect() };
                        let n = nd::numel(&dims);
                        let vector = dims.len() == 2 && (dims[0] == 1 || dims[1] == 1);
                        let data = (0..n)
                            .map(|k| {
                                if vector {
                                    Expr::var(&format!("{base}{}", k + 1))
                                } else {
                                    let (r, c) = (k % dims[0], k / dims[0]);
                                    Expr::var(&format!("{base}{}_{}", r + 1, c + 1))
                                }
                            })
                            .collect();
                        return one(Value::Sym(SymArray::from_dims(dims, data)));
                    }
                }
            }
            one(Value::Sym(SymArray::from_value(a)?))
        }
        "symvar" => {
            let s = sym_arg(&args, 0)?;
            let mut vars = Vec::new();
            for e in &s.data {
                e.free_vars(&mut vars);
            }
            vars.sort();
            if let Some(n) = args.get(1) {
                let k = n.as_usize()?;
                if let Some(v) = s.data.first().and_then(default_var) {
                    vars.retain(|x| *x != v);
                    vars.insert(0, v);
                }
                vars.truncate(k);
            }
            let n = vars.len();
            one(Value::Sym(SymArray::from_dims(vec![if n == 0 { 0 } else { 1 }, n], vars.iter().map(|v| Expr::var(v)).collect())))
        }
        "subs" => {
            let f = sym_arg(&args, 0)?;
            let (olds, news): (Vec<Value>, Vec<Value>) = match args.len() {
                1 => return one(Value::Sym(f)),
                2 => {
                    let v = f.data.iter().find_map(default_var).ok_or_else(|| LangError::new("The expression has no symbolic variables to substitute.", 0))?;
                    (vec![sym_val(Expr::var(&v))], vec![args[1].clone()])
                }
                _ => (cell_items(&args[1]), cell_items(&args[2])),
            };
            let mut pairs: Vec<(String, Value)> = Vec::new();
            for (o, n) in olds.iter().zip(&news) {
                let vars = var_list(o)?;
                if vars.len() == 1 {
                    pairs.push((vars[0].clone(), n.clone()));
                } else {
                    let ns = SymArray::from_value(n)?;
                    if ns.data.len() != vars.len() {
                        return rt("Inconsistency between sizes of second and third arguments.");
                    }
                    for (v, e) in vars.iter().zip(ns.data) {
                        pairs.push((v.clone(), sym_val(e)));
                    }
                }
            }
            // Substituting an array for a scalar expression gives an array (elementwise).
            if f.data.len() == 1 && pairs.len() == 1 {
                let vals = SymArray::from_value(&pairs[0].1)?;
                if vals.data.len() > 1 {
                    let v = &pairs[0].0;
                    return one(Value::Sym(vals.map(|x| f.data[0].subs(v, x))));
                }
            }
            let mut out = f;
            for (v, val) in pairs {
                let e = expr_of_scalar(&val)?;
                out = out.map(|x| x.subs(&v, &e));
            }
            one(Value::Sym(out))
        }
        "diff" => {
            let f = sym_arg(&args, 0)?;
            let mut var = None;
            let mut n = 1usize;
            for a in &args[1..] {
                match a {
                    Value::Num(m) if m.kind != Kind::Char => n = a.as_usize()?,
                    other => var = Some(var_name(other)?),
                }
            }
            let out = f.map(|e| {
                let v = var.clone().or_else(|| default_var(e));
                match v {
                    Some(v) => {
                        let mut d = e.clone();
                        for _ in 0..n {
                            d = calculus::diff(&d, &v);
                        }
                        calculus::tidy(&d)
                    }
                    None => Expr::zero(),
                }
            });
            one(Value::Sym(out))
        }
        "int" => {
            let f = sym_arg(&args, 0)?;
            let rest = &args[1..];
            let (var, bounds) = match rest.len() {
                0 => (None, None),
                1 => (Some(var_name(&rest[0])?), None),
                2 => (None, Some((expr_of_scalar(&rest[0])?, expr_of_scalar(&rest[1])?))),
                _ => (Some(var_name(&rest[0])?), Some((expr_of_scalar(&rest[1])?, expr_of_scalar(&rest[2])?))),
            };
            let out = f.map(|e| {
                let Some(v) = var.clone().or_else(|| default_var(e)).or_else(|| Some("x".into())) else { return e.clone() };
                match &bounds {
                    Some((a, b)) => calculus::int_def(e, &v, a, b),
                    None => calculus::int_indef(e, &v),
                }
            });
            one(Value::Sym(out))
        }
        "limit" => {
            let f = expr_of_scalar(&args[0])?;
            let mut dir = 0;
            let mut rest: Vec<&Value> = Vec::new();
            for a in &args[1..] {
                match a.text().ok().as_deref() {
                    Some("left") => dir = -1,
                    Some("right") => dir = 1,
                    _ => rest.push(a),
                }
            }
            let (v, point) = match rest.len() {
                0 => (default_var(&f).unwrap_or_else(|| "x".into()), Expr::zero()),
                1 => (default_var(&f).unwrap_or_else(|| "x".into()), expr_of_scalar(rest[0])?),
                _ => (var_name(rest[0])?, expr_of_scalar(rest[1])?),
            };
            one(sym_val(calculus::limit(&f, &v, &point, dir)))
        }
        "taylor" => {
            let f = expr_of_scalar(&args[0])?;
            let mut v = default_var(&f).unwrap_or_else(|| "x".into());
            let mut a = Expr::zero();
            let mut order = 6usize;
            let mut i = 1;
            let mut positional = 0;
            while i < args.len() {
                match args[i].text().ok().as_deref() {
                    Some("Order") => {
                        order = args.get(i + 1).ok_or_else(|| LangError::new("Missing value for 'Order'.", 0))?.as_usize()?;
                        i += 2;
                        continue;
                    }
                    Some("ExpansionPoint") => {
                        a = expr_of_scalar(args.get(i + 1).ok_or_else(|| LangError::new("Missing value for 'ExpansionPoint'.", 0))?)?;
                        i += 2;
                        continue;
                    }
                    _ => {}
                }
                if positional == 0 {
                    v = var_name(&args[i])?;
                } else {
                    a = expr_of_scalar(&args[i])?;
                }
                positional += 1;
                i += 1;
            }
            one(sym_val(calculus::taylor(&f, &v, &a, order)))
        }
        "simplify" => one(Value::Sym(sym_arg(&args, 0)?.map(algebra::simplify))),
        "expand" => one(Value::Sym(sym_arg(&args, 0)?.map(algebra::expand))),
        "factor" => {
            let f = expr_of_scalar(&args[0])?;
            let fs = algebra::factor_list(&f);
            let n = fs.len();
            one(Value::Sym(SymArray::from_dims(vec![1, n], fs)))
        }
        "collect" => {
            let f = sym_arg(&args, 0)?;
            let v = match args.get(1) {
                Some(a) => Some(var_name(a)?),
                None => None,
            };
            one(Value::Sym(f.map(|e| match v.clone().or_else(|| default_var(e)) {
                Some(v) => algebra::collect(e, &v),
                None => e.clone(),
            })))
        }
        "numden" => {
            let f = expr_of_scalar(&args[0])?;
            let t = algebra::together(&f);
            let (n, d) = algebra::numden(&t);
            Ok(Some(vec![sym_val(n), sym_val(d)]))
        }
        "coeffs" => {
            let f = expr_of_scalar(&args[0])?;
            let all = args.iter().any(|a| a.text().map(|t| t == "All").unwrap_or(false));
            let v = match args.get(1).filter(|a| !a.is_text() || a.text().map(|t| t != "All").unwrap_or(true)) {
                Some(a) => var_name(a)?,
                None => default_var(&f).unwrap_or_else(|| "x".into()),
            };
            let c = algebra::poly_coeffs(&f, &v).ok_or_else(|| LangError::new("The input must be a polynomial in the given variable.", 0))?;
            let (coeffs, terms): (Vec<Expr>, Vec<Expr>) = if all {
                // 'All': every coefficient, highest degree first.
                c.iter().enumerate().rev().map(|(k, a)| (a.clone(), Expr::pow(Expr::var(&v), Expr::int(k as i128)))).unzip()
            } else {
                // Nonzero coefficients in ascending degree.
                c.iter().enumerate().filter(|(_, a)| !a.is_zero()).map(|(k, a)| (a.clone(), Expr::pow(Expr::var(&v), Expr::int(k as i128)))).unzip()
            };
            let n = coeffs.len();
            Ok(Some(vec![Value::Sym(SymArray::from_dims(vec![1, n], coeffs)), Value::Sym(SymArray::from_dims(vec![1, n], terms))]))
        }
        "sym2poly" => {
            let f = expr_of_scalar(&args[0])?;
            let v = default_var(&f).unwrap_or_else(|| "x".into());
            let c = algebra::poly_coeffs(&f, &v).ok_or_else(|| LangError::new("Not a polynomial.", 0))?;
            let vals: Option<Vec<f64>> = c.iter().rev().map(|e| e.to_f64()).collect();
            let vals = vals.ok_or_else(|| LangError::new("Polynomial coefficients must be numeric.", 0))?;
            one(Value::Num(Matrix::row(vals)))
        }
        "poly2sym" => {
            let c = SymArray::from_value(&args[0])?;
            let v = match args.get(1) {
                Some(a) => var_name(a)?,
                None => "x".into(),
            };
            let asc: Vec<Expr> = c.data.iter().rev().cloned().collect();
            one(sym_val(algebra::from_coeffs(&asc, &v)))
        }
        "solve" => {
            let eqs = sym_arg(&args, 0)?;
            let eqs: Vec<Expr> = eqs.data.iter().map(as_zero).collect();
            let vars: Vec<String> = match args.get(1) {
                Some(v) => var_list(v)?,
                None => {
                    let mut vs = Vec::new();
                    for e in &eqs {
                        e.free_vars(&mut vs);
                    }
                    vs.sort();
                    if eqs.len() == 1 {
                        default_var(&eqs[0]).into_iter().collect()
                    } else {
                        vs.truncate(eqs.len());
                        vs
                    }
                }
            };
            if vars.len() == 1 && eqs.len() == 1 {
                let roots = algebra::solve_one(&eqs[0], &vars[0]).ok_or_else(|| LangError::new("Unable to find an explicit solution.", 0))?;
                let n = roots.len();
                return one(Value::Sym(SymArray::from_dims(vec![n, if n == 0 { 0 } else { 1 }], roots)));
            }
            let sol = algebra::solve_linear(&eqs, &vars).ok_or_else(|| LangError::new("Unable to find an explicit solution (only linear systems are supported for several unknowns in this version).", 0))?;
            if nargout >= vars.len() {
                return Ok(Some(sol.into_iter().map(sym_val).collect()));
            }
            let mut s = StructArray::scalar();
            for (v, e) in vars.iter().zip(sol) {
                s.set(v, sym_val(e));
            }
            one(Value::Struct(s))
        }
        "double" => one(Value::Num(sym_arg(&args, 0)?.to_matrix()?)),
        "vpa" => {
            // vpa(x, d) evaluates with d significant digits; plain vpa(x) uses digits().
            let saved = expr::digits();
            if let Some(d) = args.get(1) {
                let d = d.scalar()?;
                if !(1.0..=536870912.0).contains(&d) || d.fract() != 0.0 {
                    return rt("Number of digits must be a positive integer.");
                }
                expr::set_digits(d as u32);
            }
            let out = sym_arg(&args, 0).map(|s| s.map(mp::vpa));
            expr::set_digits(saved);
            one(Value::Sym(out?))
        }
        "digits" => {
            let old = expr::digits();
            if let Some(d) = args.first() {
                let d = d.scalar()?;
                if !(1.0..=536870912.0).contains(&d) || d.fract() != 0.0 {
                    return rt("Number of digits must be a positive integer.");
                }
                expr::set_digits(d as u32);
                return if nargout > 0 { one(Value::num(old as f64)) } else { Ok(Some(Vec::new())) };
            }
            if nargout > 0 {
                return one(Value::num(old as f64));
            }
            it.emit(crate::interp::OutputKind::Text(format!("\nDigits = {old}\n\n")));
            Ok(Some(Vec::new()))
        }
        "char" => one(Value::chars(&sym_arg(&args, 0)?.disp())),
        "string" => {
            let s = sym_arg(&args, 0)?;
            one(Value::Str(StrArray::from_dims(s.dims.clone(), s.data.iter().map(|e| Some(e.to_string())).collect())))
        }
        "latex" => one(Value::chars(&print::latex(&expr_of_scalar(&args[0])?))),
        "pretty" => {
            let s = sym_arg(&args, 0)?;
            it.emit(crate::interp::OutputKind::Text(format!("{}\n", s.disp())));
            Ok(Some(Vec::new()))
        }
        "isAlways" => one(Value::bool(sym_arg(&args, 0)?.truth().unwrap_or(false))),
        "lhs" | "rhs" => one(Value::Sym(sym_arg(&args, 0)?.map(|e| match e {
            Expr::Rel(_, l, r) => if name == "lhs" { (**l).clone() } else { (**r).clone() },
            other => if name == "lhs" { other.clone() } else { Expr::zero() },
        }))),
        "children" => {
            let e = expr_of_scalar(&args[0])?;
            let kids: Vec<Expr> = match e {
                Expr::Add(x) | Expr::Mul(x) | Expr::Fn(_, x) => x,
                Expr::Pow(a, b) | Expr::Rel(_, a, b) => vec![*a, *b],
                other => vec![other],
            };
            let n = kids.len();
            one(Value::Sym(SymArray::from_dims(vec![1, n], kids)))
        }
        "det" => one(sym_val(algebra::simplify(&sym_det(&sym_arg(&args, 0)?)?))),
        "inv" => one(Value::Sym(sym_inv(&sym_arg(&args, 0)?)?)),
        "transpose" => {
            let s = sym_arg(&args, 0)?;
            let (d, data) = nd::transpose(&s.dims, &s.data)?;
            one(Value::Sym(SymArray::from_dims(d, data)))
        }
        "trace" => {
            let s = sym_arg(&args, 0)?;
            let n = s.dims[0];
            one(sym_val(Expr::add((0..n.min(s.dims[1])).map(|i| s.data[i * n + i].clone()).collect())))
        }
        "sum" | "prod" => {
            let s = sym_arg(&args, 0)?;
            let combine = |v: Vec<Expr>| if name == "sum" { Expr::add(v) } else { Expr::mul(v) };
            if s.dims[0] == 1 || s.dims[1] == 1 {
                return one(sym_val(combine(s.data.clone())));
            }
            let (r, c) = (s.dims[0], s.dims[1]);
            let data = (0..c).map(|j| combine(s.data[j * r..(j + 1) * r].to_vec())).collect();
            one(Value::Sym(SymArray::from_dims(vec![1, c], data)))
        }
        "jacobian" => {
            let f = sym_arg(&args, 0)?;
            let vars = var_list(&args[1])?;
            let (m, n) = (f.data.len(), vars.len());
            let mut data = vec![Expr::zero(); m * n];
            for (i, e) in f.data.iter().enumerate() {
                for (j, v) in vars.iter().enumerate() {
                    data[j * m + i] = algebra::simplify(&calculus::diff(e, v));
                }
            }
            one(Value::Sym(SymArray::from_dims(vec![m, n], data)))
        }
        "isequal" => {
            let a = sym_arg(&args, 0)?;
            let ok = args[1..].iter().all(|b| match SymArray::from_value(b) {
                Ok(b) => a.dims == b.dims && a.data.iter().zip(&b.data).all(|(x, y)| algebra::simplify(&Expr::sub(x.clone(), y.clone())).is_zero()),
                Err(_) => false,
            });
            one(Value::bool(ok))
        }
        "matlabFunction" => {
            let f = sym_arg(&args, 0)?;
            let mut vars: Vec<String> = Vec::new();
            let mut i = 1;
            while i < args.len() {
                if args[i].text().map(|t| t == "Vars").unwrap_or(false) {
                    if let Some(v) = args.get(i + 1) {
                        vars = cell_items(v).iter().map(|x| var_list(x)).collect::<R<Vec<_>>>()?.concat();
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if vars.is_empty() {
                for e in &f.data {
                    e.free_vars(&mut vars);
                }
                vars.sort();
            }
            let body = if f.data.len() == 1 {
                print::print(&f.data[0], print::Style::Code)
            } else {
                let rows: Vec<String> = (0..f.dims[0])
                    .map(|r| (0..f.dims[1]).map(|c| print::print(&f.data[c * f.dims[0] + r], print::Style::Code)).collect::<Vec<_>>().join(","))
                    .collect();
                format!("[{}]", rows.join(";"))
            };
            let src = format!("@({}){}", vars.join(","), body);
            let ast = crate::parser::parse_expr(&src)?;
            one(it.eval(&ast)?)
        }
        "fplot" => {
            let f = args.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?.clone();
            let (a, b) = match args.get(1) {
                Some(v) if !v.is_text() => {
                    let m = v.to_matrix()?;
                    (m.data.first().copied().unwrap_or(-5.0), m.data.get(1).copied().unwrap_or(5.0))
                }
                _ => (-5.0, 5.0),
            };
            let opts: Vec<Value> = args.iter().skip(1).skip_while(|v| !v.is_text()).cloned().collect();
            let n = 400;
            let xs: Vec<f64> = (0..n).map(|k| a + (b - a) * k as f64 / (n - 1) as f64).collect();
            let ys: Vec<f64> = match &f {
                Value::Sym(s) => {
                    let e = s.data.first().cloned().unwrap_or_else(Expr::zero);
                    let v = default_var(&e);
                    xs.iter()
                        .map(|&x| {
                            e.eval_c(&|name| if Some(name) == v.as_deref() { Some(crate::cplx::C::new(x, 0.0)) } else { None })
                                .filter(|z| z.im.abs() < 1e-12)
                                .map(|z| z.re)
                                .unwrap_or(f64::NAN)
                        })
                        .collect()
                }
                Value::Func(fv) => {
                    let mut ys = Vec::with_capacity(n);
                    for &x in &xs {
                        let y = it.call_value(fv, vec![Value::num(x)], 1)?.into_iter().next().and_then(|v| v.scalar().ok()).unwrap_or(f64::NAN);
                        ys.push(y);
                    }
                    ys
                }
                _ => return rt("fplot requires a function handle or a symbolic expression."),
            };
            let mut pargs = vec![Value::Num(Matrix::row(xs)), Value::Num(Matrix::row(ys))];
            pargs.extend(opts);
            it.call_function("plot", pargs, 0)?;
            Ok(Some(Vec::new()))
        }
        "symfun" => rt("Symbolic functions are not supported in this version."),
        _ => Ok(None),
    }
}

fn cell_items(v: &Value) -> Vec<Value> {
    match v {
        Value::Cell(c) => c.data.clone(),
        other => vec![other.clone()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text() {
        assert_eq!(parse_sym_text("x^2 + 2*x + 1").unwrap().to_string(), "x^2 + 2*x + 1");
        assert_eq!(parse_sym_text("1/3").unwrap(), Expr::rat(1, 3));
    }

    #[test]
    fn rationals_from_doubles() {
        assert_eq!(expr::Num::from_f64(0.1), expr::Num::rat(1, 10));
    }
}
