//! Native operations on sparse matrices held as compressed columns ([`Value::Sparse`]):
//! construction from triplets, transpose, sums, products, solves and the common queries.
//! Anything not handled here is done on the expanded array by the ordinary code, and the result
//! is compressed again (see `Value::densify` and `builtins_sparse::compress`).

use crate::ast::{BinOp, UnOp};
use crate::builtins::Ret;
use crate::builtins_sparse::compress;
use crate::indexing::sparse_value;
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;
use std::sync::Arc;

type R<T> = Result<T, LangError>;

/// How repeated positions combine when building from triplets.
#[derive(Clone, Copy, PartialEq)]
pub enum Dup {
    Sum,
    Last,
    Or,
}

/// Compressed columns from `(row, col, value)` triplets (zero-based). Repeated positions combine
/// by `dup`; entries that end up zero are dropped.
pub fn from_triplets(rows: usize, cols: usize, mut t: Vec<(u32, u32, f64)>, dup: Dup, logical: bool) -> Csc {
    // Stable sort by (col, row) keeps the input order among repeated positions.
    t.sort_by_key(|&(r, c, _)| (c, r));
    let (mut ptr, mut idx, mut val) = (vec![0usize; cols + 1], Vec::with_capacity(t.len()), Vec::with_capacity(t.len()));
    let mut k = 0;
    while k < t.len() {
        let (r, c) = (t[k].0, t[k].1);
        let mut v = t[k].2;
        k += 1;
        while k < t.len() && t[k].0 == r && t[k].1 == c {
            v = match dup {
                Dup::Sum => v + t[k].2,
                Dup::Last => t[k].2,
                Dup::Or => if v != 0.0 || t[k].2 != 0.0 { 1.0 } else { 0.0 },
            };
            k += 1;
        }
        if v != 0.0 {
            idx.push(r);
            val.push(if logical { 1.0 } else { v });
            ptr[c as usize + 1] += 1;
        }
    }
    for j in 0..cols {
        ptr[j + 1] += ptr[j];
    }
    Csc { rows, cols, ptr, idx, val, logical }
}

pub fn transpose(c: &Csc) -> Csc {
    let mut ptr = vec![0usize; c.rows + 1];
    for &r in &c.idx {
        ptr[r as usize + 1] += 1;
    }
    for i in 0..c.rows {
        ptr[i + 1] += ptr[i];
    }
    let mut next = ptr.clone();
    let (mut idx, mut val) = (vec![0u32; c.val.len()], vec![0.0; c.val.len()]);
    for j in 0..c.cols {
        for e in c.ptr[j]..c.ptr[j + 1] {
            let r = c.idx[e] as usize;
            idx[next[r]] = j as u32;
            val[next[r]] = c.val[e];
            next[r] += 1;
        }
    }
    Csc { rows: c.cols, cols: c.rows, ptr, idx, val, logical: c.logical }
}

fn scaled(c: &Csc, s: f64) -> Csc {
    let mut out = c.clone();
    out.val.iter_mut().for_each(|v| *v *= s);
    out.logical = false;
    out
}

/// `a + sign * b` (same size), merging the columns.
fn add(a: &Csc, b: &Csc, sign: f64) -> Csc {
    let (mut ptr, mut idx, mut val) = (vec![0usize; a.cols + 1], Vec::new(), Vec::new());
    for j in 0..a.cols {
        let (mut p, mut q) = (a.ptr[j], b.ptr[j]);
        let (pe, qe) = (a.ptr[j + 1], b.ptr[j + 1]);
        while p < pe || q < qe {
            let (ra, rb) = (if p < pe { a.idx[p] } else { u32::MAX }, if q < qe { b.idx[q] } else { u32::MAX });
            let (r, v) = if ra < rb {
                p += 1;
                (ra, a.val[p - 1])
            } else if rb < ra {
                q += 1;
                (rb, sign * b.val[q - 1])
            } else {
                p += 1;
                q += 1;
                (ra, a.val[p - 1] + sign * b.val[q - 1])
            };
            if v != 0.0 {
                idx.push(r);
                val.push(v);
            }
        }
        ptr[j + 1] = idx.len();
    }
    Csc { rows: a.rows, cols: a.cols, ptr, idx, val, logical: false }
}

/// `a .* b` (same size).
fn hadamard(a: &Csc, b: &Csc) -> Csc {
    let (mut ptr, mut idx, mut val) = (vec![0usize; a.cols + 1], Vec::new(), Vec::new());
    for j in 0..a.cols {
        let (mut p, mut q) = (a.ptr[j], b.ptr[j]);
        let (pe, qe) = (a.ptr[j + 1], b.ptr[j + 1]);
        while p < pe && q < qe {
            if a.idx[p] < b.idx[q] {
                p += 1;
            } else if b.idx[q] < a.idx[p] {
                q += 1;
            } else {
                let v = a.val[p] * b.val[q];
                if v != 0.0 {
                    idx.push(a.idx[p]);
                    val.push(v);
                }
                p += 1;
                q += 1;
            }
        }
        ptr[j + 1] = idx.len();
    }
    Csc { rows: a.rows, cols: a.cols, ptr, idx, val, logical: false }
}

/// `a * b` for two sparse matrices (Gustavson's column algorithm).
fn spgemm(a: &Csc, b: &Csc) -> Csc {
    let mut ptr = vec![0usize; b.cols + 1];
    let (mut idx, mut val) = (Vec::new(), Vec::new());
    let mut acc = vec![0.0; a.rows];
    let mut mark = vec![usize::MAX; a.rows];
    let mut rows: Vec<u32> = Vec::new();
    for j in 0..b.cols {
        rows.clear();
        for e in b.ptr[j]..b.ptr[j + 1] {
            let (k, bv) = (b.idx[e] as usize, b.val[e]);
            for f in a.ptr[k]..a.ptr[k + 1] {
                let r = a.idx[f] as usize;
                if mark[r] != j {
                    mark[r] = j;
                    acc[r] = 0.0;
                    rows.push(r as u32);
                }
                acc[r] += a.val[f] * bv;
            }
        }
        rows.sort_unstable();
        for &r in &rows {
            let v = acc[r as usize];
            if v != 0.0 {
                idx.push(r);
                val.push(v);
            }
        }
        ptr[j + 1] = idx.len();
    }
    Csc { rows: a.rows, cols: b.cols, ptr, idx, val, logical: false }
}

/// `a * x` for a dense `x` (a few columns or many: one pass per column).
fn times_dense(a: &Csc, x: &Matrix) -> Matrix {
    let k = x.cols();
    let mut out = vec![0.0; a.rows * k];
    for col in 0..k {
        let xs = &x.data[col * a.cols..(col + 1) * a.cols];
        let y = &mut out[col * a.rows..(col + 1) * a.rows];
        for j in 0..a.cols {
            let xj = xs[j];
            if xj != 0.0 {
                for e in a.ptr[j]..a.ptr[j + 1] {
                    y[a.idx[e] as usize] += a.val[e] * xj;
                }
            }
        }
    }
    Matrix::new(a.rows, k, out)
}

/// `x * b` for a dense `x`.
fn dense_times(x: &Matrix, b: &Csc) -> Matrix {
    let r = x.rows();
    let mut out = vec![0.0; r * b.cols];
    for j in 0..b.cols {
        for e in b.ptr[j]..b.ptr[j + 1] {
            let (k, v) = (b.idx[e] as usize, b.val[e]);
            for row in 0..r {
                out[j * r + row] += x.data[k * r + row] * v;
            }
        }
    }
    Matrix::new(r, b.cols, out)
}

fn real_scalar(v: &Value) -> Option<f64> {
    match v {
        Value::Num(m) if m.numel() == 1 && !m.is_complex() && matches!(m.kind, Kind::Double | Kind::Logical) && m.data[0].is_finite() => Some(m.data[0]),
        _ => None,
    }
}

fn plain_matrix(v: &Value) -> Option<&Matrix> {
    match v {
        Value::Num(m) if !m.is_complex() && m.dims.len() == 2 && matches!(m.kind, Kind::Double | Kind::Logical) => Some(m),
        _ => None,
    }
}

fn wrap(c: Csc) -> R<Value> {
    sparse_value(c.rows, c.cols, c.ptr, c.idx, c.val, c.logical)
}

/// `a op b` where an operand is a compressed sparse matrix.
pub fn binary(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    if let Some(r) = native_binary(op, a, b)? {
        return Ok(r);
    }
    let (da, db) = (a.clone().densify()?, b.clone().densify()?);
    Ok(compress(crate::ops::binary(op, &da, &db)?))
}

fn native_binary(op: BinOp, a: &Value, b: &Value) -> R<Option<Value>> {
    use BinOp::*;
    match (op, a, b) {
        (MatMul | Mul, Value::Sparse(x), s) if real_scalar(s).is_some() => wrap(scaled(x, real_scalar(s).unwrap())).map(Some),
        (MatMul | Mul, s, Value::Sparse(y)) if real_scalar(s).is_some() => wrap(scaled(y, real_scalar(s).unwrap())).map(Some),
        (Div | MatDiv, Value::Sparse(x), s) if real_scalar(s).is_some_and(|d| d != 0.0) => wrap(scaled(x, 1.0 / real_scalar(s).unwrap())).map(Some),
        (MatMul, Value::Sparse(x), Value::Sparse(y)) if x.cols == y.rows => wrap(spgemm(x, y)).map(Some),
        (MatMul, Value::Sparse(x), d) if plain_matrix(d).is_some_and(|m| m.rows() == x.cols) && !x.logical => {
            let m = plain_matrix(d).unwrap();
            if m.numel() == 1 {
                return Ok(None);
            }
            Ok(Some(Value::Num(times_dense(x, m))))
        }
        (MatMul, d, Value::Sparse(y)) if plain_matrix(d).is_some_and(|m| m.cols() == y.rows && m.numel() > 1) && !y.logical => Ok(Some(Value::Num(dense_times(plain_matrix(d).unwrap(), y)))),
        (Add | Sub, Value::Sparse(x), Value::Sparse(y)) if x.rows == y.rows && x.cols == y.cols => wrap(add(x, y, if op == Add { 1.0 } else { -1.0 })).map(Some),
        (Mul, Value::Sparse(x), Value::Sparse(y)) if x.rows == y.rows && x.cols == y.cols => wrap(hadamard(x, y)).map(Some),
        (MatLeftDiv, Value::Sparse(x), d) if x.rows == x.cols && !x.logical => {
            let Some(m) = plain_matrix(d) else { return Ok(None) };
            Ok(crate::ops::sparse_solve_csc(x, m).map(Value::Num))
        }
        _ => Ok(None),
    }
}

/// `op v` for a compressed sparse matrix.
pub fn unary(op: UnOp, v: &Value) -> R<Value> {
    let Value::Sparse(c) = v else { unreachable!() };
    match op {
        UnOp::Transpose | UnOp::CTranspose => wrap(transpose(c)),
        UnOp::Plus => Ok(v.clone()),
        UnOp::Neg => wrap(scaled(c, -1.0)),
        UnOp::Not => Ok(compress(crate::ops::unary(op, &v.clone().densify()?)?)),
    }
}

fn dim_arg(v: &Value) -> R<usize> {
    v.as_usize()
}

/// Builtins that run on compressed columns. `None` when `name` needs the expanded array.
pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    let Some(Value::Sparse(c)) = a.first() else { return Ok(None) };
    let c = c.clone();
    let double = |m: Matrix| Value::Num(m);
    Ok(match name {
        "nnz" => Some(vec![Value::num(c.nnz() as f64)]),
        "nzmax" => Some(vec![Value::num(c.nnz().max(1) as f64)]),
        "issparse" => Some(vec![Value::bool(true)]),
        "isreal" | "isnumeric" if !c.logical || name == "isreal" => Some(vec![Value::bool(true)]),
        "islogical" => Some(vec![Value::bool(c.logical)]),
        "isempty" => Some(vec![Value::bool(c.rows == 0 || c.cols == 0)]),
        "numel" if a.len() == 1 => Some(vec![Value::num(c.rows as f64 * c.cols as f64)]),
        "length" => Some(vec![Value::num(if c.rows == 0 || c.cols == 0 { 0.0 } else { c.rows.max(c.cols) as f64 })]),
        "ndims" => Some(vec![Value::num(2.0)]),
        "class" => Some(vec![Value::chars(if c.logical { "logical" } else { "double" })]),
        "size" => match a.len() {
            1 if nargout <= 1 => Some(vec![double(Matrix::row(vec![c.rows as f64, c.cols as f64]))]),
            1 => {
                let mut out = vec![Value::num(c.rows as f64), Value::num(c.cols as f64)];
                out.truncate(nargout);
                while out.len() < nargout {
                    out.push(Value::num(1.0));
                }
                Some(out)
            }
            2 => Some(vec![Value::num(match dim_arg(&a[1])? {
                1 => c.rows as f64,
                2 => c.cols as f64,
                _ => 1.0,
            })]),
            _ => None,
        },
        "full" => Some(vec![Value::Num({
            let mut m = c.to_dense()?;
            m.sparse = false;
            m
        })]),
        "double" if a.len() == 1 => {
            let mut o = (*c).clone();
            o.logical = false;
            Some(vec![wrap(o)?])
        }
        "nonzeros" => Some(vec![double(Matrix::col(c.val.clone()))]),
        "transpose" | "ctranspose" if a.len() == 1 => Some(vec![wrap(transpose(&c))?]),
        "uminus" if a.len() == 1 => Some(vec![wrap(scaled(&c, -1.0))?]),
        "abs" if a.len() == 1 => {
            let mut o = (*c).clone();
            o.val.iter_mut().for_each(|v| *v = v.abs());
            Some(vec![wrap(o)?])
        }
        "spones" if a.len() == 1 => {
            let mut o = (*c).clone();
            o.val.iter_mut().for_each(|v| *v = 1.0);
            o.logical = false;
            Some(vec![wrap(o)?])
        }
        "trace" if a.len() == 1 && c.rows == c.cols => {
            let mut s = 0.0;
            for j in 0..c.cols {
                if let Ok(p) = c.idx[c.ptr[j]..c.ptr[j + 1]].binary_search(&(j as u32)) {
                    s += c.val[c.ptr[j] + p];
                }
            }
            Some(vec![Value::num(s)])
        }
        "diag" if a.len() == 1 && c.rows != 1 && c.cols != 1 => {
            let n = c.rows.min(c.cols);
            let mut d = vec![0.0; n];
            for j in 0..n {
                if let Ok(p) = c.idx[c.ptr[j]..c.ptr[j + 1]].binary_search(&(j as u32)) {
                    d[j] = c.val[c.ptr[j] + p];
                }
            }
            let t: Vec<(u32, u32, f64)> = d.iter().enumerate().filter(|(_, v)| **v != 0.0).map(|(i, &v)| (i as u32, 0, v)).collect();
            Some(vec![wrap(from_triplets(n, 1, t, Dup::Last, false))?])
        }
        "sum" if a.len() == 1 || (a.len() == 2 && matches!(&a[1], Value::Num(m) if m.numel() == 1)) => {
            let dim = if a.len() == 2 { dim_arg(&a[1])? } else if c.rows == 1 { 2 } else { 1 };
            if dim == 1 {
                let t: Vec<(u32, u32, f64)> = (0..c.cols).filter_map(|j| {
                    let s: f64 = c.val[c.ptr[j]..c.ptr[j + 1]].iter().sum();
                    (s != 0.0).then_some((0u32, j as u32, s))
                }).collect();
                Some(vec![wrap(from_triplets(1, c.cols, t, Dup::Last, false))?])
            } else if dim == 2 {
                let mut s = vec![0.0; c.rows];
                for e in 0..c.val.len() {
                    s[c.idx[e] as usize] += c.val[e];
                }
                let t: Vec<(u32, u32, f64)> = s.iter().enumerate().filter(|(_, v)| **v != 0.0).map(|(i, &v)| (i as u32, 0, v)).collect();
                Some(vec![wrap(from_triplets(c.rows, 1, t, Dup::Last, false))?])
            } else {
                None
            }
        }
        "find" if a.len() == 1 && nargout <= 3 => {
            let (mut ri, mut ci, mut vv) = (Vec::with_capacity(c.nnz()), Vec::with_capacity(c.nnz()), Vec::with_capacity(c.nnz()));
            for j in 0..c.cols {
                for e in c.ptr[j]..c.ptr[j + 1] {
                    ri.push(c.idx[e] as f64 + 1.0);
                    ci.push(j as f64 + 1.0);
                    vv.push(c.val[e]);
                }
            }
            let lin: Vec<f64> = ri.iter().zip(&ci).map(|(r, cc)| (cc - 1.0) * c.rows as f64 + r).collect();
            Some(match nargout {
                0 | 1 => vec![double(Matrix::col(lin))],
                2 => vec![double(Matrix::col(ri)), double(Matrix::col(ci))],
                _ => vec![double(Matrix::col(ri)), double(Matrix::col(ci)), double(Matrix::col(vv))],
            })
        }
        "norm" if a.len() == 2 => {
            let kind = match &a[1] {
                Value::Num(m) if m.numel() == 1 && m.kind != Kind::Char => Some(m.data[0]),
                v if v.is_text() => match v.text()?.as_str() {
                    "fro" => Some(-1.0),
                    "inf" | "Inf" => Some(f64::INFINITY),
                    _ => None,
                },
                _ => None,
            };
            match kind {
                Some(1.0) => Some(vec![Value::num((0..c.cols).map(|j| c.val[c.ptr[j]..c.ptr[j + 1]].iter().map(|v| v.abs()).sum::<f64>()).fold(0.0, f64::max))]),
                Some(k) if k == f64::INFINITY => {
                    let mut s = vec![0.0; c.rows];
                    for e in 0..c.val.len() {
                        s[c.idx[e] as usize] += c.val[e].abs();
                    }
                    Some(vec![Value::num(s.into_iter().fold(0.0, f64::max))])
                }
                Some(k) if k == -1.0 => Some(vec![Value::num(c.val.iter().map(|v| v * v).sum::<f64>().sqrt())]),
                _ => None,
            }
        }
        _ => None,
    })
}

/// Builders that produce compressed columns directly when the result is large. `None` leaves
/// the call to the dense builders.
pub fn build(name: &str, a: &[Value]) -> Ret {
    match name {
        "sparse" => build_sparse(a),
        "speye" => build_speye(a),
        "spdiags" => build_spdiags(a),
        "kron" => build_kron(a),
        _ => Ok(None),
    }
}

fn to_idx(x: f64) -> R<u32> {
    if x < 1.0 || x.fract() != 0.0 {
        return rt("Index into matrix must be positive.");
    }
    if x > u32::MAX as f64 {
        return rt("Index exceeds matrix dimensions.");
    }
    Ok(x as u32 - 1)
}

fn build_sparse(a: &[Value]) -> Ret {
    if a.len() == 2 {
        let (r, c) = (a[0].as_usize()?, a[1].as_usize()?);
        if r.saturating_mul(c) < crate::builtins_sparse::COMPRESS_MIN {
            return Ok(None);
        }
        return Ok(Some(vec![sparse_value(r, c, vec![0; c + 1], Vec::new(), Vec::new(), false)?]));
    }
    if a.len() == 1 {
        // sparse(S) of a compressed matrix is itself; sparse(A) of a large dense array compresses.
        return Ok(match &a[0] {
            Value::Sparse(_) => Some(vec![a[0].clone()]),
            Value::Num(m) if m.numel() >= crate::builtins_sparse::COMPRESS_MIN && !m.is_complex() && matches!(m.kind, Kind::Double | Kind::Logical) => {
                let mut f = m.clone();
                f.sparse = true;
                let v = compress(Value::Num(f));
                if v.is_compressed() { Some(vec![v]) } else { None }
            }
            _ => None,
        });
    }
    if a.len() < 5 {
        return Ok(None);
    }
    let (rows, cols) = (a[3].as_usize()?, a[4].as_usize()?);
    if rows.saturating_mul(cols) < crate::builtins_sparse::COMPRESS_MIN {
        return Ok(None);
    }
    let (Value::Num(i), Value::Num(j), Value::Num(v)) = (&a[0], &a[1], &a[2]) else { return Ok(None) };
    if v.is_complex() || !matches!(v.kind, Kind::Double | Kind::Logical) {
        return Ok(None);
    }
    let n = i.numel().max(j.numel()).max(v.numel());
    for m in [i, j, v] {
        if m.numel() != 1 && m.numel() != n {
            return rt("Vectors must be the same length.");
        }
    }
    let pick = |m: &Matrix, k: usize| m.data[if m.numel() == 1 { 0 } else { k }];
    let mut t = Vec::with_capacity(n);
    for k in 0..n {
        let (r, c) = (to_idx(pick(i, k))?, to_idx(pick(j, k))?);
        if r as usize >= rows || c as usize >= cols {
            return rt("Index exceeds matrix dimensions.");
        }
        t.push((r, c, pick(v, k)));
    }
    let logical = v.kind == Kind::Logical;
    let c = from_triplets(rows, cols, t, if logical { Dup::Or } else { Dup::Sum }, logical);
    Ok(Some(vec![wrap(c)?]))
}

fn build_speye(a: &[Value]) -> Ret {
    let (r, c) = match a {
        [n] => match n {
            Value::Num(m) if m.numel() == 2 => (m.data[0] as usize, m.data[1] as usize),
            _ => (n.as_usize()?, n.as_usize()?),
        },
        [r, c, ..] => (r.as_usize()?, c.as_usize()?),
        _ => return Ok(None),
    };
    if r.saturating_mul(c) < crate::builtins_sparse::COMPRESS_MIN {
        return Ok(None);
    }
    let d = r.min(c);
    let mut ptr = Vec::with_capacity(c + 1);
    for j in 0..=c {
        ptr.push(j.min(d));
    }
    Ok(Some(vec![sparse_value(r, c, ptr, (0..d as u32).collect(), vec![1.0; d], false)?]))
}

fn build_spdiags(a: &[Value]) -> Ret {
    let [b, d, m, n] = a else { return Ok(None) };
    let (m, n) = (m.as_usize()?, n.as_usize()?);
    if m.saturating_mul(n) < crate::builtins_sparse::COMPRESS_MIN {
        return Ok(None);
    }
    let (Value::Num(b), Value::Num(d)) = (b, d) else { return Ok(None) };
    if b.is_complex() {
        return Ok(None);
    }
    let mut t = Vec::new();
    for (k, &dk) in d.data.iter().enumerate() {
        let dk = dk as i64;
        for j in 0..n {
            let i = j as i64 - dk;
            if i < 0 || i as usize >= m || b.cols() <= k {
                continue;
            }
            let idx = if m >= n { j } else { i as usize };
            let v = if idx < b.rows() { b.data[k * b.rows() + idx] } else { 0.0 };
            // Later diagonals overwrite earlier ones, zeros included.
            t.push((i as u32, j as u32, v));
        }
    }
    let c = from_triplets_keep_last(m, n, t);
    Ok(Some(vec![wrap(c)?]))
}

/// As `from_triplets` with `Dup::Last` (a repeated position keeps its last value, even zero).
fn from_triplets_keep_last(rows: usize, cols: usize, t: Vec<(u32, u32, f64)>) -> Csc {
    from_triplets(rows, cols, t, Dup::Last, false)
}

fn build_kron(a: &[Value]) -> Ret {
    let [x, y] = a else { return Ok(None) };
    if !(x.is_compressed() || y.is_compressed()) {
        return Ok(None);
    }
    let sp = |v: &Value| -> Option<Csc> {
        match v {
            Value::Sparse(c) => Some((**c).clone()),
            Value::Num(m) if !m.is_complex() && m.dims.len() == 2 && matches!(m.kind, Kind::Double | Kind::Logical) => {
                let (r, c) = (m.rows(), m.cols());
                let t: Vec<(u32, u32, f64)> = (0..m.numel()).filter(|&k| m.data[k] != 0.0).map(|k| ((k % r) as u32, (k / r) as u32, m.data[k])).collect();
                Some(from_triplets(r, c, t, Dup::Last, false))
            }
            _ => None,
        }
    };
    let (Some(p), Some(q)) = (sp(x), sp(y)) else { return Ok(None) };
    let (rows, cols) = (p.rows * q.rows, p.cols * q.cols);
    let (mut ptr, mut idx, mut val) = (Vec::with_capacity(cols + 1), Vec::with_capacity(p.nnz() * q.nnz()), Vec::with_capacity(p.nnz() * q.nnz()));
    ptr.push(0);
    for jp in 0..p.cols {
        for jq in 0..q.cols {
            for e in p.ptr[jp]..p.ptr[jp + 1] {
                for f in q.ptr[jq]..q.ptr[jq + 1] {
                    idx.push(p.idx[e] * q.rows as u32 + q.idx[f]);
                    val.push(p.val[e] * q.val[f]);
                }
            }
            ptr.push(idx.len());
        }
    }
    Ok(Some(vec![sparse_value(rows, cols, ptr, idx, val, false)?]))
}

/// Compressed columns of any real sparse-capable argument (for the iterative solvers).
pub fn as_csc(v: &Value) -> Option<Arc<Csc>> {
    match v {
        Value::Sparse(c) => Some(c.clone()),
        _ => None,
    }
}

