//! MATLAB's `ops` folder (`toolbox/matlab/ops`): bit-wise functions (`bitand` … `bitshift`),
//! set operations (`union`, `intersect`, `setdiff`, `setxor`), page-wise matrix algebra
//! (`pagemtimes`, `pagetranspose`, `pagectranspose`, `pagemldivide`, `pagemrdivide`),
//! `tensorprod`, and the callable forms of the indexing hooks (`subsref`, `subsasgn`,
//! `substruct`, `subsindex`, `numArgumentsFromSubscript`) with `metaclass`.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{Accessor, Expr, LValue, Stmt, StmtKind};
use crate::builtins::{one, Ret};
use crate::class::{ClassDef, PropDef};
use crate::cplx::C;
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::nd::{self, Dims};
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "bitand", "bitor", "bitxor", "bitcmp", "bitset", "bitget", "bitshift", "union", "intersect", "setdiff", "setxor", "pagemtimes",
    "pagetranspose", "pagectranspose", "pagemldivide", "pagemrdivide", "tensorprod", "subsref", "subsasgn", "substruct", "subsindex",
    "numArgumentsFromSubscript", "metaclass",
];

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    match name {
        "bitand" | "bitor" | "bitxor" => bit_binary(name, a),
        "bitcmp" => bitcmp(a),
        "bitget" | "bitset" | "bitshift" => bit_position(name, a),
        "union" | "intersect" | "setdiff" | "setxor" => set_op(it, name, a),
        "pagemtimes" => pagemtimes(a),
        "pagetranspose" | "pagectranspose" => {
            nargs(a, 1, 1)?;
            one(page_transpose(&a[0], name == "pagectranspose")?)
        }
        "pagemldivide" | "pagemrdivide" => page_divide(name, a),
        "tensorprod" => tensorprod(a),
        "subsref" => subsref(it, a, nargout),
        "subsasgn" => subsasgn(it, a, nargout),
        "substruct" => substruct(a),
        "subsindex" => {
            nargs(a, 1, 1)?;
            match &a[0] {
                Value::Cell(_) => rt("Unable to use a value of type cell as an index."),
                v => rt(format!("Function 'subsindex' is not defined for values of class '{}'.", v.class_name())),
            }
        }
        "numArgumentsFromSubscript" => {
            nargs(a, 3, 3)?;
            // Only an IndexingContext enumeration member is accepted, and those cannot be
            // created here.
            rt("The third argument must be a scalar of class matlab.indexing.IndexingContext.")
        }
        "metaclass" => {
            nargs(a, 1, 1)?;
            one(metaclass(it, &a[0]))
        }
        _ => unreachable!(),
    }
}

fn nargs(a: &[Value], min: usize, max: usize) -> R<()> {
    if a.len() < min {
        return rt("Not enough input arguments.");
    }
    if a.len() > max {
        return rt("Too many input arguments.");
    }
    Ok(())
}

// ---- bit-wise functions ------------------------------------------------------------------------

fn signed(c: IntClass) -> bool {
    matches!(c, IntClass::I8 | IntClass::I16 | IntClass::I32 | IntClass::I64)
}

fn width(c: IntClass) -> u32 {
    c.bytes() as u32 * 8
}

fn mask(c: IntClass) -> u64 {
    if width(c) == 64 { u64::MAX } else { (1u64 << width(c)) - 1 }
}

/// Two's complement bit pattern of an in-range value of class `c`.
fn to_bits(v: f64, c: IntClass) -> u64 {
    if signed(c) { (v as i64 as u64) & mask(c) } else { v as u64 }
}

fn from_bits(b: u64, c: IntClass) -> f64 {
    let b = b & mask(c);
    if signed(c) {
        let shift = 64 - width(c);
        (((b << shift) as i64) >> shift) as f64
    } else {
        b as f64
    }
}

/// Whether a double holds an integer value representable in class `c` (the upper bound is
/// exclusive: 2^64 itself is out of range for uint64).
fn in_range(v: f64, c: IntClass) -> bool {
    let (lo, _) = c.range();
    let hi = if signed(c) { 2f64.powi(width(c) as i32 - 1) } else { 2f64.powi(width(c) as i32) };
    v.fract() == 0.0 && v >= lo && v < hi
}

fn assumed_type(v: &Value) -> R<IntClass> {
    if v.is_text() {
        if let Some(c) = IntClass::from_name(&v.text()?) {
            return Ok(c);
        }
    }
    rt("ASSUMEDTYPE must be an integer type name.")
}

fn int_class(m: &Matrix) -> Option<IntClass> {
    match m.kind {
        Kind::Int(c) => Some(c),
        _ => None,
    }
}

/// An explicit ASSUMEDTYPE must match the class of an integer input.
fn resolve_type(given: Option<&Value>, own: Option<IntClass>) -> R<IntClass> {
    match given {
        Some(v) => {
            let t = assumed_type(v)?;
            if own.is_some_and(|c| c != t) {
                return rt("Integer inputs cannot be assumed as any other types than their own types.");
            }
            Ok(t)
        }
        None => Ok(own.unwrap_or(IntClass::U64)),
    }
}

fn check_doubles(m: &Matrix, c: IntClass, mixed: bool) -> R<()> {
    if m.kind.is_int() || m.data.iter().all(|&v| in_range(v, c)) {
        return Ok(());
    }
    rt(if mixed {
        "Double inputs must have integer values in the integer range when mixed with integer inputs."
    } else {
        "Double inputs must have integer values in the range of ASSUMEDTYPE."
    })
}

fn bit_binary(name: &str, a: &[Value]) -> Ret {
    nargs(a, 2, 3)?;
    let operand = |v: &Value| -> R<Matrix> {
        match v {
            Value::Num(m) if m.kind != Kind::Char => Ok(m.clone()),
            _ => rt("Operands to BIT Ops must be numeric."),
        }
    };
    let (x, y) = (operand(&a[0])?, operand(&a[1])?);
    if x.is_complex() || y.is_complex() {
        return rt("Inputs must be real.");
    }
    if x.kind == Kind::Single || y.kind == Kind::Single {
        return rt("Invalid data type. First two arguments must be integer or double.");
    }
    const MIX: &str = "Inputs must be signed or unsigned integers of the same class or scalar doubles.";
    let class = match (int_class(&x), int_class(&y)) {
        (Some(p), Some(q)) if p != q => return rt(MIX),
        (Some(p), Some(_)) => Some(p),
        (Some(p), None) | (None, Some(p)) => {
            let other = if x.kind.is_int() { &y } else { &x };
            if other.kind != Kind::Double || !other.is_scalar() {
                return rt(MIX);
            }
            Some(p)
        }
        (None, None) => None,
    };
    let t = resolve_type(a.get(2), class)?;
    check_doubles(&x, t, class.is_some())?;
    check_doubles(&y, t, class.is_some())?;
    let (dims, pairs) = nd::broadcast_pairs(&x.dims, &y.dims).map_err(|_| LangError::new("Inputs must have the same size.", 0))?;
    let f = |p: u64, q: u64| match name {
        "bitand" => p & q,
        "bitor" => p | q,
        _ => p ^ q,
    };
    let data = pairs.iter().map(|&(i, j)| from_bits(f(to_bits(x.data[i], t), to_bits(y.data[j], t)), t)).collect();
    one(Value::Num(Matrix::from_dims(dims, data).with_kind(class.map_or(Kind::Double, Kind::Int))))
}

fn bitcmp(a: &[Value]) -> Ret {
    nargs(a, 1, 2)?;
    let x = match &a[0] {
        Value::Num(m) if m.kind == Kind::Double || m.kind.is_int() => m.clone(),
        _ => return rt("Invalid data type. First argument must be integer or double."),
    };
    if x.is_complex() {
        return rt("Inputs must be real.");
    }
    let t = resolve_type(a.get(1), int_class(&x))?;
    check_doubles(&x, t, false)?;
    let data = x.data.iter().map(|&v| from_bits(!to_bits(v, t), t)).collect();
    one(Value::Num(Matrix::from_dims(x.dims.clone(), data).with_kind(x.kind)))
}

/// `bitget(A, bit)`, `bitset(A, bit, V)` and `bitshift(A, k)`: elementwise over arrays of the
/// same size, where scalars expand (no implicit expansion otherwise).
fn bit_position(name: &str, a: &[Value]) -> Ret {
    let max = if name == "bitset" { 4 } else { 3 };
    nargs(a, 2, max)?;
    let numeric = |v: &Value| -> R<Matrix> {
        match v {
            Value::Num(m) if m.kind != Kind::Char && m.kind != Kind::Logical => Ok(m.clone()),
            _ => rt("Inputs must be numeric."),
        }
    };
    let x = numeric(&a[0])?;
    let k = numeric(&a[1])?;
    let mut operands = vec![x.clone(), k.clone()];
    // bitset(A, bit, [V], [assumedtype]).
    let type_arg = match (name, a.get(2)) {
        ("bitset", Some(v)) if v.is_text() => {
            if a.len() > 3 {
                return rt("Too many input arguments.");
            }
            Some(v)
        }
        ("bitset", Some(v)) => {
            // V may be logical.
            operands.push(match v {
                Value::Num(m) if m.kind != Kind::Char => m.clone(),
                _ => return rt("Inputs must be numeric."),
            });
            a.get(3)
        }
        (_, t) => t,
    };
    if operands.iter().any(|m| m.is_complex()) {
        return rt("Inputs must be real.");
    }
    if x.kind == Kind::Single || k.kind == Kind::Single {
        return rt("Invalid data type. First and second arguments must be integer or double.");
    }
    let t = resolve_type(type_arg, int_class(&x))?;
    check_doubles(&x, t, false)?;
    let dims = match operands.iter().find(|m| !m.is_scalar()) {
        Some(m) => m.dims.clone(),
        None => vec![1, 1],
    };
    let n = nd::numel(&dims);
    if operands.iter().any(|m| !m.is_scalar() && m.dims != dims) {
        return rt(if name == "bitshift" { "Inputs must have same size." } else { "Inputs must have the same size." });
    }
    let at = |m: &Matrix, i: usize| if m.is_scalar() { m.data[0] } else { m.data[i] };
    let w = width(t) as f64;
    let mut data = Vec::with_capacity(n);
    for i in 0..n {
        let bits = to_bits(at(&x, i), t);
        let kv = at(&k, i);
        let out = if name == "bitshift" {
            if kv.fract() != 0.0 || !kv.is_finite() {
                return rt("Inputs must be integers.");
            }
            if kv >= 0.0 {
                if kv >= 64.0 { 0 } else { bits << kv as u32 }
            } else if signed(t) {
                let v = (bits << (64 - width(t))) as i64 >> (64 - width(t));
                (v >> (-kv).min(63.0) as u32) as u64
            } else if -kv >= 64.0 {
                0
            } else {
                bits >> (-kv) as u32
            }
        } else {
            if kv.fract() != 0.0 || kv < 1.0 || kv > w {
                return rt("The BIT input is out of range for the datatype specified.");
            }
            let b = 1u64 << (kv as u32 - 1);
            match name {
                "bitget" => u64::from(bits & b != 0),
                _ => {
                    let on = operands.get(2).is_none_or(|v| at(v, i) != 0.0);
                    if on { bits | b } else { bits & !b }
                }
            }
        };
        data.push(from_bits(out, t));
    }
    one(Value::Num(Matrix::from_dims(dims, data).with_kind(x.kind)))
}

// ---- set operations ------------------------------------------------------------------------

/// One set element: a number (real, imaginary), a text (None for <missing>), or a row of
/// numbers for the 'rows' option.
#[derive(Clone, Debug)]
enum Item {
    Num(f64, f64),
    Text(Option<String>),
    Row(Vec<(f64, f64)>),
}

fn is_nan(p: (f64, f64)) -> bool {
    p.0.is_nan() || p.1.is_nan()
}

/// MATLAB's sort order: NaN last; complex values by magnitude, then phase angle.
fn num_cmp(p: (f64, f64), q: (f64, f64), complex: bool) -> Ordering {
    match (is_nan(p), is_nan(q)) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    if complex {
        let (a, b) = (C::new(p.0, p.1), C::new(q.0, q.1));
        a.abs().partial_cmp(&b.abs()).unwrap_or(Ordering::Equal).then(a.arg().partial_cmp(&b.arg()).unwrap_or(Ordering::Equal))
    } else {
        p.0.partial_cmp(&q.0).unwrap_or(Ordering::Equal)
    }
}

fn item_cmp(x: &Item, y: &Item, complex: bool) -> Ordering {
    match (x, y) {
        (Item::Num(a, b), Item::Num(c, d)) => num_cmp((*a, *b), (*c, *d), complex),
        (Item::Text(a), Item::Text(b)) => match (a, b) {
            (Some(a), Some(b)) => a.encode_utf16().cmp(b.encode_utf16()),
            (None, None) => Ordering::Equal,
            (None, _) => Ordering::Greater,
            (_, None) => Ordering::Less,
        },
        (Item::Row(a), Item::Row(b)) => a.iter().zip(b).map(|(&p, &q)| num_cmp(p, q, complex)).find(|o| o.is_ne()).unwrap_or(Ordering::Equal),
        _ => Ordering::Equal,
    }
}

/// Set membership equality: NaN and <missing> never equal anything.
fn item_eq(x: &Item, y: &Item) -> bool {
    match (x, y) {
        (Item::Num(a, b), Item::Num(c, d)) => !is_nan((*a, *b)) && a == c && b == d,
        (Item::Text(Some(a)), Item::Text(Some(b))) => a == b,
        (Item::Row(a), Item::Row(b)) => a.iter().zip(b).all(|(p, q)| !is_nan(*p) && p == q),
        _ => false,
    }
}

/// What the result is made of.
enum SetOut {
    Num { kind: Kind, cols: Option<usize> },
    Cell,
    Str,
}

struct SetData {
    a: Vec<Item>,
    b: Vec<Item>,
    /// Orientation of each input for the vector result: row vectors (1×n).
    row_a: bool,
    row_b: bool,
    complex: bool,
    out: SetOut,
}

fn is_row(d: &[usize]) -> bool {
    d.len() == 2 && d[0] == 1
}

fn char_rows(m: &Matrix) -> Vec<String> {
    (0..m.rows()).map(|r| (0..m.cols()).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or('?')).collect()).collect()
}

fn class_mix(ka: Kind, kb: Kind) -> R<Kind> {
    if ka == kb {
        return Ok(ka);
    }
    match (ka, kb) {
        (Kind::Double, k) | (k, Kind::Double) => Ok(if k == Kind::Logical { Kind::Double } else { k }),
        _ => rt(format!("Input A of class {} and input B of class {} must be the same class, unless one is double.", ka.name(), kb.name())),
    }
}

fn set_inputs(it: &mut Interpreter, a: &Value, b: &Value, rows: bool) -> R<SetData> {
    if matches!(a, Value::Str(_)) || matches!(b, Value::Str(_)) {
        // Strings: char vectors and cellstr elements become string elements.
        let items = |v: &Value, which: &str| -> R<(Vec<Item>, bool)> {
            Ok(match v {
                Value::Str(s) => (s.data.iter().map(|t| Item::Text(t.clone())).collect(), is_row(&s.dims)),
                Value::Num(m) if m.kind == Kind::Char => {
                    if m.rows() <= 1 {
                        (vec![Item::Text(Some(m.to_string_lossy()))], true)
                    } else {
                        (char_rows(m).into_iter().map(|t| Item::Text(Some(t))).collect(), false)
                    }
                }
                Value::Cell(c) => (c.data.iter().map(|e| e.text().map(|t| Item::Text(Some(t)))).collect::<R<_>>()?, is_row(&c.dims)),
                _ => return rt(format!("{which} argument must be text.")),
            })
        };
        let (ia, ra) = items(a, "First")?;
        let (ib, rb) = items(b, "Second")?;
        return Ok(SetData { a: ia, b: ib, row_a: ra, row_b: rb, complex: false, out: SetOut::Str });
    }
    if matches!(a, Value::Cell(_)) || matches!(b, Value::Cell(_)) {
        if rows {
            it.emit(OutputKind::Warning("The 'rows' input is not supported for cell array inputs.".into()));
        }
        let mismatch = || -> R<(Vec<Item>, bool)> {
            rt(format!(
                "Input A of class {} and input B of class {} must be cell arrays of character vectors, unless one is a character vector.",
                a.class_name(),
                b.class_name()
            ))
        };
        let items = |v: &Value| -> R<(Vec<Item>, bool)> {
            match v {
                Value::Cell(c) => {
                    let mut out = Vec::with_capacity(c.data.len());
                    for e in &c.data {
                        match e {
                            Value::Num(m) if m.kind == Kind::Char && m.rows() <= 1 => out.push(Item::Text(Some(m.to_string_lossy()))),
                            _ => return mismatch(),
                        }
                    }
                    Ok((out, is_row(&c.dims)))
                }
                Value::Num(m) if m.kind == Kind::Char => {
                    if m.rows() <= 1 {
                        Ok((vec![Item::Text(Some(m.to_string_lossy()))], true))
                    } else {
                        Ok((char_rows(m).into_iter().map(|t| Item::Text(Some(t))).collect(), false))
                    }
                }
                Value::Num(m) if m.dims == [0, 0] => Ok((Vec::new(), false)),
                _ => mismatch(),
            }
        };
        let (ia, ra) = items(a)?;
        let (ib, rb) = items(b)?;
        return Ok(SetData { a: ia, b: ib, row_a: ra, row_b: rb, complex: false, out: SetOut::Cell });
    }
    let num = |v: &Value| -> R<Matrix> {
        match v {
            Value::Num(m) => Ok(m.clone()),
            other => rt(format!("Undefined function 'union' for input arguments of type '{}'.", other.class_name())),
        }
    };
    let (x, y) = (num(a)?, num(b)?);
    let kind = class_mix(x.kind, y.kind)?;
    let complex = x.is_complex() || y.is_complex();
    let cast = |m: Matrix| if m.kind == kind || kind == Kind::Char { m } else { m.cast_to(kind) };
    let (x, y) = (cast(x), cast(y));
    if rows {
        let empty = |m: &Matrix| m.dims == [0, 0];
        if !x.is_2d() || !y.is_2d() || (x.cols() != y.cols() && !empty(&x) && !empty(&y)) {
            return rt("Inputs A and B must have the same number of columns in the 'rows' case.");
        }
        let cols = if empty(&x) { y.cols() } else { x.cols() };
        let rows_of = |m: &Matrix| -> Vec<Item> { (0..m.rows()).map(|r| Item::Row((0..m.cols()).map(|c| (m.get(r, c), m.im_at(c * m.rows() + r))).collect())).collect() };
        return Ok(SetData { a: rows_of(&x), b: rows_of(&y), row_a: false, row_b: false, complex, out: SetOut::Num { kind, cols: Some(cols) } });
    }
    let items = |m: &Matrix| -> Vec<Item> { (0..m.numel()).map(|i| Item::Num(m.data[i], m.im_at(i))).collect() };
    Ok(SetData { a: items(&x), b: items(&y), row_a: is_row(&x.dims), row_b: is_row(&y.dims), complex, out: SetOut::Num { kind, cols: None } })
}

fn set_op(it: &mut Interpreter, name: &str, a: &[Value]) -> Ret {
    nargs(a, 2, 5)?;
    const VALID: &str = "Valid flags are 'rows', 'stable', 'sorted', 'legacy'.";
    let (mut rows, mut stable, mut sorted) = (false, false, false);
    for f in &a[2..] {
        if !f.is_text() {
            return rt(format!("Invalid input. {VALID}"));
        }
        let t = f.text()?;
        match t.to_ascii_lowercase().as_str() {
            "rows" => rows = true,
            "stable" => stable = true,
            "sorted" => sorted = true,
            "legacy" => return rt("The 'legacy' behavior is not supported."),
            _ => return rt(format!("Unrecognized flag '{t}'. {VALID}")),
        }
    }
    if stable && sorted {
        return rt("You can specify either 'sorted' or 'stable' but not both.");
    }
    let d = set_inputs(it, &a[0], &a[1], rows)?;
    let na = d.a.len();
    let all: Vec<Item> = d.a.iter().chain(&d.b).cloned().collect();
    // Stable sort, then runs of equal items: rep[i] is the first index of i's run.
    let mut order: Vec<usize> = (0..all.len()).collect();
    order.sort_by(|&i, &j| item_cmp(&all[i], &all[j], d.complex));
    let mut rep: Vec<usize> = (0..all.len()).collect();
    for w in 1..order.len() {
        let (p, q) = (order[w - 1], order[w]);
        if item_eq(&all[p], &all[q]) {
            rep[q] = rep[p];
        }
    }
    let mut first_b: HashMap<usize, usize> = HashMap::new();
    for (i, &r) in rep.iter().enumerate().skip(na) {
        first_b.entry(r).or_insert(i);
    }
    let reps: Vec<usize> = if stable { (0..all.len()).filter(|&i| rep[i] == i).collect() } else { order.iter().copied().filter(|&i| rep[i] == i).collect() };
    let keep: Vec<usize> = match name {
        "union" => reps,
        "intersect" => reps.into_iter().filter(|&i| i < na && first_b.contains_key(&i)).collect(),
        "setdiff" => reps.into_iter().filter(|&i| i < na && !first_b.contains_key(&i)).collect(),
        _ => reps.into_iter().filter(|&i| i >= na || !first_b.contains_key(&i)).collect(),
    };
    let row = if name == "setdiff" { d.row_a } else { d.row_a && d.row_b };
    let items: Vec<&Item> = keep.iter().map(|&i| &all[i]).collect();
    let n = items.len();
    let vdims = if row { vec![1, n] } else { vec![n, 1] };
    let c = match d.out {
        SetOut::Num { kind, cols } => {
            let mut m = match cols {
                Some(cols) => {
                    let (mut re, mut im) = (vec![0.0; n * cols], vec![0.0; n * cols]);
                    for (r, it) in items.iter().enumerate() {
                        if let Item::Row(v) = it {
                            for (c, &(x, y)) in v.iter().enumerate() {
                                re[c * n + r] = x;
                                im[c * n + r] = y;
                            }
                        }
                    }
                    Matrix::complex(vec![n, cols], re, im)
                }
                None => {
                    let (re, im) = items.iter().map(|it| if let Item::Num(x, y) = it { (*x, *y) } else { (0.0, 0.0) }).unzip();
                    Matrix::complex(vdims, re, im)
                }
            };
            m.kind = kind;
            Value::Num(m)
        }
        SetOut::Cell => Value::Cell(CellArray::from_dims(vdims, items.iter().map(|it| if let Item::Text(t) = it { Value::chars(t.as_deref().unwrap_or("")) } else { Value::empty() }).collect())),
        SetOut::Str => Value::Str(StrArray::from_dims(vdims, items.iter().map(|it| if let Item::Text(t) = it { t.clone() } else { None }).collect())),
    };
    let index = |v: Vec<f64>| Value::Num(Matrix::from_dims(vec![v.len(), 1], v));
    let ia = index(keep.iter().filter(|&&i| i < na).map(|&i| (i + 1) as f64).collect());
    let ib = match name {
        "intersect" => index(keep.iter().map(|i| (first_b[i] - na + 1) as f64).collect()),
        _ => index(keep.iter().filter(|&&i| i >= na).map(|&i| (i - na + 1) as f64).collect()),
    };
    Ok(Some(if name == "setdiff" { vec![c, ia] } else { vec![c, ia, ib] }))
}

// ---- page-wise functions ------------------------------------------------------------------

/// Arithmetic in double, or in single precision by rounding every operation to f32 (exact
/// for + - * / since a double holds more than twice the bits of a single).
#[derive(Clone, Copy)]
struct Ar {
    single: bool,
}

impl Ar {
    fn r(self, x: f64) -> f64 {
        if self.single { x as f32 as f64 } else { x }
    }
    fn rc(self, c: C) -> C {
        C::new(self.r(c.re), self.r(c.im))
    }
    fn add(self, a: C, b: C) -> C {
        self.rc(a.add(b))
    }
    fn sub(self, a: C, b: C) -> C {
        self.rc(a.sub(b))
    }
    fn mul(self, a: C, b: C) -> C {
        if a.im == 0.0 && b.im == 0.0 {
            return C::new(self.r(a.re * b.re), 0.0);
        }
        let re = self.r(self.r(a.re * b.re) - self.r(a.im * b.im));
        let im = self.r(self.r(a.re * b.im) + self.r(a.im * b.re));
        C::new(re, im)
    }
    fn div(self, a: C, b: C) -> C {
        self.rc(a.div(b))
    }
}

/// A dense matrix of complex scalars (column-major).
#[derive(Clone)]
struct Mat {
    r: usize,
    c: usize,
    d: Vec<C>,
}

impl Mat {
    fn zeros(r: usize, c: usize) -> Mat {
        Mat { r, c, d: vec![C::new(0.0, 0.0); r * c] }
    }
    fn at(&self, i: usize, j: usize) -> C {
        self.d[j * self.r + i]
    }
    fn set(&mut self, i: usize, j: usize, v: C) {
        self.d[j * self.r + i] = v;
    }
    fn transposed(&self, conj: bool) -> Mat {
        let mut t = Mat::zeros(self.c, self.r);
        for i in 0..self.r {
            for j in 0..self.c {
                let v = self.at(i, j);
                t.set(j, i, if conj { v.conj() } else { v });
            }
        }
        t
    }
}

fn float_arg(v: &Value) -> R<Matrix> {
    match v {
        Value::Num(m) if m.kind.is_float() => Ok(m.clone()),
        _ => rt("Invalid data types. Input arrays must be double or single."),
    }
}

/// Page `p` of an N-D array as a matrix.
fn page(m: &Matrix, p: usize) -> Mat {
    let (r, c) = (m.dims[0], m.dims[1]);
    let off = p * r * c;
    Mat { r, c, d: (off..off + r * c).map(|i| C::new(m.data[i], m.im_at(i))).collect() }
}

/// Pairs of page indices for the expanded page dimensions of two arrays.
fn page_pairs(da: &[usize], db: &[usize]) -> R<(Dims, Vec<(usize, usize)>)> {
    let pa: Dims = da[2..].to_vec();
    let pb: Dims = db[2..].to_vec();
    let n = pa.len().max(pb.len());
    let size = |d: &Dims, k: usize| d.get(k).copied().unwrap_or(1);
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let (x, y) = (size(&pa, k), size(&pb, k));
        if x != y && x != 1 && y != 1 {
            return rt("Arrays have incompatible sizes. For every dimension beyond the first two, the dimension sizes for both arrays must either be the same, or one of them must be 1.");
        }
        out.push(if x == 1 { y } else { x });
    }
    let total: usize = out.iter().product();
    let mut pairs = Vec::with_capacity(total);
    for lin in 0..total {
        let (mut rem, mut oa, mut ob, mut sa, mut sb) = (lin, 0, 0, 1, 1);
        for (k, &len) in out.iter().enumerate() {
            let sub = rem % len;
            rem /= len;
            if size(&pa, k) != 1 {
                oa += sub * sa;
            }
            if size(&pb, k) != 1 {
                ob += sub * sb;
            }
            sa *= size(&pa, k);
            sb *= size(&pb, k);
        }
        pairs.push((oa, ob));
    }
    Ok((out, pairs))
}

/// Stacks equally sized pages into an N-D array of shape [r, c, page_dims]. Real operands
/// give a real result (dropping the NaN imaginary parts that 0/0 or 0*Inf leave behind).
fn assemble(pages: Vec<Mat>, r: usize, c: usize, page_dims: &[usize], single: bool, complex: bool) -> Matrix {
    let mut dims = vec![r, c];
    dims.extend_from_slice(page_dims);
    let (re, im): (Vec<f64>, Vec<f64>) = pages.iter().flat_map(|p| p.d.iter().map(|z| (z.re, z.im))).unzip();
    let m = if complex { Matrix::complex(dims, re, im) } else { Matrix::from_dims(dims, re) };
    if single { m.with_kind(Kind::Single) } else { m }
}

/// 'none', 'transpose' or 'ctranspose' as (transpose, conjugate).
fn transp_option(v: &Value) -> R<(bool, bool)> {
    let t = if v.is_text() { v.text()?.to_ascii_lowercase() } else { String::new() };
    match t.as_str() {
        "none" => Ok((false, false)),
        "transpose" => Ok((true, false)),
        "ctranspose" => Ok((true, true)),
        _ => rt("Option must be 'transpose', 'ctranspose' or 'none'."),
    }
}

fn apply_option(m: Mat, (t, conj): (bool, bool)) -> Mat {
    if t { m.transposed(conj) } else { m }
}

fn mat_mul(ar: Ar, x: &Mat, y: &Mat) -> Mat {
    let mut out = Mat::zeros(x.r, y.c);
    for j in 0..y.c {
        for i in 0..x.r {
            let mut s = C::new(0.0, 0.0);
            for k in 0..x.c {
                s = ar.add(s, ar.mul(x.at(i, k), y.at(k, j)));
            }
            out.set(i, j, s);
        }
    }
    out
}

fn scale(ar: Ar, s: C, m: &Mat) -> Mat {
    Mat { r: m.r, c: m.c, d: m.d.iter().map(|&v| ar.mul(s, v)).collect() }
}

fn pagemtimes(a: &[Value]) -> Ret {
    if a.len() != 2 && a.len() != 4 {
        return rt("Incorrect number of input arguments. Specify either two or four input arguments.");
    }
    let (xv, yv) = if a.len() == 2 { (&a[0], &a[1]) } else { (&a[0], &a[2]) };
    let (x, y) = (float_arg(xv)?, float_arg(yv)?);
    let (ox, oy) = if a.len() == 4 { (transp_option(&a[1])?, transp_option(&a[3])?) } else { ((false, false), (false, false)) };
    let ar = Ar { single: x.kind == Kind::Single || y.kind == Kind::Single };
    let (page_dims, pairs) = page_pairs(&x.dims, &y.dims)?;
    let shape = |m: &Matrix, (t, _): (bool, bool)| if t { (m.dims[1], m.dims[0]) } else { (m.dims[0], m.dims[1]) };
    let ((xr, xc), (yr, yc)) = (shape(&x, ox), shape(&y, oy));
    let scalar_x = xr == 1 && xc == 1;
    let scalar_y = yr == 1 && yc == 1;
    if !scalar_x && !scalar_y && xc != yr {
        return rt("Incorrect dimensions for matrix multiplication. Check that the number of columns in the first array matches the number of rows in the second array.");
    }
    let (r, c) = if scalar_x { (yr, yc) } else if scalar_y { (xr, xc) } else { (xr, yc) };
    let pages = pairs
        .iter()
        .map(|&(p, q)| {
            let (px, py) = (apply_option(page(&x, p), ox), apply_option(page(&y, q), oy));
            if scalar_x {
                scale(ar, px.d[0], &py)
            } else if scalar_y {
                scale(ar, py.d[0], &px)
            } else {
                mat_mul(ar, &px, &py)
            }
        })
        .collect();
    one(Value::Num(assemble(pages, r, c, &page_dims, ar.single, x.is_complex() || y.is_complex())))
}

fn page_transpose(v: &Value, conj: bool) -> R<Value> {
    let d = v.dims();
    let mut order: Vec<usize> = (0..d.len()).collect();
    order.swap(0, 1);
    Ok(match v {
        Value::Num(m) => {
            let (dims, re) = nd::permute(&m.dims, &m.data, &order)?;
            let mut out = Matrix::from_dims(dims, re).with_kind(m.kind);
            if let Some(im) = &m.im {
                let im = nd::permute(&m.dims, im, &order)?.1;
                out.im = Some(if conj { im.into_iter().map(|x| -x).collect() } else { im });
            }
            Value::Num(out)
        }
        Value::Cell(c) => {
            let (dims, data) = nd::permute(&c.dims, &c.data, &order)?;
            Value::Cell(CellArray::from_dims(dims, data))
        }
        Value::Str(s) => {
            let (dims, data) = nd::permute(&s.dims, &s.data, &order)?;
            Value::Str(StrArray::from_dims(dims, data))
        }
        Value::Struct(s) => {
            let (dims, elems) = nd::permute(&s.dims, &s.elems, &order)?;
            Value::Struct(StructArray { dims: nd::normalize(dims), fields: s.fields.clone(), elems })
        }
        Value::Func(_) | Value::Object(_) => v.clone(),
        other => return rt(format!("Undefined function 'permute' for input arguments of type '{}'.", other.class_name())),
    })
}

fn is_upper(m: &Mat) -> bool {
    (0..m.c).all(|j| (j + 1..m.r).all(|i| m.at(i, j) == C::new(0.0, 0.0)))
}

fn is_lower(m: &Mat) -> bool {
    (0..m.c).all(|j| (0..j.min(m.r)).all(|i| m.at(i, j) == C::new(0.0, 0.0)))
}

/// Solves a triangular system in place by column sweeps (as BLAS trsv does).
fn tri_solve(ar: Ar, t: &Mat, b: &mut Mat, upper: bool, unit: bool) {
    let n = t.r;
    for col in 0..b.c {
        let idx: Vec<usize> = if upper { (0..n).rev().collect() } else { (0..n).collect() };
        for &j in &idx {
            let mut xj = b.at(j, col);
            if !unit {
                xj = ar.div(xj, t.at(j, j));
            }
            b.set(j, col, xj);
            let rest: Vec<usize> = if upper { (0..j).collect() } else { (j + 1..n).collect() };
            for i in rest {
                let v = ar.sub(b.at(i, col), ar.mul(xj, t.at(i, j)));
                b.set(i, col, v);
            }
        }
    }
}

fn mag1(z: C) -> f64 {
    z.re.abs() + z.im.abs()
}

/// LU with partial pivoting (pivot by |re| + |im| as LAPACK does); zero pivots are kept, so
/// a singular matrix gives Inf/NaN rather than an error.
fn lu(ar: Ar, a: &Mat) -> (Mat, Vec<usize>) {
    let n = a.r;
    let mut f = a.clone();
    let mut perm: Vec<usize> = (0..n).collect();
    for k in 0..n {
        let p = (k..n).fold(k, |best, i| if mag1(f.at(i, k)) > mag1(f.at(best, k)) { i } else { best });
        if p != k {
            for j in 0..n {
                let (x, y) = (f.at(k, j), f.at(p, j));
                f.set(k, j, y);
                f.set(p, j, x);
            }
            perm.swap(k, p);
        }
        let piv = f.at(k, k);
        if piv == C::new(0.0, 0.0) {
            continue;
        }
        for i in k + 1..n {
            let l = ar.div(f.at(i, k), piv);
            f.set(i, k, l);
        }
        for j in k + 1..n {
            let ukj = f.at(k, j);
            for i in k + 1..n {
                let v = ar.sub(f.at(i, j), ar.mul(f.at(i, k), ukj));
                f.set(i, j, v);
            }
        }
    }
    (f, perm)
}

fn norm1(m: &Mat) -> f64 {
    (0..m.c).map(|j| (0..m.r).map(|i| m.at(i, j).abs()).sum::<f64>()).fold(0.0, f64::max)
}

/// Solves square `a x = b`, with the reciprocal condition number MATLAB reports for the page:
/// the diagonal ratio for triangular matrices, else the 1-norm condition.
fn solve_square(ar: Ar, a: &Mat, b: &Mat) -> (Mat, f64) {
    let n = a.r;
    let (upper, lower) = (is_upper(a), is_lower(a));
    if n == 0 {
        return (Mat::zeros(0, b.c), f64::INFINITY);
    }
    if upper || lower {
        let d: Vec<f64> = (0..n).map(|i| a.at(i, i).abs()).collect();
        let (lo, hi) = d.iter().fold((f64::INFINITY, 0.0f64), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        let rcond = if lo == 0.0 { 0.0 } else { lo / hi };
        let mut x = b.clone();
        // A diagonal matrix is solved forwards, as MATLAB does (zeros(2)\[1;1] is [Inf; NaN]).
        tri_solve(ar, a, &mut x, !lower, false);
        return (x, rcond);
    }
    let (f, perm) = lu(ar, a);
    let lu_solve = |rhs: &Mat| -> Mat {
        let mut x = Mat::zeros(n, rhs.c);
        for j in 0..rhs.c {
            for (i, &p) in perm.iter().enumerate() {
                x.set(i, j, rhs.at(p, j));
            }
        }
        tri_solve(ar, &f, &mut x, false, true);
        tri_solve(ar, &f, &mut x, true, false);
        x
    };
    let x = lu_solve(b);
    let singular = (0..n).any(|i| f.at(i, i) == C::new(0.0, 0.0));
    let rcond = if singular {
        0.0
    } else {
        let mut eye = Mat::zeros(n, n);
        (0..n).for_each(|i| eye.set(i, i, C::new(1.0, 0.0)));
        let inv = lu_solve(&eye);
        let c = norm1(a) * norm1(&inv);
        if c.is_finite() { 1.0 / c } else { 0.0 }
    };
    (x, rcond)
}

/// Householder vector for column `x`: (v with v[0] = 1, tau, beta) as LAPACK's larfg.
fn householder(x: &[C]) -> (Vec<C>, C, C) {
    let alpha = x[0];
    let xnorm = x[1..].iter().map(|z| z.abs()).fold(0.0f64, f64::hypot);
    let mut v = vec![C::new(1.0, 0.0); x.len()];
    if xnorm == 0.0 && alpha.im == 0.0 {
        return (v, C::new(0.0, 0.0), alpha);
    }
    let norm = alpha.abs().hypot(xnorm);
    let beta = if alpha.re >= 0.0 { -norm } else { norm };
    let tau = C::new((beta - alpha.re) / beta, -alpha.im / beta);
    let scale = C::new(1.0, 0.0).div(alpha.sub(C::new(beta, 0.0)));
    for (vi, xi) in v.iter_mut().zip(x).skip(1) {
        *vi = xi.mul(scale);
    }
    (v, tau, C::new(beta, 0.0))
}

/// Least-squares / minimum-pivot solution of a rectangular system through QR with column
/// pivoting, as mldivide does: rank-deficient columns get zeros (the basic solution).
fn solve_rect(ar: Ar, a: &Mat, b: &Mat) -> (Mat, f64) {
    let (m, n) = (a.r, a.c);
    let mut r = a.clone();
    let mut qtb = b.clone();
    let mut perm: Vec<usize> = (0..n).collect();
    let k = m.min(n);
    for s in 0..k {
        let colnorm = |r: &Mat, j: usize| (s..m).map(|i| r.at(i, j).abs()).fold(0.0f64, f64::hypot);
        let p = (s..n).fold(s, |best, j| if colnorm(&r, j) > colnorm(&r, best) { j } else { best });
        if p != s {
            for i in 0..m {
                let (x, y) = (r.at(i, s), r.at(i, p));
                r.set(i, s, y);
                r.set(i, p, x);
            }
            perm.swap(s, p);
        }
        let col: Vec<C> = (s..m).map(|i| r.at(i, s)).collect();
        let (v, tau, beta) = householder(&col);
        // Apply H' = I - conj(tau) v v' to the remaining columns and to b.
        let apply = |mat: &mut Mat, j: usize| {
            let mut w = C::new(0.0, 0.0);
            for (t, vi) in v.iter().enumerate() {
                w = w.add(vi.conj().mul(mat.at(s + t, j)));
            }
            let f = tau.conj().mul(w);
            for (t, vi) in v.iter().enumerate() {
                let x = mat.at(s + t, j).sub(vi.mul(f));
                mat.set(s + t, j, x);
            }
        };
        for j in s + 1..n {
            apply(&mut r, j);
        }
        for j in 0..qtb.c {
            apply(&mut qtb, j);
        }
        r.set(s, s, beta);
        for i in s + 1..m {
            r.set(i, s, C::new(0.0, 0.0));
        }
    }
    let r11 = if k > 0 { r.at(0, 0).abs() } else { 0.0 };
    let tol = m.max(n) as f64 * f64::EPSILON * r11;
    let rank = (0..k).take_while(|&i| r.at(i, i).abs() > tol).count();
    let mut tri = Mat::zeros(rank, rank);
    let mut rhs = Mat::zeros(rank, b.c);
    for i in 0..rank {
        for j in i..rank {
            tri.set(i, j, r.at(i, j));
        }
        for j in 0..b.c {
            rhs.set(i, j, qtb.at(i, j));
        }
    }
    tri_solve(ar, &tri, &mut rhs, true, false);
    let mut x = Mat::zeros(n, b.c);
    for (i, &p) in perm.iter().enumerate().take(rank) {
        for j in 0..b.c {
            x.set(p, j, rhs.at(i, j));
        }
    }
    let mut rk = Mat::zeros(k, k);
    for i in 0..k {
        for j in i..k {
            rk.set(i, j, r.at(i, j));
        }
    }
    let rcond = if rank < k || k == 0 {
        0.0
    } else {
        let mut inv = Mat::zeros(k, k);
        (0..k).for_each(|i| inv.set(i, i, C::new(1.0, 0.0)));
        tri_solve(Ar { single: false }, &rk, &mut inv, true, false);
        1.0 / (norm1(&rk) * norm1(&inv))
    };
    (Mat { r: n, c: b.c, d: x.d.iter().map(|&z| ar.rc(z)).collect() }, rcond)
}

/// `a \ b` for one page; `None` when the row counts disagree.
fn page_solve(ar: Ar, a: &Mat, b: &Mat) -> Option<(Mat, f64)> {
    if a.r == 1 && a.c == 1 {
        let s = a.d[0];
        let rcond = if s == C::new(0.0, 0.0) { 0.0 } else { 1.0 };
        return Some((Mat { r: b.r, c: b.c, d: b.d.iter().map(|&v| ar.div(v, s)).collect() }, rcond));
    }
    if a.r != b.r {
        return None;
    }
    Some(if a.r == a.c { solve_square(ar, a, b) } else { solve_rect(ar, a, b) })
}

fn page_divide(name: &str, a: &[Value]) -> Ret {
    nargs(a, 2, 3)?;
    let left = name == "pagemldivide";
    // pagemldivide(A, [transpA,] B) and pagemrdivide(B, A [, transpA]).
    let (av, bv, opt) = match (left, a.len()) {
        (true, 2) => (&a[0], &a[1], None),
        (true, _) => (&a[0], &a[2], Some(&a[1])),
        (false, 2) => (&a[1], &a[0], None),
        (false, _) => (&a[1], &a[0], Some(&a[2])),
    };
    let (am, bm) = (float_arg(av)?, float_arg(bv)?);
    let opt = opt.map(transp_option).transpose()?.unwrap_or((false, false));
    let ar = Ar { single: am.kind == Kind::Single || bm.kind == Kind::Single };
    let (page_dims, pairs) = page_pairs(&am.dims, &bm.dims)?;
    let mut pages = Vec::with_capacity(pairs.len());
    let mut rconds = Vec::with_capacity(pairs.len());
    for &(p, q) in &pairs {
        let pa = apply_option(page(&am, p), opt);
        let pb = page(&bm, q);
        // B / A is (A.' \ B.').'.
        let (sa, sb) = if left { (pa.clone(), pb) } else { (pa.transposed(false), pb.transposed(false)) };
        let Some((x, mut rc)) = page_solve(ar, &sa, &sb) else {
            return rt(if left {
                "Incorrect dimensions for matrix division. Check that the number of rows in the first array matches the number of rows in the second array, unless first input is scalar."
            } else {
                "Incorrect dimensions for matrix division. Check that the number of columns in the first array matches the number of columns in the second array, unless second input is scalar."
            });
        };
        if !left && pa.r == pa.c && pa.r > 1 {
            // The condition number is that of A as given, not of the transposed system.
            rc = solve_square(ar, &pa, &Mat::zeros(pa.r, 0)).1;
        }
        pages.push(if left { x } else { x.transposed(false) });
        rconds.push(Mat { r: 1, c: 1, d: vec![C::new(ar.r(rc), 0.0)] });
    }
    let (r, c) = match pages.first() {
        Some(p) => (p.r, p.c),
        None => {
            // No pages: the shape follows from the operand sizes.
            let (ar_, ac) = if opt.0 { (am.dims[1], am.dims[0]) } else { (am.dims[0], am.dims[1]) };
            let scalar = ar_ == 1 && ac == 1;
            if left {
                if scalar { (bm.dims[0], bm.dims[1]) } else { (ac, bm.dims[1]) }
            } else if scalar {
                (bm.dims[0], bm.dims[1])
            } else {
                (bm.dims[0], ar_)
            }
        }
    };
    let x = assemble(pages, r, c, &page_dims, ar.single, am.is_complex() || bm.is_complex());
    let rcond = assemble(rconds, 1, 1, &page_dims, ar.single, false);
    Ok(Some(vec![Value::Num(x), Value::Num(rcond)]))
}

// ---- tensorprod ----------------------------------------------------------------------------

fn dim_list(v: &Value) -> R<Vec<usize>> {
    const BAD: &str = "Dimension argument must be a positive integer scalar or a vector of unique positive integers.";
    let m = match v {
        Value::Num(m) if !m.is_complex() && m.kind != Kind::Char => m,
        _ => return rt(BAD),
    };
    let mut out = Vec::with_capacity(m.numel());
    for &x in &m.data {
        if x < 1.0 || x.fract() != 0.0 || !x.is_finite() || out.contains(&(x as usize)) {
            return rt(BAD);
        }
        out.push(x as usize);
    }
    Ok(out)
}

fn tensorprod(a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    let (x, y) = (float_arg(&a[0])?, float_arg(&a[1])?);
    let mut rest: Vec<&Value> = a[2..].iter().collect();
    // Trailing name-value options.
    let (mut nda, mut ndb) = (0usize, 0usize);
    while rest.len() >= 2 && rest[rest.len() - 2].is_text() && !rest[rest.len() - 2].text()?.eq_ignore_ascii_case("all") {
        let key = rest[rest.len() - 2].text()?;
        let val = rest[rest.len() - 1].as_usize()?;
        match key.to_ascii_lowercase().as_str() {
            "numdimensionsa" => nda = val,
            "numdimensionsb" => ndb = val,
            _ => return rt(format!("Invalid parameter name: {key}.")),
        }
        rest.truncate(rest.len() - 2);
    }
    let ar = Ar { single: x.kind == Kind::Single || y.kind == Kind::Single };
    const SIZES: &str = "Specified dimensions of the input arrays must have the same sizes.";
    let all = rest.len() == 1 && rest[0].is_text() && rest[0].text()?.eq_ignore_ascii_case("all");
    if all {
        if x.dims != y.dims {
            return rt(SIZES);
        }
        let mut s = C::new(0.0, 0.0);
        for i in 0..x.numel() {
            s = ar.add(s, ar.mul(C::new(x.data[i], x.im_at(i)), C::new(y.data[i], y.im_at(i))));
        }
        let out = assemble(vec![Mat { r: 1, c: 1, d: vec![s] }], 1, 1, &[], false, x.is_complex() || y.is_complex());
        return one(Value::Num(if ar.single { out.with_kind(Kind::Single) } else { out }));
    }
    let (da, db) = match rest.len() {
        0 => (Vec::new(), Vec::new()),
        1 => {
            let d = dim_list(rest[0])?;
            (d.clone(), d)
        }
        2 => (dim_list(rest[0])?, dim_list(rest[1])?),
        _ => return rt("Too many input arguments."),
    };
    if da.len() != db.len() {
        return rt("Dimension arguments must have the same length.");
    }
    let pad = |m: &Matrix, dims: &[usize], nd: usize| -> Dims {
        let n = m.dims.len().max(dims.iter().copied().max().unwrap_or(0)).max(nd);
        let mut d = m.dims.clone();
        d.resize(n, 1);
        d
    };
    let (sa, sb) = (pad(&x, &da, nda), pad(&y, &db, ndb));
    if da.iter().zip(&db).any(|(&p, &q)| sa[p - 1] != sb[q - 1]) {
        return rt(SIZES);
    }
    let free_a: Vec<usize> = (0..sa.len()).filter(|k| !da.contains(&(k + 1))).collect();
    let free_b: Vec<usize> = (0..sb.len()).filter(|k| !db.contains(&(k + 1))).collect();
    let ord_a: Vec<usize> = free_a.iter().copied().chain(da.iter().map(|d| d - 1)).collect();
    let ord_b: Vec<usize> = db.iter().map(|d| d - 1).chain(free_b.iter().copied()).collect();
    let flat = |m: &Matrix, dims: &Dims, ord: &[usize], rows: usize| -> R<Mat> {
        let z: Vec<C> = (0..m.numel()).map(|i| C::new(m.data[i], m.im_at(i))).collect();
        let (_, d) = nd::permute(dims, &z, ord)?;
        Ok(Mat { r: rows, c: d.len().checked_div(rows).unwrap_or(0), d })
    };
    let k: usize = da.iter().map(|&d| sa[d - 1]).product();
    let ma: usize = free_a.iter().map(|&d| sa[d]).product();
    let nb: usize = free_b.iter().map(|&d| sb[d]).product();
    let pa = flat(&x, &sa, &ord_a, ma)?;
    let pb = flat(&y, &sb, &ord_b, k)?;
    let pa = Mat { r: ma, c: k, d: pa.d };
    let pb = Mat { r: k, c: nb, d: pb.d };
    let prod = mat_mul(ar, &pa, &pb);
    let mut dims: Dims = free_a.iter().map(|&d| sa[d]).chain(free_b.iter().map(|&d| sb[d])).collect();
    while dims.len() < 2 {
        dims.push(1);
    }
    let out = assemble(vec![prod], ma, nb, &[], false, x.is_complex() || y.is_complex()).reshaped(nd::normalize(dims));
    one(Value::Num(if ar.single { out.with_kind(Kind::Single) } else { out }))
}

// ---- indexing hooks ------------------------------------------------------------------------

fn substruct(a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("SUBSTRUCT takes at least two arguments.");
    }
    if !a.len().is_multiple_of(2) {
        return rt("SUBSTRUCT takes an even number of arguments.");
    }
    let elems: Vec<Vec<Value>> = a.chunks(2).map(|p| vec![p[0].clone(), p[1].clone()]).collect();
    let n = elems.len();
    one(Value::Struct(StructArray { dims: vec![1, n], fields: vec!["type".into(), "subs".into()], elems }))
}

/// One level of a subscript structure.
enum Sub {
    Paren(Vec<Value>),
    Brace(Vec<Value>),
    Dot(String),
}

fn parse_subs(s: &Value) -> R<Vec<Sub>> {
    let Value::Struct(s) = s else {
        return rt("Subscript argument to SUBSREF and SUBSASGN must be a structure.");
    };
    if s.fields.len() != 2 {
        return rt("Subscript argument to SUBSREF and SUBSASGN must have two fields.");
    }
    let (Some(ti), Some(si)) = (s.field_index("type"), s.field_index("subs")) else {
        return rt("Subscript argument to SUBSREF and SUBSASGN must have two fields whose names are \"type\" and \"subs\".");
    };
    if s.elems.is_empty() {
        return rt("Subscript argument to SUBSREF and SUBSASGN must not be empty.");
    }
    let mut out = Vec::with_capacity(s.elems.len());
    for e in &s.elems {
        let ty = if e[ti].is_text() { e[ti].text()? } else { String::new() };
        let subs = &e[si];
        let cell_subs = |what: &str| -> R<Vec<Value>> {
            match subs {
                Value::Cell(c) => Ok(c.data.clone()),
                _ => rt(format!("SUBS field must be a cell array for {what} TYPE.")),
            }
        };
        out.push(match ty.as_str() {
            "()" => Sub::Paren(cell_subs("()")?),
            "{}" => Sub::Brace(cell_subs("{}")?),
            "." => {
                if !subs.is_text() {
                    return rt("Argument to dynamic structure reference must evaluate to a valid field name.");
                }
                Sub::Dot(subs.text()?)
            }
            _ => return rt("The \"type\" field in the indexing structure argument to SUBSREF and SUBSASGN must be a string scalar or character vector of \".\" or \"{}\" or \"()\"."),
        });
    }
    if out.windows(2).any(|w| matches!(w[0], Sub::Paren(_)) && !matches!(w[1], Sub::Dot(_))) {
        return rt("Only a dot field name can follow ()'s.");
    }
    Ok(out)
}

/// Temporary variables holding the base value and the subscripts; names that are not valid
/// identifiers, so they cannot clash with user variables.
struct Temps {
    names: Vec<String>,
}

impl Temps {
    fn put(&mut self, it: &mut Interpreter, v: Value) -> String {
        let name = format!("#ops{}", self.names.len());
        it.set_var(&name, v);
        self.names.push(name.clone());
        name
    }

    fn args(&mut self, it: &mut Interpreter, vals: &[Value]) -> Vec<Expr> {
        vals.iter()
            .map(|v| match v {
                Value::Num(m) if m.kind == Kind::Char && m.to_string_lossy() == ":" => Expr::Colon,
                _ => Expr::Ident(self.put(it, v.clone())),
            })
            .collect()
    }

    fn clear(self, it: &mut Interpreter) {
        for n in &self.names {
            it.remove_var(n);
        }
    }
}

fn subsref(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    nargs(a, 2, 2)?;
    let subs = parse_subs(&a[1])?;
    let mut t = Temps { names: Vec::new() };
    let mut e = Expr::Ident(t.put(it, a[0].clone()));
    for s in &subs {
        e = match s {
            // A() is the whole value.
            Sub::Paren(v) if v.is_empty() => e,
            Sub::Paren(v) => Expr::Index(Box::new(e), t.args(it, v)),
            Sub::Brace(v) => Expr::CellIndex(Box::new(e), t.args(it, v)),
            Sub::Dot(f) => Expr::Field(Box::new(e), f.clone()),
        };
    }
    let r = it.eval_multi(&e, nargout.max(1));
    t.clear(it);
    let mut vals = r?;
    vals.truncate(nargout.max(1));
    Ok(Some(vals))
}

fn subsasgn(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if nargout == 0 {
        return rt("SUBSASGN must be called with an output.");
    }
    nargs(a, 3, 3)?;
    let subs = parse_subs(&a[1])?;
    let mut t = Temps { names: Vec::new() };
    let base = t.put(it, a[0].clone());
    let rhs = t.put(it, a[2].clone());
    let chain: Vec<Accessor> = subs
        .iter()
        .map(|s| match s {
            Sub::Paren(v) => Accessor::Paren(t.args(it, v)),
            Sub::Brace(v) => Accessor::Brace(t.args(it, v)),
            Sub::Dot(f) => Accessor::Field(f.clone()),
        })
        .collect();
    let stmt = Stmt { kind: StmtKind::Assign { lhs: vec![LValue::Target { name: base.clone(), chain }], rhs: Expr::Ident(rhs), print: false }, line: 0 };
    let r = it.exec_block(std::slice::from_ref(&stmt));
    let out = it.get_var(&base).cloned();
    t.clear(it);
    r?;
    one(out.unwrap_or_else(Value::empty))
}

// ---- metaclass -----------------------------------------------------------------------------

fn meta_class_def(name: &str, props: &[&str]) -> Rc<ClassDef> {
    Rc::new(ClassDef {
        name: name.into(),
        supers: vec!["handle".into()],
        ancestors: vec!["handle".into()],
        is_handle: true,
        props: props.iter().map(|p| PropDef { name: (*p).into(), default: None, constant: false, dependent: false, hidden: false }).collect(),
        methods: HashMap::new(),
        statics: HashSet::new(),
        ctor: None,
    })
}

/// A struct array of `n` elements with the given fields, filled row by row.
fn struct_list(fields: &[&str], rows: Vec<Vec<Value>>) -> Value {
    let n = rows.len();
    Value::Struct(StructArray { dims: vec![n, 1], fields: fields.iter().map(|f| (*f).into()).collect(), elems: rows })
}

const CLASS_PROPS: &[&str] = &[
    "Name", "Description", "DetailedDescription", "Hidden", "Sealed", "Abstract", "Enumeration", "ConstructOnLoad", "HandleCompatible", "InferiorClasses",
    "ContainingPackage", "Aliases", "RestrictsSubclassing", "PropertyList", "MethodList", "EventList", "EnumerationMemberList", "SuperclassList",
];

/// `metaclass(obj)`: a meta.class object describing the class of `obj`. The member lists
/// are struct arrays (object arrays are not available) carrying the main attributes.
fn metaclass(it: &mut Interpreter, v: &Value) -> Value {
    let class = match v {
        Value::Object(o) => Some(o.class.clone()),
        _ => None,
    };
    let name = v.class_name();
    let empty_cell = || Value::Cell(CellArray::from_dims(vec![0, 1], Vec::new()));
    let props = match &class {
        Some(c) => struct_list(
            &["Name", "Description", "DetailedDescription", "GetAccess", "SetAccess", "Dependent", "Constant", "Abstract", "Transient", "Hidden", "HasDefault", "DefiningClass"],
            c.props
                .iter()
                .map(|p| {
                    vec![
                        Value::chars(&p.name),
                        Value::chars(""),
                        Value::chars(""),
                        Value::chars("public"),
                        Value::chars("public"),
                        Value::bool(p.dependent),
                        Value::bool(p.constant),
                        Value::bool(false),
                        Value::bool(false),
                        Value::bool(p.hidden),
                        Value::bool(p.default.is_some()),
                        Value::chars(&c.name),
                    ]
                })
                .collect(),
        ),
        None => struct_list(&["Name"], Vec::new()),
    };
    let methods = match &class {
        Some(c) => {
            let mut names: Vec<(&String, &String)> = c.methods.iter().map(|(k, (_, def))| (k, def)).collect();
            if c.ctor.is_some() {
                names.push((&c.name, &c.name));
            }
            names.sort();
            struct_list(
                &["Name", "Description", "DetailedDescription", "Access", "Static", "Abstract", "Sealed", "Hidden", "DefiningClass"],
                names
                    .into_iter()
                    .map(|(m, def)| {
                        vec![
                            Value::chars(m),
                            Value::chars(""),
                            Value::chars(""),
                            Value::chars("public"),
                            Value::bool(c.statics.contains(m)),
                            Value::bool(false),
                            Value::bool(false),
                            Value::bool(false),
                            Value::chars(def),
                        ]
                    })
                    .collect(),
            )
        }
        None => struct_list(&["Name"], Vec::new()),
    };
    let supers = match &class {
        Some(c) => struct_list(
            &["Name", "HandleCompatible"],
            c.supers
                .iter()
                .map(|s| {
                    let handle = s == "handle" || it.find_class(s).is_some_and(|d| d.is_handle);
                    vec![Value::chars(s), Value::bool(handle)]
                })
                .collect(),
        ),
        None => struct_list(&["Name"], Vec::new()),
    };
    let handle = class.as_ref().is_some_and(|c| c.is_handle);
    let values = vec![
        Value::chars(&name),
        Value::chars(""),
        Value::chars(""),
        Value::bool(false),
        Value::bool(false),
        Value::bool(false),
        Value::bool(false),
        Value::bool(false),
        Value::bool(handle),
        empty_cell(),
        Value::empty(),
        Value::Str(StrArray::from_dims(vec![0, 1], Vec::new())),
        Value::bool(false),
        props,
        methods,
        struct_list(&["Name"], Vec::new()),
        struct_list(&["Name"], Vec::new()),
        supers,
    ];
    Value::Object(Object { class: meta_class_def("meta.class", CLASS_PROPS), props: Rc::new(RefCell::new(values)) })
}
