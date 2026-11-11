//! Polynomial and filter-form utilities: `polystab`, `polyscale`, `eqtflength`, `sos2tf`,
//! `isstable`.

use crate::builtins::{m_arg, Ret};
use crate::cplx::{poly_roots, C};
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["polystab", "polyscale", "eqtflength", "sos2tf", "isstable", "tf2zp", "tf2zpk", "zp2tf", "sos2zp"];

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "tf2zp" => return tf2zp(a, nargout),
        "tf2zpk" => return tf2zpk(a, nargout),
        "zp2tf" => return zp2tf(a, nargout),
        "sos2zp" => return sos2zp(a, nargout),
        _ => {}
    }
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

fn c0() -> C {
    C::new(0.0, 0.0)
}

fn num_value(rows: usize, cols: usize, v: &[C]) -> Value {
    let re: Vec<f64> = v.iter().map(|c| c.re).collect();
    if v.iter().all(|c| c.im == 0.0) {
        Value::Num(Matrix::new(rows, cols, re))
    } else {
        Value::Num(Matrix::complex(vec![rows, cols], re, v.iter().map(|c| c.im).collect()))
    }
}

fn scalar_of(v: C) -> Value {
    num_value(1, 1, &[v])
}

const DATA_TYPE: &str = "Invalid data type. Input matrix must be double or single.";

/// Zeros, poles and gain of a transfer function in powers of z^-1 (or s): the shorter of
/// numerator and denominator is padded with trailing zeros, the zeros common to both are dropped,
/// and the gain is the first nonzero numerator coefficient over the first denominator one.
fn zpk_of(num: &[C], den: &[C]) -> R<(Vec<C>, Vec<C>, C)> {
    if den.iter().all(|c| c.abs() == 0.0) {
        return rt("Denominator cannot be zero or empty.");
    }
    if den[0].abs() == 0.0 {
        return rt("Denominator must have non-zero leading coefficient.");
    }
    let n = num.len().max(den.len());
    let mut b = num.to_vec();
    let mut a = den.to_vec();
    b.resize(n, c0());
    a.resize(n, c0());
    while b.len() > 1 && b.last().is_some_and(|c| c.abs() == 0.0) && a.last().is_some_and(|c| c.abs() == 0.0) {
        b.pop();
        a.pop();
    }
    let k = b.iter().find(|c| c.abs() != 0.0).map_or(c0(), |&lead| lead.div(a[0]));
    Ok((roots_matlab(&b)?, roots_matlab(&a)?, k))
}

/// Class and shape checks that `tf2zpk` shares with the section conversion.
fn tf_vector(a: &[Value], i: usize, label: &str) -> R<Vec<C>> {
    let m = m_arg(a, i, "tf2zpk")?;
    if m.kind == Kind::Char {
        return Err(class_error(label, i + 1, "double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64", m.kind, false));
    }
    if !matches!(m.kind, Kind::Double | Kind::Single) {
        return rt(DATA_TYPE);
    }
    if m.numel() > 1 && (m.dims.len() > 2 || (m.dims[0] != 1 && m.dims[1] != 1)) {
        return rt(format!("Expected input number {}, {label}, to be a vector.", i + 1));
    }
    Ok((0..m.numel()).map(|k| C::new(m.data[k], m.im_at(k))).collect())
}

fn tf2zpk(a: &[Value], nargout: usize) -> Ret {
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let mut num = tf_vector(a, 0, "num")?;
    let mut den = if a.len() == 2 { tf_vector(a, 1, "den")? } else { vec![C::new(1.0, 0.0)] };
    if num.is_empty() {
        num = vec![c0()];
    }
    if den.is_empty() {
        den = vec![C::new(1.0, 0.0)];
    }
    let (z, p, k) = zpk_of(&num, &den)?;
    out(vec![col_value(&z), col_value(&p), scalar_of(k)])
}

fn tf2zp(a: &[Value], nargout: usize) -> Ret {
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    let num = m_arg(a, 0, "tf2zp")?;
    let den = m_arg(a, 1, "tf2zp")?;
    for m in [&num, &den] {
        if m.kind == Kind::Char {
            return rt("Argument following 'like' must be a variable with a data type that supports ZEROS, such as a double or single variable.");
        }
        if !matches!(m.kind, Kind::Double | Kind::Single) {
            return rt(DATA_TYPE);
        }
    }
    if den.dims[0] > 1 || den.dims.len() > 2 {
        return rt("Denominator must be a row vector.");
    }
    if num.dims.len() > 2 {
        return rt("N-D arrays are not supported.");
    }
    if num.dims[1] > den.numel() {
        return rt("Transfer function not proper.");
    }
    if den.numel() == 0 || den.data[0] == 0.0 && den.im_at(0) == 0.0 {
        return rt("Denominator must have non-zero leading coefficient.");
    }
    let d: Vec<C> = (0..den.numel()).map(|k| C::new(den.data[k], den.im_at(k))).collect();
    let p = roots_matlab(&d)?;
    let (rows, cols) = (num.dims[0], num.dims[1]);
    if num.numel() == 0 {
        return out(vec![Value::Num(Matrix::zeros(0, 0)), col_value(&p), Value::Num(Matrix::zeros(0, 1))]);
    }
    let row = |r: usize| -> Vec<C> { (0..cols).map(|c| C::new(num.data[c * rows + r], num.im_at(c * rows + r))).collect() };
    let mut z_all = Vec::new();
    let mut k_all = Vec::new();
    let mut nz = None;
    for r in 0..rows {
        let coefs = row(r);
        let z = roots_matlab(&coefs)?;
        if *nz.get_or_insert(z.len()) != z.len() {
            return rt("Dimensions of arrays being concatenated are not consistent.");
        }
        z_all.extend(z);
        k_all.push(coefs[0].div(d[0]));
    }
    out(vec![num_value(nz.unwrap_or(0), rows, &z_all), col_value(&p), num_value(rows, 1, &k_all)])
}

fn zp2tf(a: &[Value], nargout: usize) -> Ret {
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    let z = m_arg(a, 0, "zp2tf")?;
    let p = m_arg(a, 1, "zp2tf")?;
    let k = m_arg(a, 2, "zp2tf")?;
    if z.dims.len() == 2 && z.dims[0] == 1 && z.dims[1] > 1 {
        return rt("Z must be a column vector.");
    }
    let cols = if z.numel() == 0 { 1 } else { z.dims[1] };
    if k.numel() != cols {
        return rt("K must have as many elements as Z has columns.");
    }
    if matches!(k.kind, Kind::Int(_)) {
        return rt("Integers can only be combined with integers of the same class, or scalar doubles.");
    }
    let denom = poly_of(&(0..p.numel()).map(|i| C::new(p.data[i], p.im_at(i))).collect::<Vec<_>>());
    let den: Vec<f64> = denom.iter().map(|c| c.re).collect();
    let rows = if z.numel() == 0 { 0 } else { z.dims[0] };
    let mut width = 0;
    let mut nums: Vec<Vec<f64>> = Vec::new();
    for c in 0..cols {
        let roots: Vec<C> = (0..rows).map(|r| C::new(z.data[c * rows + r], z.im_at(c * rows + r))).collect();
        let kc = C::new(k.data[c], k.im_at(c));
        let mut b: Vec<f64> = poly_of(&roots).iter().map(|co| kc.mul(*co).re).collect();
        if b.len() < den.len() {
            let mut padded = vec![0.0; den.len() - b.len()];
            padded.extend(b);
            b = padded;
        }
        width = b.len();
        nums.push(b);
    }
    let mut data = vec![0.0; cols * width];
    for (r, b) in nums.iter().enumerate() {
        for (c, v) in b.iter().enumerate() {
            data[c * cols + r] = *v;
        }
    }
    let mut b = Value::Num(Matrix::new(cols, width, data));
    let mut a_out = real_row(den);
    if z.kind == Kind::Single || k.kind == Kind::Single {
        b = crate::signal::to_single(b);
    }
    if p.kind == Kind::Single {
        a_out = crate::signal::to_single(a_out);
    }
    out(vec![b, a_out])
}

fn sos2zp(a: &[Value], nargout: usize) -> Ret {
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let sos = m_arg(a, 0, "sos2zp")?;
    if !matches!(sos.kind, Kind::Double | Kind::Single) {
        return Err(class_error("SOS", 1, "single, double", sos.kind, false));
    }
    let g = if a.len() == 2 { m_arg(a, 1, "sos2zp")? } else { Matrix::scalar(1.0) };
    let mut z = Vec::new();
    let mut p = Vec::new();
    let mut gain = C::new(1.0, 0.0);
    if sos.numel() > 0 {
        if sos.dims.len() > 2 {
            return rt("N-D arrays are not supported.");
        }
        let (rows, cols) = (sos.dims[0], sos.dims[1]);
        if cols < 6 {
            return rt(format!("Index in position 2 exceeds array bounds. Index must not exceed {cols}."));
        }
        for r in 0..rows {
            let at = |c: usize| C::new(sos.data[c * rows + r], sos.im_at(c * rows + r));
            let (zi, pi, ki) = zpk_of(&[at(0), at(1), at(2)], &[at(3), at(4), at(5)])?;
            z.extend(zi);
            p.extend(pi);
            gain = gain.mul(ki);
        }
    }
    let k: Vec<C> = (0..g.numel()).map(|i| gain.mul(C::new(g.data[i], g.im_at(i)))).collect();
    out(vec![col_value(&z), col_value(&p), num_value(g.dims[0], g.dims[1], &k)])
}
