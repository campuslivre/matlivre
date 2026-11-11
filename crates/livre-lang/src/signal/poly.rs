//! Complex polynomial helpers shared by the signal modules.

use crate::builtins::{m_arg, Ret};
use crate::cplx::C;
use crate::error::{rt, LangError};
use crate::nd::Dims;
use crate::value::*;

pub(crate) type R<T> = Result<T, LangError>;

/// The elements of a numeric argument as complex numbers.
pub(crate) fn cvec_arg(a: &[Value], i: usize, name: &str) -> R<Vec<C>> {
    let m = m_arg(a, i, name)?;
    Ok((0..m.numel()).map(|k| C::new(m.data[k], m.im_at(k))).collect())
}

/// A real vector argument.
pub(crate) fn rvec_arg(a: &[Value], i: usize, name: &str) -> R<Vec<f64>> {
    let m = m_arg(a, i, name)?;
    if m.im.as_ref().is_some_and(|im| im.iter().any(|&v| v != 0.0)) {
        return rt(format!("Expected input number {} to be real.", i + 1));
    }
    Ok(m.data.clone())
}

/// A real scalar argument.
pub(crate) fn scalar_arg(a: &[Value], i: usize, name: &str) -> R<f64> {
    let m = m_arg(a, i, name)?;
    if m.numel() != 1 || m.im.as_ref().is_some_and(|im| im[0] != 0.0) || m.data[0].is_nan() {
        return rt(format!("Expected input number {} to be a real scalar.", i + 1));
    }
    Ok(m.data[0])
}

/// Text of argument `i`, lower-cased, if it is a nonempty string (so `[]` is not a flag).
pub(crate) fn text_arg(a: &[Value], i: usize) -> Option<String> {
    a.get(i).and_then(|v| v.text().ok()).filter(|s| !s.is_empty()).map(|s| s.to_lowercase())
}

/// Polynomial with the given roots, leading coefficient 1.
pub(crate) fn poly_of(roots: &[C]) -> Vec<C> {
    let mut p = vec![C::new(1.0, 0.0)];
    for &r in roots {
        let mut q = vec![C::new(0.0, 0.0); p.len() + 1];
        for (i, &c) in p.iter().enumerate() {
            q[i] = q[i].add(c);
            q[i + 1] = q[i + 1].sub(c.mul(r));
        }
        p = q;
    }
    p
}

/// Horner evaluation, highest power first.
pub(crate) fn polyval_c(coef: &[C], x: C) -> C {
    coef.iter().fold(C::new(0.0, 0.0), |acc, &c| acc.mul(x).add(c))
}

/// True when the roots come in complex-conjugate pairs (so the polynomial is real).
pub(crate) fn conj_closed(roots: &[C]) -> bool {
    let mut used = vec![false; roots.len()];
    for (i, r) in roots.iter().enumerate() {
        if used[i] {
            continue;
        }
        if r.im == 0.0 {
            used[i] = true;
            continue;
        }
        let tol = 1e-8 * (1.0 + r.abs());
        let want = r.conj();
        let found = (0..roots.len()).find(|&j| j != i && !used[j] && roots[j].sub(want).abs() <= tol);
        match found {
            Some(j) => {
                used[i] = true;
                used[j] = true;
            }
            None => return false,
        }
    }
    true
}

/// Real polynomial coefficients when the roots are conjugate-closed, otherwise complex.
pub(crate) fn poly_value(roots: &[C], scale: C) -> (Vec<C>, bool) {
    let mut p = poly_of(roots);
    for c in p.iter_mut() {
        *c = c.mul(scale);
    }
    let real = conj_closed(roots) && scale.im == 0.0;
    (p, real)
}

fn all_real(v: &[C]) -> bool {
    v.iter().all(|c| c.im == 0.0)
}

/// A column vector value, real when every imaginary part is zero.
pub(crate) fn col_value(v: &[C]) -> Value {
    shaped(vec![v.len(), 1], v)
}

fn shaped(dims: Dims, v: &[C]) -> Value {
    let re: Vec<f64> = v.iter().map(|c| c.re).collect();
    if all_real(v) {
        Value::Num(Matrix::from_dims(dims, re))
    } else {
        Value::Num(Matrix::complex(dims, re, v.iter().map(|c| c.im).collect()))
    }
}

pub(crate) fn real_row(v: Vec<f64>) -> Value {
    Value::Num(Matrix::row(v))
}

pub(crate) fn real_col(v: Vec<f64>) -> Value {
    Value::Num(Matrix::col(v))
}

pub(crate) fn scalar_value(v: f64) -> Value {
    Value::Num(Matrix::scalar(v))
}

pub(crate) fn out(values: Vec<Value>) -> Ret {
    Ok(Some(values))
}

// ---- real-matrix helpers shared by the filtering and transform modules -------------------

/// The real matrix argument `i`.
pub(crate) fn real_matrix(a: &[Value], i: usize, name: &str) -> R<Matrix> {
    real_matrix_named(a, i, name, "X")
}

/// Like `real_matrix`, with the argument's documented name in the error message.
pub(crate) fn real_matrix_named(a: &[Value], i: usize, name: &str, label: &str) -> R<Matrix> {
    let m = m_arg(a, i, name)?;
    if m.im.as_ref().is_some_and(|im| im.iter().any(|&v| v != 0.0)) {
        return rt(format!("Expected input number {}, {label}, to be real.", i + 1));
    }
    if m.dims.len() > 2 {
        return rt("N-D arrays are not supported.");
    }
    Ok(m)
}

/// First dimension (1 or 2) of size other than 1.
pub(crate) fn first_dim(m: &Matrix) -> usize {
    if m.dims[0] == 1 { 2 } else { 1 }
}

/// Apply `f` to every vector along `dim` of a 2-D matrix; the results share one length.
pub(crate) fn along(m: &Matrix, dim: usize, f: &dyn Fn(&[f64]) -> R<Vec<f64>>) -> R<Matrix> {
    let (rows, cols) = (m.dims[0], m.dims[1]);
    if m.numel() == 0 {
        return Ok(m.clone());
    }
    if dim >= 3 {
        // Along a singleton dimension every element is its own vector.
        let results: Vec<Vec<f64>> = m.data.iter().map(|&v| f(&[v])).collect::<R<_>>()?;
        if results.iter().any(|r| r.len() != 1) {
            return rt("Operating along this dimension is not supported.");
        }
        return Ok(Matrix::new(rows, cols, results.into_iter().flatten().collect()));
    }
    if dim == 1 {
        let results: Vec<Vec<f64>> = (0..cols).map(|c| f(&m.data[c * rows..(c + 1) * rows])).collect::<R<_>>()?;
        let len = results[0].len();
        Ok(Matrix::new(len, cols, results.into_iter().flatten().collect()))
    } else {
        let results: Vec<Vec<f64>> = (0..rows).map(|r| f(&(0..cols).map(|c| m.data[c * rows + r]).collect::<Vec<_>>())).collect::<R<_>>()?;
        let len = results[0].len();
        let mut data = vec![0.0; rows * len];
        for (r, res) in results.iter().enumerate() {
            for (c, &v) in res.iter().enumerate() {
                data[c * rows + r] = v;
            }
        }
        Ok(Matrix::new(rows, len, data))
    }
}

