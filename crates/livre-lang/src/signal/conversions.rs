//! Polynomial and filter-form utilities: `polystab`, `polyscale`, `eqtflength`, `sos2tf`,
//! `isstable`.

use crate::builtins::{m_arg, Ret};
use crate::cplx::{poly_roots, C};
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["polystab", "polyscale", "eqtflength", "sos2tf", "isstable"];

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    match name {
        "polystab" => polystab(a),
        "polyscale" => polyscale(a),
        "eqtflength" => eqtflength(a, nargout),
        "sos2tf" => sos2tf(a),
        _ => isstable(a),
    }
}

fn row_of(v: &[C]) -> Value {
    if v.iter().all(|c| c.im == 0.0) {
        real_row(v.iter().map(|c| c.re).collect())
    } else {
        Value::Num(Matrix::complex(vec![1, v.len()], v.iter().map(|c| c.re).collect(), v.iter().map(|c| c.im).collect()))
    }
}

fn vector_arg(a: &[Value], i: usize, name: &str, message: &str) -> R<Vec<C>> {
    let m = m_arg(a, i, name)?;
    if m.dims.len() > 2 || (m.dims[0] != 1 && m.dims[1] != 1 && m.numel() > 0) {
        return rt(message);
    }
    cvec_arg(a, i, name)
}

/// Reflect the roots outside the unit circle to their conjugate-reciprocal positions. The result
/// is a row vector, with leading zeros of the input removed.
fn polystab(a: &[Value]) -> Ret {
    if a.len() > 1 {
        return rt("Too many input arguments.");
    }
    let mut v = vector_arg(a, 0, "polystab", "Input must be a vector.")?;
    while v.len() > 1 && v[0].abs() == 0.0 {
        v.remove(0);
    }
    if v.is_empty() {
        return out(vec![Value::Num(Matrix::zeros(0, 0))]);
    }
    if v.len() <= 1 || v[0].abs() == 0.0 {
        return out(vec![row_of(&v)]);
    }
    let roots: Vec<C> = poly_roots(&v)
        .into_iter()
        .map(|r| if r.abs() > 1.0 { C::new(1.0, 0.0).div(r.conj()) } else { r })
        .collect();
    let (mut p, real) = poly_value(&roots, v[0]);
    if real {
        for c in p.iter_mut() {
            c.im = 0.0;
        }
    }
    out(vec![row_of(&p)])
}

/// Scale the roots by `alpha`: coefficient k (from the leading one) is multiplied by alpha^k.
/// A column input is multiplied against the row of powers, so the result is a matrix, as in
/// MATLAB's implementation.
fn polyscale(a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let m = m_arg(a, 0, "polyscale")?;
    let v = vector_arg(a, 0, "polyscale", "Input must be a vector.")?;
    let alpha = cvec_arg(a, 1, "polyscale")?;
    if alpha.len() != 1 {
        return rt("Arrays have incompatible sizes for this operation.");
    }
    let al = alpha[0];
    let powers: Vec<C> = (0..v.len()).scan(C::new(1.0, 0.0), |acc, _| {
        let cur = *acc;
        *acc = acc.mul(al);
        Some(cur)
    }).collect();
    let n = v.len();
    let column = m.dims[1] == 1 && m.dims[0] > 1;
    let cells: Vec<C> = if column {
        // result(i, j) = a(i) * alpha^j, stored column by column.
        (0..n).flat_map(|j| (0..n).map(move |i| (i, j))).map(|(i, j)| v[i].mul(powers[j])).collect()
    } else {
        v.iter().zip(&powers).map(|(c, p)| c.mul(*p)).collect()
    };
    let dims = if column { vec![n, n] } else { vec![1, n] };
    let re: Vec<f64> = cells.iter().map(|c| c.re).collect();
    if cells.iter().all(|c| c.im == 0.0) {
        out(vec![Value::Num(Matrix::from_dims(dims, re))])
    } else {
        out(vec![Value::Num(Matrix::complex(dims, re, cells.iter().map(|c| c.im).collect()))])
    }
}

/// Pad the shorter of numerator and denominator with trailing zeros. Both come back as rows; the
/// third output is the length of the shorter input.
fn eqtflength(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let mut num = vector_arg(a, 0, "eqtflength", "Input must be a vector.")?;
    let mut den = vector_arg(a, 1, "eqtflength", "Input must be a vector.")?;
    let shorter = num.len().min(den.len());
    let n = num.len().max(den.len());
    num.resize(n, C::new(0.0, 0.0));
    den.resize(n, C::new(0.0, 0.0));
    let mut outs = vec![row_of(&num), row_of(&den)];
    if nargout >= 3 {
        outs.push(scalar_value(shorter as f64));
    }
    out(outs)
}

/// Multiply the sections of a second-order-sections matrix `[b0 b1 b2 a0 a1 a2]` and scale the
/// numerator by the gain `g`. With two or more sections one trailing zero shared by the
/// numerator and denominator is dropped.
fn sos2tf(a: &[Value]) -> Ret {
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let sos = real_matrix(a, 0, "sos2tf")?;
    if sos.dims[1] != 6 || sos.dims[0] == 0 {
        return rt("Size of SOS matrix must be Mx6. See \"zp2sos\" or \"ss2sos\" for details.");
    }
    let g = if a.len() == 2 { scalar_arg(a, 1, "sos2tf")? } else { 1.0 };
    let k = sos.dims[0];
    let mut b = vec![g];
    let mut den = vec![1.0];
    for i in 0..k {
        let at = |c: usize| sos.data[c * k + i];
        b = conv(&b, &[at(0), at(1), at(2)]);
        den = conv(&den, &[at(3), at(4), at(5)]);
    }
    if k >= 2 && b.last() == Some(&0.0) && den.last() == Some(&0.0) {
        b.pop();
        den.pop();
    }
    out(vec![real_row(b), real_row(den)])
}

fn conv(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; a.len() + b.len() - 1];
    for (i, &x) in a.iter().enumerate() {
        for (j, &y) in b.iter().enumerate() {
            out[i + j] += x * y;
        }
    }
    out
}

/// True when every pole (root of the denominator) lies strictly inside the unit circle.
fn isstable(a: &[Value]) -> Ret {
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let den = if a.len() == 2 { vector_arg(a, 1, "isstable", "Input must be a vector.")? } else { vec![C::new(1.0, 0.0)] };
    if den.is_empty() {
        return rt("Expected the denominator to be nonempty.");
    }
    // An all-zero denominator has no roots, so nothing violates the condition.
    let stable = poly_roots(&den).iter().all(|r| r.abs() < 1.0);
    out(vec![Value::Num(Matrix::logical(1, 1, vec![if stable { 1.0 } else { 0.0 }]))])
}
