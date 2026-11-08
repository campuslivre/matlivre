//! Operators with MATLAB class rules, complex support and N-D implicit expansion.

use crate::ast::{BinOp, UnOp};
use crate::cplx::C;
use crate::display::fmt_g;
use crate::error::{rt, LangError};
use crate::nd;
use crate::value::{CellArray, Kind, Matrix, StrArray, Value};

type R<T> = Result<T, LangError>;

/// Elementwise real `f(a, b)` with implicit expansion; result is a real double array.
pub fn broadcast(a: &Matrix, b: &Matrix, f: impl Fn(f64, f64) -> f64 + Sync + Send) -> R<Matrix> {
    use rayon::prelude::*;
    let big = a.numel().max(b.numel()) >= PAR_MIN;
    if a.dims == b.dims {
        let data = if big {
            a.data.par_iter().zip(&b.data).map(|(&x, &y)| f(x, y)).collect()
        } else {
            a.data.iter().zip(&b.data).map(|(&x, &y)| f(x, y)).collect()
        };
        return Ok(Matrix::from_dims(a.dims.clone(), data));
    }
    if a.is_scalar() {
        let x = a.data[0];
        let data = if big { b.data.par_iter().map(|&y| f(x, y)).collect() } else { b.data.iter().map(|&y| f(x, y)).collect() };
        return Ok(Matrix::from_dims(b.dims.clone(), data));
    }
    if b.is_scalar() {
        let y = b.data[0];
        let data = if big { a.data.par_iter().map(|&x| f(x, y)).collect() } else { a.data.iter().map(|&x| f(x, y)).collect() };
        return Ok(Matrix::from_dims(a.dims.clone(), data));
    }
    // General implicit expansion: each line along the first dimension is computed with fixed
    // strides (0 on an expanded dimension), lines in parallel for large results.
    let dims = nd::broadcast_dims(&a.dims, &b.dims)?;
    let n = nd::numel(&dims);
    if n == 0 {
        return Ok(Matrix::from_dims(dims, Vec::new()));
    }
    let rank = dims.len();
    let strides_of = |d: &[usize]| -> Vec<usize> {
        let mut s = vec![0; rank];
        let mut acc = 1;
        for (k, sk) in s.iter_mut().enumerate() {
            let len = *d.get(k).unwrap_or(&1);
            *sk = if len == 1 { 0 } else { acc };
            acc *= len;
        }
        s
    };
    let (sa, sb) = (strides_of(&a.dims), strides_of(&b.dims));
    let line = |l: usize, out: &mut [f64]| {
        let (mut oa, mut ob, mut rem) = (0, 0, l);
        for k in 1..rank {
            let sub = rem % dims[k];
            rem /= dims[k];
            oa += sub * sa[k];
            ob += sub * sb[k];
        }
        for (x, o) in out.iter_mut().enumerate() {
            *o = f(a.data[oa + x * sa[0]], b.data[ob + x * sb[0]]);
        }
    };
    let mut data = vec![0.0; n];
    if big && n / dims[0] >= 2 {
        data.par_chunks_mut(dims[0]).enumerate().for_each(|(l, c)| line(l, c));
    } else {
        data.chunks_mut(dims[0]).enumerate().for_each(|(l, c)| line(l, c));
    }
    Ok(Matrix::from_dims(dims, data))
}

/// Element count from which elementwise work is split across threads.
pub const PAR_MIN: usize = 1 << 15;

/// Elementwise complex `f(a, b)` with implicit expansion.
pub fn broadcast_c(a: &Matrix, b: &Matrix, f: impl Fn(C, C) -> C) -> R<Matrix> {
    let ca = |i: usize| C::new(a.data[i], a.im_at(i));
    let cb = |j: usize| C::new(b.data[j], b.im_at(j));
    let (dims, pairs): (nd::Dims, Vec<(usize, usize)>) = if a.dims == b.dims {
        (a.dims.clone(), (0..a.numel()).map(|i| (i, i)).collect())
    } else {
        nd::broadcast_pairs(&a.dims, &b.dims)?
    };
    let mut re = Vec::with_capacity(pairs.len());
    let mut im = Vec::with_capacity(pairs.len());
    for (i, j) in pairs {
        let z = f(ca(i), cb(j));
        re.push(z.re);
        im.push(z.im);
    }
    Ok(Matrix::complex(dims, re, im))
}

pub fn map_c(m: &Matrix, f: impl Fn(C) -> C) -> Matrix {
    let mut re = Vec::with_capacity(m.numel());
    let mut im = Vec::with_capacity(m.numel());
    for i in 0..m.numel() {
        let z = f(C::new(m.data[i], m.im_at(i)));
        re.push(z.re);
        im.push(z.im);
    }
    Matrix::complex(m.dims.clone(), re, im)
}

fn b2f(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

/// Result class of an arithmetic operation (MATLAB combination rules).
pub fn arith_kind(a: Kind, b: Kind) -> R<Kind> {
    Ok(match (a, b) {
        (Kind::Int(x), Kind::Int(y)) if x == y => Kind::Int(x),
        (Kind::Int(x), Kind::Int(y)) => {
            return rt(format!("Integers can only be combined with integers of the same class, or scalar doubles. ({} and {})", x.name(), y.name()))
        }
        (Kind::Int(x), _) | (_, Kind::Int(x)) => Kind::Int(x),
        (Kind::Single, _) | (_, Kind::Single) => Kind::Single,
        _ => Kind::Double,
    })
}

pub fn binary(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    if a.is_compressed() || b.is_compressed() {
        return crate::sparse_ops::binary(op, a, b);
    }
    let mut out = binary_dense(op, a, b)?;
    if crate::builtins_sparse::binary_sparse(op, a, b) {
        if let Value::Num(m) = &mut out {
            m.sparse = true;
        }
        out = crate::builtins_sparse::compress(out);
    }
    Ok(out)
}

fn binary_dense(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    if matches!(a, Value::Object(_)) || matches!(b, Value::Object(_)) {
        if let Some(r) = crate::lang_more::enum_compare(op, a, b)? {
            return Ok(r);
        }
    }
    if matches!(a, Value::Sym(_)) || matches!(b, Value::Sym(_)) {
        return crate::sym::binary(op, a, b);
    }
    if matches!(a, Value::Time(_)) || matches!(b, Value::Time(_)) {
        return crate::time::binary(op, a, b);
    }
    if matches!(a, Value::Str(_)) || matches!(b, Value::Str(_)) {
        return string_binary(op, a, b);
    }
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => num_binary(op, x, y).map(Value::Num),
        _ => {
            let bad = if matches!(a, Value::Num(_)) { b } else { a };
            rt(format!("Operator '{}' is not supported for operands of type '{}'.", op.symbol(), bad.class_name()))
        }
    }
}

// ---- strings -----------------------------------------------------------------------------

/// Converts an operand of a string operation to a string array (MATLAB `string()` rules).
pub fn to_str_array(v: &Value) -> R<StrArray> {
    Ok(match v {
        Value::Str(s) => s.clone(),
        Value::Num(m) if m.kind == Kind::Char => {
            // A char matrix becomes one string per row.
            let (rows, cols) = (m.rows(), m.cols());
            if rows <= 1 {
                StrArray::scalar(m.to_string_lossy())
            } else {
                StrArray::col((0..rows).map(|r| (0..cols).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or('?')).collect()).collect())
            }
        }
        Value::Num(m) => StrArray::from_dims(
            m.dims.clone(),
            (0..m.numel())
                .map(|i| {
                    let x = m.data[i];
                    if m.kind == Kind::Logical {
                        Some(if x != 0.0 { "true".into() } else { "false".into() })
                    } else if x.is_nan() && m.kind == Kind::Double {
                        None
                    } else if m.is_complex() {
                        Some(format!("{}{}{}i", fmt_g(x, 5), if m.im_at(i) < 0.0 { "-" } else { "+" }, fmt_g(m.im_at(i).abs(), 5)))
                    } else {
                        Some(fmt_g(x, 5))
                    }
                })
                .collect(),
        ),
        Value::Cell(c) => StrArray::from_dims(c.dims.clone(), c.data.iter().map(|x| x.text().map(Some)).collect::<R<_>>()?),
        Value::Time(t) => StrArray::from_dims(t.dims.clone(), (0..t.data.len()).map(|i| Some(t.format_elem(i))).collect()),
        other => return rt(format!("Conversion to string from {} is not possible.", other.class_name())),
    })
}

fn string_binary(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    let x = to_str_array(a)?;
    let y = to_str_array(b)?;
    let pairs: (nd::Dims, Vec<(usize, usize)>) = if x.dims == y.dims {
        (x.dims.clone(), (0..x.data.len()).map(|i| (i, i)).collect())
    } else {
        nd::broadcast_pairs(&x.dims, &y.dims)?
    };
    let (dims, pairs) = pairs;
    match op {
        BinOp::Add => Ok(Value::Str(StrArray::from_dims(
            dims,
            pairs
                .into_iter()
                .map(|(i, j)| match (&x.data[i], &y.data[j]) {
                    (Some(p), Some(q)) => Some(format!("{p}{q}")),
                    _ => None,
                })
                .collect(),
        ))),
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let data = pairs
                .into_iter()
                .map(|(i, j)| match (&x.data[i], &y.data[j]) {
                    (Some(p), Some(q)) => b2f(match op {
                        BinOp::Eq => p == q,
                        BinOp::Ne => p != q,
                        BinOp::Lt => p < q,
                        BinOp::Le => p <= q,
                        BinOp::Gt => p > q,
                        _ => p >= q,
                    }),
                    // Comparisons with <missing> are false (and ~= is true).
                    _ => b2f(op == BinOp::Ne),
                })
                .collect();
            Ok(Value::Num(Matrix::from_dims(dims, data).with_kind(Kind::Logical)))
        }
        _ => rt(format!("Operator '{}' is not supported for operands of type 'string'.", op.symbol())),
    }
}

// ---- numeric -----------------------------------------------------------------------------

pub fn num_binary(op: BinOp, x: &Matrix, y: &Matrix) -> R<Matrix> {
    use BinOp::*;
    // Fast path: real double scalars.
    if x.kind == Kind::Double && y.kind == Kind::Double && x.is_scalar() && y.is_scalar() && x.im.is_none() && y.im.is_none() {
        let (a, b) = (x.data[0], y.data[0]);
        let real = |v: f64| Ok(Matrix::scalar(v));
        let logical = |v: bool| Ok(Matrix::scalar(b2f(v)).with_kind(Kind::Logical));
        match op {
            Add => return real(a + b),
            Sub => return real(a - b),
            Mul | MatMul => return real(a * b),
            Div | MatDiv => return real(a / b),
            LeftDiv | MatLeftDiv => return real(b / a),
            Pow | MatPow if a >= 0.0 || b.fract() == 0.0 => return real(a.powf(b)),
            Eq => return logical(a == b),
            Ne => return logical(a != b),
            Lt => return logical(a < b),
            Le => return logical(a <= b),
            Gt => return logical(a > b),
            Ge => return logical(a >= b),
            And | AndAnd | Or | OrOr if a.is_nan() || b.is_nan() => return rt("NaN values cannot be converted to logicals."),
            And | AndAnd => return logical(a != 0.0 && b != 0.0),
            Or | OrOr => return logical(a != 0.0 || b != 0.0),
            _ => {}
        }
    }
    let complex = x.is_complex() || y.is_complex();
    let logical = |m: Matrix| m.with_kind(Kind::Logical);
    match op {
        Eq | Ne => {
            if complex {
                let m = broadcast_c(x, y, |p, q| C::new(b2f((p == q) == (op == Eq)), 0.0))?;
                return Ok(logical(Matrix::from_dims(m.dims, m.data)));
            }
            return Ok(logical(broadcast(x, y, |a, b| b2f((a == b) == (op == Eq)))?));
        }
        Lt => return Ok(logical(broadcast(x, y, |a, b| b2f(a < b))?)),
        Le => return Ok(logical(broadcast(x, y, |a, b| b2f(a <= b))?)),
        Gt => return Ok(logical(broadcast(x, y, |a, b| b2f(a > b))?)),
        Ge => return Ok(logical(broadcast(x, y, |a, b| b2f(a >= b))?)),
        And | AndAnd | Or | OrOr => {
            let nz = |m: &Matrix| -> Matrix {
                Matrix::from_dims(m.dims.clone(), (0..m.numel()).map(|i| b2f(m.data[i] != 0.0 || m.im_at(i) != 0.0)).collect())
            };
            let has_nan = |m: &Matrix| -> bool {
                use rayon::prelude::*;
                if m.numel() >= PAR_MIN { m.data.par_iter().any(|v| v.is_nan()) } else { m.data.iter().any(|v| v.is_nan()) }
            };
            if has_nan(x) || has_nan(y) {
                return rt("NaN values cannot be converted to logicals.");
            }
            let and = matches!(op, And | AndAnd);
            if !complex {
                // Real operands: test the values in place instead of copying both arrays first.
                return Ok(logical(broadcast(x, y, |a, b| b2f(if and { a != 0.0 && b != 0.0 } else { a != 0.0 || b != 0.0 }))?));
            }
            let (p, q) = (nz(x), nz(y));
            return Ok(logical(broadcast(&p, &q, |a, b| b2f(if and { a != 0.0 && b != 0.0 } else { a != 0.0 || b != 0.0 }))?));
        }
        _ => {}
    }
    let kind = arith_kind(x.kind, y.kind)?;
    if complex && kind.is_int() {
        return rt("Complex integer arithmetic is not supported.");
    }
    let out = match op {
        Add | Sub | Mul | Div | LeftDiv => {
            if complex {
                broadcast_c(x, y, |p, q| match op {
                    Add => p.add(q),
                    Sub => p.sub(q),
                    Mul => p.mul(q),
                    Div => p.div(q),
                    _ => q.div(p),
                })?
            } else {
                match op {
                    Add => broadcast(x, y, |a, b| a + b)?,
                    Sub => broadcast(x, y, |a, b| a - b)?,
                    Mul => broadcast(x, y, |a, b| a * b)?,
                    Div => broadcast(x, y, |a, b| a / b)?,
                    _ => broadcast(x, y, |a, b| b / a)?,
                }
            }
        }
        Pow => elem_pow(x, y, kind)?,
        MatMul => {
            if x.is_scalar() || y.is_scalar() {
                return num_binary(Mul, x, y);
            }
            if kind.is_int() {
                return rt("MTIMES (*) is not fully supported for integer data types. Use TIMES (.*) instead.");
            }
            matmul_any(x, y)?
        }
        MatDiv => {
            if y.is_scalar() {
                return num_binary(Div, x, y);
            }
            let xt = ctranspose(x);
            let yt = ctranspose(y);
            ctranspose(&solve_any(&yt, &xt)?)
        }
        MatLeftDiv => {
            if x.is_scalar() {
                return num_binary(LeftDiv, x, y);
            }
            solve_any(x, y)?
        }
        MatPow => {
            if x.is_scalar() && y.is_scalar() {
                return num_binary(Pow, x, y);
            }
            if y.is_scalar() && y.im.is_none() && x.is_2d() && x.rows() == x.cols() {
                let p = y.data[0];
                if p.fract() != 0.0 || !p.is_finite() {
                    let v = crate::builtins_matfun::mpower_frac(x, faer::c64::new(p, 0.0))?;
                    return v.to_matrix();
                }
                let base = if p < 0.0 { inv_any(x)? } else { x.clone().with_kind(Kind::Double) };
                let mut n = p.abs() as u64;
                let mut result = identity(x.rows());
                let mut sq = base;
                while n > 0 {
                    if n & 1 == 1 {
                        result = matmul_any(&result, &sq)?;
                    }
                    sq = matmul_any(&sq, &sq)?;
                    n >>= 1;
                }
                result
            } else if x.is_scalar() && y.is_2d() && y.rows() == y.cols() && !kind.is_int() {
                let v = crate::builtins_matfun::scalar_mpower(faer::c64::new(x.data[0], x.im_at(0)), y);
                return v.to_matrix();
            } else if y.is_scalar() && x.is_2d() && x.rows() == x.cols() && !kind.is_int() {
                let v = crate::builtins_matfun::mpower_frac(x, faer::c64::new(y.data[0], y.im_at(0)))?;
                return v.to_matrix();
            } else {
                return rt("Incorrect dimensions for raising a matrix to a power. Check that the matrix is square and the power is a scalar. To operate on each element of the matrix individually, use POWER (.^).");
            }
        }
        _ => unreachable!(),
    };
    let mut out = out.cast_to(kind);
    out.narrow();
    Ok(out)
}

fn elem_pow(x: &Matrix, y: &Matrix, kind: Kind) -> R<Matrix> {
    let needs_complex = x.is_complex()
        || y.is_complex()
        || (!kind.is_int() && x.data.iter().any(|&a| a < 0.0) && y.data.iter().any(|&b| b.fract() != 0.0 && b.is_finite()));
    if needs_complex {
        if kind.is_int() {
            return rt("Complex integer arithmetic is not supported.");
        }
        return broadcast_c(x, y, |p, q| p.powc(q));
    }
    if y.is_scalar() && y.data[0] == 2.0 {
        return broadcast(x, y, |a, _| a * a);
    }
    broadcast(x, y, f64::powf)
}

/// `'`: conjugate transpose.
pub fn ctranspose(m: &Matrix) -> Matrix {
    let mut t = m.transpose();
    if let Some(im) = t.im.as_mut() {
        for v in im.iter_mut() {
            *v = -*v;
        }
    }
    t
}

pub fn unary(op: UnOp, v: &Value) -> R<Value> {
    if v.is_compressed() {
        return crate::sparse_ops::unary(op, v);
    }
    match v {
        Value::Sym(_) => return crate::sym::unary(op, v),
        Value::Time(t) => return crate::time::unary(op, t),
        _ => {}
    }
    match (op, v) {
        (UnOp::CTranspose | UnOp::Transpose, Value::Num(m)) => {
            if !m.is_2d() {
                return rt("Transpose on ND array is not defined. Use PERMUTE instead.");
            }
            Ok(Value::Num(if op == UnOp::CTranspose { ctranspose(m) } else { m.transpose() }))
        }
        (UnOp::CTranspose | UnOp::Transpose, Value::Cell(c)) => {
            let (d, data) = nd::transpose(&c.dims, &c.data)?;
            Ok(Value::Cell(CellArray::from_dims(d, data)))
        }
        (UnOp::CTranspose | UnOp::Transpose, Value::Str(s)) => {
            let (d, data) = nd::transpose(&s.dims, &s.data)?;
            Ok(Value::Str(StrArray::from_dims(d, data)))
        }
        (UnOp::CTranspose | UnOp::Transpose, Value::Struct(s)) => {
            let (d, elems) = nd::transpose(&s.dims, &s.elems)?;
            let mut out = s.clone();
            out.dims = d;
            out.elems = elems;
            Ok(Value::Struct(out))
        }
        (UnOp::CTranspose | UnOp::Transpose, Value::Func(_) | Value::Object(_)) => Ok(v.clone()),
        (UnOp::Neg, Value::Num(m)) => {
            let kind = if matches!(m.kind, Kind::Logical | Kind::Char) { Kind::Double } else { m.kind };
            let mut out = Matrix { dims: m.dims.clone(), data: m.data.iter().map(|x| -x).collect(), im: m.im.as_ref().map(|im| im.iter().map(|x| -x).collect()), kind: Kind::Double, sparse: m.sparse, csc: Default::default() };
            out = out.cast_to(kind);
            Ok(Value::Num(out))
        }
        (UnOp::Plus, Value::Num(m)) => {
            let kind = if matches!(m.kind, Kind::Logical | Kind::Char) { Kind::Double } else { m.kind };
            Ok(Value::Num(m.clone().with_kind(kind)))
        }
        (UnOp::Not, Value::Num(m)) => {
            if m.data.iter().any(|x| x.is_nan()) {
                return rt("NaN's cannot be converted to logicals.");
            }
            let mut out = Matrix::from_dims(m.dims.clone(), (0..m.numel()).map(|i| b2f(m.data[i] == 0.0 && m.im_at(i) == 0.0)).collect()).with_kind(Kind::Logical);
            out.sparse = m.sparse;
            Ok(Value::Num(out))
        }
        _ => rt(format!("Unary operator is not supported for values of class '{}'.", v.class_name())),
    }
}

// ---- linear algebra ----------------------------------------------------------------------

pub fn identity(n: usize) -> Matrix {
    let mut m = Matrix::zeros(n, n);
    for i in 0..n {
        m.set(i, i, 1.0);
    }
    m
}

fn real_part(m: &Matrix) -> Matrix {
    Matrix::from_dims(m.dims.clone(), m.data.clone())
}

fn imag_part(m: &Matrix) -> Matrix {
    Matrix::from_dims(m.dims.clone(), m.im.clone().unwrap_or_else(|| vec![0.0; m.numel()]))
}

/// Multiply-add count from which products go to the blocked, multithreaded kernel.
const FAER_MIN_WORK: usize = 32 * 32 * 32;
/// Order from which square solves, inverses and determinants use the blocked LU.
const FAER_MIN_ORDER: usize = 24;

/// `a \ b` for square real `a` through a blocked partial-pivoting LU.
fn faer_solve(a: &Matrix, b: &Matrix) -> R<Matrix> {
    use faer::dyn_stack::{MemBuffer, MemStack};
    use faer::linalg::lu::partial_pivoting::{factor, solve};
    let (n, k) = (a.rows(), b.cols());
    // Below ~1000 the panel factorization is latency-bound and threads only add overhead.
    let par = if n < 1000 { faer::Par::Seq } else { gemm_par() };
    let req = factor::lu_in_place_scratch::<usize, f64>(n, n, par, Default::default()).or(solve::solve_in_place_scratch::<usize, f64>(n, k, par));
    let mut buf = MemBuffer::new(req);
    let stack = MemStack::new(&mut buf);
    // L (unit diagonal, strictly below) and U share one buffer. The buffer is kept between
    // calls: a fresh allocation of this size comes back from the OS as untouched pages, and
    // faulting them in costs about as much as the factorization of a 300 x 300 matrix.
    LU_BUF.with(|cell| {
        let mut lu = cell.take();
        lu.clear();
        lu.extend_from_slice(&a.data);
        let (mut fwd, mut bwd) = (vec![0usize; n], vec![0usize; n]);
        let (_, perm) = factor::lu_in_place(faer::MatMut::from_column_major_slice_mut(&mut lu, n, n), &mut fwd, &mut bwd, par, stack, Default::default());
        let singular = (0..n).any(|i| !(lu[i * n + i].abs() >= 1e-300));
        let res = if singular {
            rt("Matrix is singular to working precision.")
        } else {
            let f = faer::MatRef::from_column_major_slice(&lu, n, n);
            let mut x = b.data.clone();
            solve::solve_in_place(f, f, perm, faer::MatMut::from_column_major_slice_mut(&mut x, n, k), par, stack);
            Ok(Matrix::new(n, k, x))
        };
        cell.set(lu);
        res
    })
}

thread_local! {
    /// Working copy of the matrix for `faer_solve`, reused from one call to the next.
    static LU_BUF: std::cell::Cell<Vec<f64>> = const { std::cell::Cell::new(Vec::new()) };
}

/// Smallest order and largest density (nonzeros / n^2) for which a sparse square `a` is solved
/// with the sparse LU instead of the dense one.
const SPARSE_MIN_ORDER: usize = 100;
const SPARSE_MAX_DENSITY: f64 = 0.1;

/// Compressed sparse columns of a column-major dense array: column pointers, row indices
/// (ascending in each column) and values. Columns are scanned in parallel blocks. `None` when
/// more than `max_nnz` entries are nonzero, or, with `finite_only`, when a nonzero is not finite.
pub fn csc_from_dense(data: &[f64], rows: usize, cols: usize, max_nnz: usize, finite_only: bool) -> Option<(Vec<usize>, Vec<u32>, Vec<f64>)> {
    use rayon::prelude::*;
    const BLOCK: usize = 32;
    let scan = |c0: usize| -> Option<(Vec<usize>, Vec<u32>, Vec<f64>)> {
        let c1 = (c0 + BLOCK).min(cols);
        let (mut counts, mut idx, mut val) = (Vec::with_capacity(c1 - c0), Vec::new(), Vec::new());
        for col in data[c0 * rows..c1 * rows].chunks_exact(rows) {
            let before = val.len();
            for (i, &v) in col.iter().enumerate() {
                if v != 0.0 {
                    if finite_only && !v.is_finite() {
                        return None;
                    }
                    idx.push(i as u32);
                    val.push(v);
                }
            }
            counts.push(val.len() - before);
        }
        Some((counts, idx, val))
    };
    let starts: Vec<usize> = (0..cols).step_by(BLOCK).collect();
    let parts: Vec<Option<(Vec<usize>, Vec<u32>, Vec<f64>)>> = if rows * cols >= PAR_MIN { starts.par_iter().map(|&c| scan(c)).collect() } else { starts.iter().map(|&c| scan(c)).collect() };
    let mut ptr = Vec::with_capacity(cols + 1);
    ptr.push(0usize);
    let mut total = 0usize;
    for p in &parts {
        total += p.as_ref()?.2.len();
    }
    if total > max_nnz {
        return None;
    }
    let (mut idx, mut val) = (Vec::with_capacity(total), Vec::with_capacity(total));
    for p in parts {
        let (counts, i, v) = p?;
        for c in counts {
            ptr.push(ptr.last().unwrap() + c);
        }
        idx.extend(i);
        val.extend(v);
    }
    Some((ptr, idx, val))
}

/// Compressed columns of a real 2-D matrix, kept in the matrix itself so that repeated products
/// and solves with the same variable scan it once. `None` when it is small, complex, or has
/// more than a fifth of its elements nonzero.
pub fn csc_of(m: &Matrix) -> Option<std::sync::Arc<crate::value::Csc>> {
    let (rows, cols) = (m.rows(), m.cols());
    if rows * cols < 4096 || m.is_complex() || m.dims.len() != 2 {
        return None;
    }
    m.csc.get_or_init(|| {
        let (ptr, idx, val) = csc_from_dense(&m.data, rows, cols, rows * cols / 5, false)?;
        Some(std::sync::Arc::new(crate::value::Csc { rows, cols, ptr, idx, val, logical: false }))
    })
}

/// `a * b` for a sparse `a` and a dense `b` with a few columns: the cost is the nonzeros of `a`
/// per column of `b`.
fn sparse_times(a: &Matrix, b: &Matrix) -> Option<Matrix> {
    if b.cols() > 16 || b.is_complex() || b.dims.len() != 2 {
        return None;
    }
    let c = csc_of(a)?;
    let (k, m) = (b.cols(), c.cols);
    let mut out = vec![0.0; c.rows * k];
    for col in 0..k {
        let x = &b.data[col * m..(col + 1) * m];
        let y = &mut out[col * c.rows..(col + 1) * c.rows];
        for j in 0..m {
            let xj = x[j];
            if xj != 0.0 {
                for e in c.ptr[j]..c.ptr[j + 1] {
                    y[c.idx[e] as usize] += c.val[e] * xj;
                }
            }
        }
    }
    Some(Matrix::new(c.rows, k, out))
}

/// `x * b` for a dense `x` with a few rows and a sparse `b`.
fn times_sparse(x: &Matrix, b: &Matrix) -> Option<Matrix> {
    if x.rows() > 16 || x.is_complex() || x.dims.len() != 2 {
        return None;
    }
    let c = csc_of(b)?;
    let r = x.rows();
    let mut out = vec![0.0; r * c.cols];
    for j in 0..c.cols {
        for row in 0..r {
            let mut s = 0.0;
            for e in c.ptr[j]..c.ptr[j + 1] {
                s += c.val[e] * x.data[c.idx[e] as usize * r + row];
            }
            out[j * r + row] = s;
        }
    }
    Some(Matrix::new(r, c.cols, out))
}

/// `a \ b` for a square matrix flagged sparse, through a sparse LU with Markowitz pivoting
/// (`sparse_lu`). Returns `None` when the matrix is not sparse enough, holds non-finite values,
/// is (numerically) singular, or the result fails a residual check; the caller then takes the
/// dense path, which also produces MATLAB's singularity warnings.
fn sparse_solve(a: &Matrix, b: &Matrix) -> Option<Matrix> {
    let csc = csc_of(a)?;
    sparse_solve_csc(&csc, b)
}

/// `a  b` for a square sparse `a` given as compressed columns (see [`sparse_solve`]).
pub fn sparse_solve_csc(csc: &crate::value::Csc, b: &Matrix) -> Option<Matrix> {
    use crate::sparse_lu::LuFactor;
    let (n, k) = (csc.rows, b.cols());
    if csc.rows != csc.cols || b.rows() != n {
        return None;
    }
    let max_nnz = (SPARSE_MAX_DENSITY * (n as f64) * (n as f64)) as usize;
    if csc.val.len() > max_nnz || csc.val.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let (ptr, idx, val) = (&csc.ptr, &csc.idx, &csc.val);
    if b.data.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let lu = LuFactor::factorize_csc(n, ptr, idx, val).ok()?;
    let anorm = (0..n).map(|j| val[ptr[j]..ptr[j + 1]].iter().map(|v| v.abs()).sum::<f64>()).fold(0.0, f64::max);
    let mut out = vec![0.0; n * k];
    let (mut s1, mut s2, mut r) = (Vec::new(), Vec::new(), vec![0.0; n]);
    for c in 0..k {
        let rhs = &b.data[c * n..(c + 1) * n];
        let bnorm = rhs.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        lu.ftran_into(rhs, &mut s1, &mut s2, None);
        let mut x = s2.clone();
        // One step of iterative refinement at most; a residual still large afterwards means the
        // factors are not trustworthy and the dense solver takes over.
        for pass in 0..2 {
            r.copy_from_slice(rhs);
            for j in 0..n {
                let xj = x[j];
                if xj != 0.0 {
                    for e in ptr[j]..ptr[j + 1] {
                        r[idx[e] as usize] -= val[e] * xj;
                    }
                }
            }
            let rnorm = r.iter().fold(0.0f64, |m, v| m.max(v.abs()));
            let xnorm = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
            if !(rnorm <= 1e-10 * (anorm * xnorm + bnorm)) {
                if pass == 1 || !rnorm.is_finite() {
                    return None;
                }
                lu.ftran_into(&r, &mut s1, &mut s2, None);
                for (xi, di) in x.iter_mut().zip(&s2) {
                    *xi += di;
                }
            } else {
                break;
            }
        }
        out[c * n..(c + 1) * n].copy_from_slice(&x);
    }
    Some(Matrix::new(n, k, out))
}

/// Threads for blocked products: one per physical core. Two hyperthreads sharing a core
/// contend for the same FMA units and caches, so using them makes timings worse and erratic.
fn gemm_par() -> faer::Par {
    static N: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let n = *N.get_or_init(|| {
        let logical = std::thread::available_parallelism().map_or(1, |n| n.get());
        if cfg!(any(target_arch = "x86", target_arch = "x86_64")) && logical >= 4 { logical / 2 } else { logical }
    });
    if n <= 1 { faer::Par::Seq } else { faer::Par::rayon(n) }
}

pub fn matmul(a: &Matrix, b: &Matrix) -> R<Matrix> {
    if !a.is_2d() || !b.is_2d() {
        return rt("Arguments must be 2-D, or at least one argument must be scalar. Use TIMES (.*) for elementwise multiplication.");
    }
    if a.cols() != b.rows() {
        return rt("Incorrect dimensions for matrix multiplication. Check that the number of columns in the first matrix matches the number of rows in the second matrix. To operate on each element of the matrix individually, use TIMES (.*) for elementwise multiplication.");
    }
    let (n, m, p) = (a.rows(), a.cols(), b.cols());
    if a.kind == Kind::Double && b.kind == Kind::Double {
        let fast = if a.sparse { sparse_times(a, b) } else if b.sparse { times_sparse(a, b) } else { None };
        if let Some(r) = fast {
            return Ok(r);
        }
    }
    if n * m * p >= FAER_MIN_WORK {
        let mut out = vec![0.0; n * p];
        let lhs = faer::MatRef::from_column_major_slice(&a.data, n, m);
        let rhs = faer::MatRef::from_column_major_slice(&b.data, m, p);
        let dst = faer::MatMut::from_column_major_slice_mut(&mut out, n, p);
        faer::linalg::matmul::matmul(dst, faer::Accum::Replace, lhs, rhs, 1.0, gemm_par());
        return Ok(Matrix::new(n, p, out));
    }
    let mut out = vec![0.0; n * p];
    // Column-major j-k-i loop order keeps the inner loop contiguous.
    for j in 0..p {
        let oc = &mut out[j * n..(j + 1) * n];
        for k in 0..m {
            let bkj = b.data[j * m + k];
            if bkj == 0.0 {
                continue;
            }
            let ac = &a.data[k * n..(k + 1) * n];
            for i in 0..n {
                oc[i] += ac[i] * bkj;
            }
        }
    }
    Ok(Matrix::new(n, p, out))
}

/// Matrix product for real or complex operands.
pub fn matmul_any(a: &Matrix, b: &Matrix) -> R<Matrix> {
    if !a.is_complex() && !b.is_complex() {
        return matmul(a, b);
    }
    let (ar, ai, br, bi) = (real_part(a), imag_part(a), real_part(b), imag_part(b));
    let re = broadcast(&matmul(&ar, &br)?, &matmul(&ai, &bi)?, |x, y| x - y)?;
    let im = broadcast(&matmul(&ar, &bi)?, &matmul(&ai, &br)?, |x, y| x + y)?;
    Ok(Matrix::complex(re.dims, re.data, im.data))
}

/// Real embedding of a complex matrix: [Re -Im; Im Re].
fn embed(m: &Matrix) -> Matrix {
    let (r, c) = (m.rows(), m.cols());
    let mut e = Matrix::zeros(2 * r, 2 * c);
    for j in 0..c {
        for i in 0..r {
            let re = m.get(i, j);
            let im = m.im_at(j * r + i);
            e.set(i, j, re);
            e.set(i, j + c, -im);
            e.set(i + r, j, im);
            e.set(i + r, j + c, re);
        }
    }
    e
}

fn stack_complex(m: &Matrix) -> Matrix {
    let (r, c) = (m.rows(), m.cols());
    let mut s = Matrix::zeros(2 * r, c);
    for j in 0..c {
        for i in 0..r {
            s.set(i, j, m.get(i, j));
            s.set(i + r, j, m.im_at(j * r + i));
        }
    }
    s
}

fn unstack_complex(s: &Matrix) -> Matrix {
    let r = s.rows() / 2;
    let c = s.cols();
    let mut re = Vec::with_capacity(r * c);
    let mut im = Vec::with_capacity(r * c);
    for j in 0..c {
        for i in 0..r {
            re.push(s.get(i, j));
            im.push(s.get(i + r, j));
        }
    }
    Matrix::complex(vec![r, c], re, im)
}

pub fn solve_any(a: &Matrix, b: &Matrix) -> R<Matrix> {
    if !a.is_complex() && !b.is_complex() {
        return solve(a, b);
    }
    Ok(unstack_complex(&solve(&embed(a), &stack_complex(b))?))
}

pub fn inv_any(a: &Matrix) -> R<Matrix> {
    if !a.is_complex() {
        return inv(a);
    }
    let n = a.rows();
    solve_any(a, &identity(n))
}

thread_local! {
    /// Warnings raised inside operators, which have no interpreter at hand; the interpreter
    /// prints them after the operation.
    static WARNINGS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Warnings raised by operators since the last call.
pub fn take_warnings() -> Vec<String> {
    WARNINGS.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

fn push_warning(msg: &str) {
    WARNINGS.with(|w| w.borrow_mut().push(msg.to_string()));
}

/// LU decomposition with partial pivoting. Returns (lu, perm, sign) or an error if singular.
fn lu(a: &Matrix) -> R<(Matrix, Vec<usize>, f64)> {
    lu_tol(a, false)
}

/// LU decomposition; with `allow_singular`, a zero pivot column is left as it is (as LAPACK
/// does), so the substitution afterwards gives Inf or NaN.
fn lu_tol(a: &Matrix, allow_singular: bool) -> R<(Matrix, Vec<usize>, f64)> {
    let n = a.rows();
    let mut lu = Matrix::from_dims(a.dims.clone(), a.data.clone());
    let mut perm: Vec<usize> = (0..n).collect();
    let mut sign = 1.0;
    for k in 0..n {
        let mut p = k;
        let mut best = lu.get(k, k).abs();
        for i in k + 1..n {
            let v = lu.get(i, k).abs();
            if v > best {
                best = v;
                p = i;
            }
        }
        if best < 1e-300 {
            if !allow_singular {
                return rt("Matrix is singular to working precision.");
            }
            if best == 0.0 {
                continue;
            }
        }
        if p != k {
            for c in 0..n {
                let t = lu.get(k, c);
                lu.set(k, c, lu.get(p, c));
                lu.set(p, c, t);
            }
            perm.swap(k, p);
            sign = -sign;
        }
        let pivot = lu.get(k, k);
        for i in k + 1..n {
            let f = lu.get(i, k) / pivot;
            lu.set(i, k, f);
            if f != 0.0 {
                for c in k + 1..n {
                    let v = lu.get(i, c) - f * lu.get(k, c);
                    lu.set(i, c, v);
                }
            }
        }
    }
    Ok((lu, perm, sign))
}

fn lu_solve(lu: &Matrix, perm: &[usize], b: &Matrix) -> Matrix {
    let n = lu.rows();
    let mut x = Matrix::zeros(n, b.cols());
    for col in 0..b.cols() {
        let mut y: Vec<f64> = perm.iter().map(|&p| b.get(p, col)).collect();
        for i in 0..n {
            for k in 0..i {
                y[i] -= lu.get(i, k) * y[k];
            }
        }
        for i in (0..n).rev() {
            for k in i + 1..n {
                y[i] -= lu.get(i, k) * y[k];
            }
            y[i] /= lu.get(i, i);
        }
        for i in 0..n {
            x.set(i, col, y[i]);
        }
    }
    x
}

/// `a \ b`: exact solve for square systems, least squares otherwise.
pub fn solve(a: &Matrix, b: &Matrix) -> R<Matrix> {
    if !a.is_2d() || !b.is_2d() {
        return rt("Inputs must be 2-D.");
    }
    if a.rows() != b.rows() {
        return rt("Matrix dimensions must agree.");
    }
    if a.rows() == a.cols() {
        if a.sparse && a.rows() >= SPARSE_MIN_ORDER {
            if let Some(x) = sparse_solve(a, b) {
                return Ok(x);
            }
        }
        if a.rows() >= FAER_MIN_ORDER {
            if let Ok(x) = faer_solve(a, b) {
                return Ok(x);
            }
        } else if let Ok((lu, perm, _)) = lu(a) {
            return Ok(lu_solve(&lu, &perm, b));
        }
        // A singular matrix gives a warning and the Inf or NaN of the substitution.
        push_warning("Matrix is singular to working precision.");
        let (lu, perm, _) = lu_tol(a, true)?;
        Ok(lu_solve(&lu, &perm, b))
    } else {
        let at = a.transpose();
        let ata = matmul(&at, a)?;
        let atb = matmul(&at, b)?;
        let (lu, perm, _) = lu(&ata)?;
        Ok(lu_solve(&lu, &perm, &atb))
    }
}

pub fn inv(a: &Matrix) -> R<Matrix> {
    if !a.is_2d() || a.rows() != a.cols() {
        return rt("Matrix must be square.");
    }
    if a.rows() >= FAER_MIN_ORDER {
        return faer_solve(a, &identity(a.rows()));
    }
    let (lu, perm, _) = lu(a)?;
    Ok(lu_solve(&lu, &perm, &identity(a.rows())))
}

pub fn det(a: &Matrix) -> R<f64> {
    if !a.is_2d() || a.rows() != a.cols() {
        return rt("Matrix must be square.");
    }
    if a.rows() == 0 {
        return Ok(1.0);
    }
    if a.rows() >= FAER_MIN_ORDER {
        return Ok(faer::MatRef::from_column_major_slice(&a.data, a.rows(), a.rows()).determinant());
    }
    match lu(a) {
        Ok((lu, _, sign)) => Ok((0..a.rows()).map(|i| lu.get(i, i)).product::<f64>() * sign),
        Err(_) => Ok(0.0),
    }
}
