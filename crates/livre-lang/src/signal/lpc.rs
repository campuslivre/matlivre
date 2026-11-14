//! Linear prediction and reflection coefficients: `levinson`, `ac2rc`, `rc2ac`, `rc2poly`,
//! `poly2rc`.

use crate::builtins::{m_arg, Ret};
use crate::cplx::C;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["levinson", "ac2rc", "rc2ac", "rc2poly", "poly2rc"];

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "levinson" => levinson(a, nargout),
        "ac2rc" => ac2rc(a, nargout),
        "rc2ac" => rc2ac(a, nargout),
        "rc2poly" => rc2poly(a, nargout),
        _ => poly2rc(a, nargout),
    }
}

fn too_many(nin: usize, max_in: usize, nargout: usize, max_out: usize) -> R<()> {
    if nin > max_in {
        return rt("Too many input arguments.");
    }
    if nargout > max_out {
        return rt("Too many output arguments.");
    }
    Ok(())
}

fn is_float(m: &Matrix) -> bool {
    matches!(m.kind, Kind::Double | Kind::Single)
}

fn c_of(m: &Matrix) -> Vec<C> {
    (0..m.numel()).map(|i| C::new(m.data[i], m.im_at(i))).collect()
}

fn scalar_c(v: C) -> Value {
    if v.im == 0.0 {
        scalar_value(v.re)
    } else {
        Value::Num(Matrix::complex_scalar(v.re, v.im))
    }
}

fn row_c(v: &[C]) -> Value {
    if v.iter().all(|c| c.im == 0.0) {
        real_row(v.iter().map(|c| c.re).collect())
    } else {
        Value::Num(Matrix::complex(vec![1, v.len()], v.iter().map(|c| c.re).collect(), v.iter().map(|c| c.im).collect()))
    }
}

/// Levinson-Durbin recursion on the autocorrelation `r` for `p` steps: the prediction polynomial,
/// the final error and the reflection coefficients.
fn recursion(r: &[f64], p: usize) -> (Vec<f64>, f64, Vec<f64>) {
    let mut a = vec![1.0];
    let mut e = r[0];
    let mut k = Vec::with_capacity(p);
    for m in 1..=p {
        let delta: f64 = (0..m).map(|i| a[i] * r[m - i]).sum();
        let km = -delta / e;
        k.push(km);
        let na: Vec<f64> = (0..=m).map(|i| if i == 0 { 1.0 } else { (if i < m { a[i] } else { 0.0 }) + km * a[m - i] }).collect();
        a = na;
        e *= 1.0 - km * km;
    }
    (a, e, k)
}

/// The columns of a signal argument: a vector is one column, a matrix has one per column.
fn columns(m: &Matrix) -> (Vec<&[f64]>, bool) {
    let vector = m.dims[0] == 1 || m.dims[1] == 1;
    if vector {
        (vec![&m.data[..]], true)
    } else {
        let rows = m.dims[0];
        ((0..m.dims[1]).map(|c| &m.data[c * rows..(c + 1) * rows]).collect(), false)
    }
}

fn levinson(a: &[Value], nargout: usize) -> Ret {
    too_many(a.len(), 2, nargout, 3)?;
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let r = m_arg(a, 0, "levinson")?;
    if !is_float(&r) {
        return Err(class_error("R", 1, "double, single", r.kind, r.kind != Kind::Char));
    }
    if r.numel() == 0 {
        return rt("Expected input number 1, R, to be nonempty.");
    }
    if r.dims.len() > 2 {
        return rt("N-D arrays are not supported.");
    }
    let mut order = None;
    if a.len() == 2 {
        let n = m_arg(a, 1, "levinson")?;
        if !is_float(&n) {
            return Err(class_error("N", 2, "double, single", n.kind, n.kind != Kind::Char));
        }
        if n.numel() != 1 {
            return rt("Expected input number 2, N, to be a scalar.");
        }
        if n.data[0] < 0.0 {
            return rt("Expected input number 2, N, to be nonnegative.");
        }
        order = Some(n.data[0].floor() as usize);
    }
    let (cols, vector) = columns(&r);
    let len = cols[0].len();
    let p = order.map_or(len - 1, |n| n.min(len - 1));
    let results: Vec<_> = cols.iter().map(|c| recursion(c, p)).collect();
    let mut outs = Vec::new();
    if vector {
        let (av, e, k) = &results[0];
        outs.push(real_row(av.clone()));
        outs.push(scalar_value(*e));
        outs.push(if k.is_empty() { Value::Num(Matrix::zeros(0, 1)) } else { real_col(k.clone()) });
    } else {
        let n = results.len();
        let mut av = vec![0.0; n * (p + 1)];
        let mut kv = vec![0.0; p * n];
        for (c, (aa, _, kk)) in results.iter().enumerate() {
            for (i, v) in aa.iter().enumerate() {
                av[i * n + c] = *v;
            }
            kv[c * p..(c + 1) * p].copy_from_slice(kk);
        }
        outs.push(Value::Num(Matrix::new(n, p + 1, av)));
        outs.push(real_col(results.iter().map(|x| x.1).collect()));
        outs.push(Value::Num(Matrix::new(p, n, kv)));
    }
    out(outs)
}

fn ac2rc(a: &[Value], nargout: usize) -> Ret {
    too_many(a.len(), 1, nargout, 2)?;
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let r = m_arg(a, 0, "ac2rc")?;
    if r.numel() == 0 {
        return rt("Expected input number 1, R, to be nonempty.");
    }
    if r.kind != Kind::Double {
        return rt("Input arguments must be 'double'.");
    }
    if r.dims.len() > 2 {
        return rt("N-D arrays are not supported.");
    }
    // A row vector is one signal; its second output is the row as a column.
    let (cols, _) = columns(&r);
    let len = cols[0].len();
    let ks: Vec<Vec<f64>> = cols.iter().map(|c| recursion(c, len - 1).2).collect();
    let n = ks.len();
    let k = Value::Num(Matrix::new(len - 1, n, ks.into_iter().flatten().collect()));
    let r0 = if r.dims[0] == 1 { real_col(r.data.clone()) } else { real_col((0..r.dims[1]).map(|c| r.data[c * r.dims[0]]).collect()) };
    out(vec![k, r0])
}

fn class_text(m: &Matrix, label: &str, func: &str) -> R<()> {
    if is_float(m) {
        Ok(())
    } else {
        rt(format!("Input {label} in {func} must be double/single, instead it was {}.", m.kind.name()))
    }
}

fn is_vector(m: &Matrix) -> bool {
    m.dims.len() == 2 && m.numel() > 0 && (m.dims[0] == 1 || m.dims[1] == 1)
}

const MISMATCH: &str = "Unable to perform assignment because the left and right sides have a different number of elements.";

fn zero() -> C {
    C::new(0.0, 0.0)
}

/// One step of the order-up recursion: the polynomial of order `m` from the one of order `m - 1`.
fn raise(a: &[C], km: C, m: usize) -> Vec<C> {
    (0..=m)
        .map(|i| {
            if i == 0 {
                return a[0];
            }
            let lo = if i < m { a[i] } else { zero() };
            lo.add(km.mul(a[m - i].conj()))
        })
        .collect()
}

/// Drops the imaginary parts that complex arithmetic invents (NaN from Inf * 0) when the inputs
/// were real.
fn realize(v: &mut [C], real_in: bool) {
    if real_in {
        for c in v.iter_mut() {
            c.im = 0.0;
        }
    }
}

fn all_real(ms: &[&Matrix]) -> bool {
    ms.iter().all(|m| m.im.is_none())
}

/// Prediction polynomial, final error and product of (1 - |k|^2) for reflection coefficients `k`
/// and zero-lag value `r0`.
fn step_up(k: &[C], r0: C) -> (Vec<C>, C, f64) {
    let mut a = vec![C::new(1.0, 0.0)];
    let mut e = r0;
    let mut prod = 1.0;
    for (m, &km) in k.iter().enumerate() {
        a = raise(&a, km, m + 1);
        let f = 1.0 - km.abs() * km.abs();
        e = C::new(e.re * f, e.im * f);
        prod *= f;
    }
    (a, e, prod)
}

fn rc2ac(a: &[Value], nargout: usize) -> Ret {
    too_many(a.len(), 2, nargout, 1)?;
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    let k = m_arg(a, 0, "rc2ac")?;
    let r0 = m_arg(a, 1, "rc2ac")?;
    class_text(&k, "K(Reflection Coefficients)", "rc2ac")?;
    class_text(&r0, "R0(Zero Lag Autocorrelation)", "rc2ac")?;
    if !is_vector(&k) {
        return rt("The reflection coefficients must be stored in a vector.");
    }
    if r0.numel() != 1 {
        return rt(MISMATCH);
    }
    let kc = c_of(&k);
    let real_in = all_real(&[&k, &r0]);
    let given = C::new(r0.data[0], r0.im_at(0));
    // The zero-lag value is recovered from the final error, as MATLAB does.
    let (_, efinal, prod) = step_up(&kc, given);
    let r0c = C::new(efinal.re / prod, efinal.im / prod);
    // The orthogonality of the prediction error gives, at step m,
    // r(m) = -(k(m) e(m-1) + sum_i a(i) r(m-i)).
    let mut r = vec![r0c];
    let mut av = vec![C::new(1.0, 0.0)];
    let mut e = r0c;
    for (m, &km) in kc.iter().enumerate() {
        let m = m + 1;
        let mut s = km.mul(e);
        for i in 1..m {
            s = s.add(av[i].mul(r[m - i]));
        }
        r.push(C::new(-s.re, -s.im));
        av = raise(&av, km, m);
        let f = 1.0 - km.abs() * km.abs();
        e = C::new(e.re * f, e.im * f);
    }
    realize(&mut r, real_in);
    out(vec![col_value(&r)])
}

fn rc2poly(a: &[Value], nargout: usize) -> Ret {
    too_many(a.len(), 2, nargout, 2)?;
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() == 1 && nargout >= 2 {
        return rt("Zero lag autocorrelation, R0, not specified.");
    }
    let k = m_arg(a, 0, "rc2poly")?;
    class_text(&k, "K(reflection coefficients)", "rc2poly")?;
    if !is_vector(&k) {
        return rt("The reflection coefficients must be stored in a vector.");
    }
    let mut r0 = C::new(1.0, 0.0);
    if a.len() == 2 {
        let r = m_arg(a, 1, "rc2poly")?;
        class_text(&r, "R0(zero lag autocorrelation)", "rc2poly")?;
        if r.numel() != 1 {
            return rt(MISMATCH);
        }
        r0 = C::new(r.data[0], r.im_at(0));
    }
    let real_in = a.iter().all(|v| matches!(v, Value::Num(m) if m.im.is_none()));
    let (mut poly, mut e, _) = step_up(&c_of(&k), r0);
    realize(&mut poly, real_in);
    realize(std::slice::from_mut(&mut e), real_in);
    let mut outs = vec![row_c(&poly)];
    if nargout >= 2 {
        outs.push(scalar_c(e));
    }
    out(outs)
}

fn poly2rc(a: &[Value], nargout: usize) -> Ret {
    too_many(a.len(), 2, nargout, 2)?;
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let p = m_arg(a, 0, "poly2rc")?;
    class_text(&p, "A(prediction polynomial)", "poly2rc")?;
    if p.numel() > 0 && !is_vector(&p) {
        return rt("The prediction polynomial must be stored in a vector.");
    }
    let mut efinal = 0.0;
    if a.len() == 2 {
        let e = m_arg(a, 1, "poly2rc")?;
        if e.numel() != 1 {
            return rt(MISMATCH);
        }
        efinal = e.data[0];
    }
    if p.numel() == 0 {
        return out(vec![Value::Num(Matrix::zeros(0, 0)), Value::Num(Matrix::zeros(0, 0))]);
    }
    let mut poly = c_of(&p);
    if poly[0].abs() == 0.0 {
        return rt("Leading coefficient cannot be zero.");
    }
    let lead = poly[0];
    for c in poly.iter_mut() {
        *c = c.div(lead);
    }
    let order = poly.len() - 1;
    let mut k = vec![zero(); order];
    let mut prod = 1.0;
    for m in (1..=order).rev() {
        let km = poly[m];
        if km.abs() == 1.0 {
            return rt("At least one of the reflection coefficients is equal to one. The algorithm fails for this case.");
        }
        k[m - 1] = km;
        let f = 1.0 - km.abs() * km.abs();
        prod *= f;
        let next: Vec<C> = (1..m)
            .map(|i| {
                let v = poly[i].sub(km.mul(poly[m - i].conj()));
                C::new(v.re / f, v.im / f)
            })
            .collect();
        for (i, v) in next.into_iter().enumerate() {
            poly[i + 1] = v;
        }
    }
    let kv = if order == 0 { Value::Num(Matrix::zeros(0, 0)) } else { col_value(&k) };
    out(vec![kv, scalar_value(efinal / prod)])
}
