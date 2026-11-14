//! Analog lowpass prototypes (`buttap`, `cheb1ap`, `cheb2ap`), the lowpass-to-X frequency
//! transformations (`lp2lp`, `lp2hp`, `lp2bp`, `lp2bs`) and `bilinear`.
//!
//! The transformations take a transfer function `(b, a)` in descending powers of s. Internally
//! polynomials are real vectors in ascending powers.

use crate::builtins::Ret;
use crate::cplx::C;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["buttap", "cheb1ap", "cheb2ap", "lp2lp", "lp2hp", "lp2bp", "lp2bs", "bilinear"];

const PI: f64 = std::f64::consts::PI;

/// Zeros, poles and gain.
#[derive(Clone, Debug)]
pub(crate) struct Zpk {
    pub z: Vec<C>,
    pub p: Vec<C>,
    pub k: f64,
}

fn prod_neg(v: &[C]) -> C {
    v.iter().fold(C::new(1.0, 0.0), |acc, &r| acc.mul(C::new(-r.re, -r.im)))
}

/// Pairs `[w, conj(w)]` for each w, then the real pole `last` when `n` is odd.
fn with_conjugates(half: Vec<C>, odd_last: Option<C>) -> Vec<C> {
    let mut out = Vec::new();
    for w in half {
        out.push(w);
        out.push(w.conj());
    }
    out.extend(odd_last);
    out
}

pub(crate) fn buttap(n: usize) -> Zpk {
    let half = (0..n / 2).map(|i| {
        let theta = PI * (2 * i + 1) as f64 / (2 * n) as f64 + PI / 2.0;
        C::new(theta.cos(), theta.sin())
    });
    let p = with_conjugates(half.collect(), (n % 2 == 1).then(|| C::new(-1.0, 0.0)));
    let k = prod_neg(&p).re;
    Zpk { z: Vec::new(), p, k }
}

pub(crate) fn cheb1ap(n: usize, rp: f64) -> Zpk {
    let eps = (10f64.powf(rp / 10.0) - 1.0).sqrt();
    let mu = (1.0 / eps).asinh() / n as f64;
    let p = cheb_poles(n, mu);
    let mut k = prod_neg(&p).re;
    if n % 2 == 0 {
        k /= (1.0 + eps * eps).sqrt();
    }
    Zpk { z: Vec::new(), p, k }
}

/// sinh(mu) cos(theta_k) + j cosh(mu) sin(theta_k) for theta_k = pi/2 + pi (2k - 1) / (2n),
/// k = 1..n: the poles in order around the left half of the ellipse.
fn cheb_poles(n: usize, mu: f64) -> Vec<C> {
    (0..n)
        .map(|i| {
            let theta = PI / 2.0 + PI * (2 * i + 1) as f64 / (2 * n) as f64;
            C::new(mu.sinh() * theta.cos(), mu.cosh() * theta.sin())
        })
        .collect()
}

pub(crate) fn cheb2ap(n: usize, rs: f64) -> Zpk {
    let delta = 1.0 / (10f64.powf(rs / 10.0) - 1.0).sqrt();
    let mu = (1.0 / delta).asinh() / n as f64;
    let z_half: Vec<C> = (0..n / 2)
        .map(|i| {
            let phi = PI * (2 * i + 1) as f64 / (2 * n) as f64;
            C::new(0.0, 1.0 / phi.cos())
        })
        .collect();
    let z = with_conjugates(z_half, None);
    let p: Vec<C> = cheb_poles(n, mu).into_iter().map(|c| C::new(1.0, 0.0).div(c)).collect();
    let k = prod_neg(&p).div(prod_neg(&z)).re;
    Zpk { z, p, k }
}

// ---- frequency transformations on zero-pole-gain form ----------------------------------------

pub(crate) fn zpk_lp2lp(f: &Zpk, wo: f64) -> Zpk {
    let scale = |v: &[C]| v.iter().map(|r| C::new(r.re * wo, r.im * wo)).collect::<Vec<_>>();
    Zpk { z: scale(&f.z), p: scale(&f.p), k: f.k * wo.powi(f.p.len() as i32 - f.z.len() as i32) }
}

pub(crate) fn zpk_lp2hp(f: &Zpk, wo: f64) -> Zpk {
    let inv = |v: &[C]| v.iter().map(|&r| C::new(wo, 0.0).div(r)).collect::<Vec<_>>();
    let mut z = inv(&f.z);
    z.extend(std::iter::repeat_n(C::new(0.0, 0.0), f.p.len() - f.z.len()));
    let k = f.k * prod_neg(&f.z).div(prod_neg(&f.p)).re;
    Zpk { z, p: inv(&f.p), k }
}

pub(crate) fn zpk_lp2bp(f: &Zpk, wo: f64, bw: f64) -> Zpk {
    let map = |v: &[C]| -> Vec<C> {
        let mut out = Vec::new();
        for &r in v {
            let h = C::new(r.re * bw / 2.0, r.im * bw / 2.0);
            let root = h.mul(h).sub(C::new(wo * wo, 0.0)).sqrt();
            out.push(h.add(root));
            out.push(h.sub(root));
        }
        out
    };
    let mut z = map(&f.z);
    z.extend(std::iter::repeat_n(C::new(0.0, 0.0), f.p.len() - f.z.len()));
    Zpk { z, p: map(&f.p), k: f.k * bw.powi(f.p.len() as i32 - f.z.len() as i32) }
}

pub(crate) fn zpk_lp2bs(f: &Zpk, wo: f64, bw: f64) -> Zpk {
    let map = |v: &[C]| -> Vec<C> {
        let mut out = Vec::new();
        for &r in v {
            let h = C::new(bw / 2.0, 0.0).div(r);
            let root = h.mul(h).sub(C::new(wo * wo, 0.0)).sqrt();
            out.push(h.add(root));
            out.push(h.sub(root));
        }
        out
    };
    let mut z = map(&f.z);
    for _ in 0..f.p.len() - f.z.len() {
        z.push(C::new(0.0, wo));
        z.push(C::new(0.0, -wo));
    }
    let k = f.k * prod_neg(&f.z).div(prod_neg(&f.p)).re;
    Zpk { z, p: map(&f.p), k }
}

/// Bilinear transform with sample rate `fs` (or the pre-warped `2 fs2` when given).
pub(crate) fn zpk_bilinear(f: &Zpk, fs2: f64) -> Zpk {
    let to_z = |r: C| C::new(fs2, 0.0).add(r).div(C::new(fs2, 0.0).sub(r));
    let mut z: Vec<C> = f.z.iter().map(|&r| to_z(r)).collect();
    z.extend(std::iter::repeat_n(C::new(-1.0, 0.0), f.p.len() - f.z.len()));
    let p: Vec<C> = f.p.iter().map(|&r| to_z(r)).collect();
    let num = f.z.iter().fold(C::new(1.0, 0.0), |acc, &r| acc.mul(C::new(fs2, 0.0).sub(r)));
    let den = f.p.iter().fold(C::new(1.0, 0.0), |acc, &r| acc.mul(C::new(fs2, 0.0).sub(r)));
    Zpk { z, p, k: f.k * num.div(den).re }
}

/// Real coefficients `b`, `a` (descending) of a zero-pole-gain filter.
pub(crate) fn zpk_to_ba(f: &Zpk) -> (Vec<f64>, Vec<f64>) {
    let (b, _) = poly_value(&f.z, C::new(f.k, 0.0));
    let (a, _) = poly_value(&f.p, C::new(1.0, 0.0));
    (b.iter().map(|c| c.re).collect(), a.iter().map(|c| c.re).collect())
}

// ---- polynomial algebra in ascending powers --------------------------------------------------

fn mul(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; a.len() + b.len() - 1];
    for (i, &x) in a.iter().enumerate() {
        for (j, &y) in b.iter().enumerate() {
            out[i + j] += x * y;
        }
    }
    out
}

fn add_into(dst: &mut Vec<f64>, src: &[f64]) {
    if dst.len() < src.len() {
        dst.resize(src.len(), 0.0);
    }
    for (d, s) in dst.iter_mut().zip(src) {
        *d += s;
    }
}

fn powp(p: &[f64], n: usize) -> Vec<f64> {
    (0..n).fold(vec![1.0], |acc, _| mul(&acc, p))
}

/// Descending coefficients to ascending powers.
fn asc(v: &[f64]) -> Vec<f64> {
    v.iter().rev().copied().collect()
}

/// Substitute `s = num_s / den_s` in `c(s)` (ascending), clearing denominators with degree `m`:
/// the result is sum_j c_j num_s^j den_s^(m-j).
fn substitute(c: &[f64], num_s: &[f64], den_s: &[f64], m: usize) -> Vec<f64> {
    let mut out = vec![0.0];
    for (j, &cj) in c.iter().enumerate() {
        let term = mul(&powp(num_s, j), &powp(den_s, m - j));
        let scaled: Vec<f64> = term.iter().map(|t| t * cj).collect();
        add_into(&mut out, &scaled);
    }
    out
}

/// Normalise to a monic denominator and drop the numerator's leading zeros. Input is ascending;
/// output is descending.
fn finish(num: Vec<f64>, den: Vec<f64>) -> (Vec<f64>, Vec<f64>) {
    let mut num: Vec<f64> = num.into_iter().rev().collect();
    let mut den: Vec<f64> = den.into_iter().rev().collect();
    while den.len() > 1 && den[0] == 0.0 {
        den.remove(0);
    }
    let lead = den[0];
    for v in num.iter_mut().chain(den.iter_mut()) {
        *v /= lead;
    }
    while num.len() > 1 && num[0] == 0.0 {
        num.remove(0);
    }
    (num, den)
}

fn transform_ba(kind: &str, b: &[f64], a: &[f64], wo: f64, bw: f64) -> (Vec<f64>, Vec<f64>) {
    let (b, a) = (asc(b), asc(a));
    let m = a.len().max(b.len()) - 1;
    let (num_s, den_s): (Vec<f64>, Vec<f64>) = match kind {
        "lp2lp" => (vec![0.0, 1.0], vec![wo]),
        "lp2hp" => (vec![wo], vec![0.0, 1.0]),
        "lp2bp" => (vec![wo * wo, 0.0, 1.0], vec![0.0, bw]),
        _ => (vec![0.0, bw], vec![wo * wo, 0.0, 1.0]),
    };
    finish(substitute(&b, &num_s, &den_s, m), substitute(&a, &num_s, &den_s, m))
}

fn bilinear_ba(b: &[f64], a: &[f64], fs2: f64) -> (Vec<f64>, Vec<f64>) {
    let (b, a) = (asc(b), asc(a));
    let m = a.len().max(b.len()) - 1;
    // s = fs2 (z - 1) / (z + 1).
    let num_s = vec![-fs2, fs2];
    let den_s = vec![1.0, 1.0];
    finish(substitute(&b, &num_s, &den_s, m), substitute(&a, &num_s, &den_s, m))
}

// ---- builtin entry points ---------------------------------------------------------------------

fn order_arg(a: &[Value], name: &str) -> Result<usize, crate::error::LangError> {
    let m = crate::builtins::m_arg(a, 0, name)?;
    if m.numel() != 1 {
        return rt("Expected N to be a scalar.");
    }
    let n = m.data[0];
    if n <= 0.0 {
        return rt("Expected N to be positive.");
    }
    if n.fract() != 0.0 {
        return rt("Expected N to be integer-valued.");
    }
    Ok(n as usize)
}

fn zpk_values(f: &Zpk, empty_zeros_00: bool) -> Vec<Value> {
    // MATLAB returns the empty zero vector of buttap and cheb1ap as 0-by-0, the others 0-by-1.
    let z = if f.z.is_empty() && empty_zeros_00 { Value::Num(Matrix::zeros(0, 0)) } else { col_value(&f.z) };
    vec![z, col_value(&f.p), scalar_value(f.k)]
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "buttap" => {
            if a.len() != 1 {
                return rt(if a.is_empty() { "Not enough input arguments." } else { "Too many input arguments." });
            }
            out(zpk_values(&buttap(order_arg(a, name)?), true))
        }
        "cheb1ap" | "cheb2ap" => {
            if a.len() < 2 {
                return rt("Not enough input arguments.");
            }
            if a.len() > 2 {
                return rt("Too many input arguments.");
            }
            let n = order_arg(a, name)?;
            let r = scalar_arg(a, 1, name)?;
            if r < 0.0 {
                return rt(if name == "cheb1ap" { "Expected Rp to be nonnegative." } else { "Expected Rs to be nonnegative." });
            }
            out(zpk_values(&if name == "cheb1ap" { cheb1ap(n, r) } else { cheb2ap(n, r) }, name == "cheb1ap"))
        }
        "bilinear" => bilinear_call(a, nargout),
        _ => transform_call(name, a),
    }
}

fn transform_call(name: &str, a: &[Value]) -> Ret {
    let band = matches!(name, "lp2bp" | "lp2bs");
    let want = if band { 4 } else { 3 };
    if a.len() < want {
        return rt("Not enough input arguments.");
    }
    if a.len() > want {
        return rt("Too many input arguments.");
    }
    let b = rvec_arg(a, 0, name)?;
    let den = rvec_arg(a, 1, name)?;
    if b.is_empty() || den.is_empty() || den.iter().all(|&v| v == 0.0) {
        return rt("Expected numerator and denominator to be nonempty with a nonzero denominator.");
    }
    if b.len() > den.len() {
        return rt("Order of denominator must be greater than or equal to order of numerator.");
    }
    let wo = scalar_arg(a, 2, name)?;
    let bw = if band { scalar_arg(a, 3, name)? } else { 0.0 };
    if wo <= 0.0 || (band && bw <= 0.0) {
        return rt("Expected frequency arguments to be positive.");
    }
    let (nb, na) = transform_ba(name, &b, &den, wo, bw);
    out(vec![real_row(nb), real_row(na)])
}

fn bilinear_call(a: &[Value], nargout: usize) -> Ret {
    // bilinear(num, den, fs[, fp]) with two outputs, or bilinear(z, p, k, fs[, fp]) with three.
    let zpk_form = nargout >= 3 && !matches!(a.get(1), Some(Value::Num(m)) if m.dims[0] == 1 && m.numel() > 1);
    let (fixed, fs_at) = if zpk_form { (3, 3) } else { (2, 2) };
    if a.len() < fixed + 1 {
        return rt("Not enough input arguments.");
    }
    if a.len() > fixed + 2 {
        return rt("Too many input arguments.");
    }
    let fs = scalar_arg(a, fs_at, "bilinear")?;
    if fs <= 0.0 {
        return rt("Expected sample rate to be positive.");
    }
    let fs2 = if a.len() == fixed + 2 {
        let fp = scalar_arg(a, fs_at + 1, "bilinear")?;
        if fp <= 0.0 || fp >= fs / 2.0 {
            return rt(format!("Expected input number {}, Fp, to be a scalar with value < {}.", fs_at + 2, fs / 2.0));
        }
        2.0 * PI * fp / (PI * fp / fs).tan()
    } else {
        2.0 * fs
    };
    if zpk_form {
        let f = Zpk { z: cvec_arg(a, 0, "bilinear")?, p: cvec_arg(a, 1, "bilinear")?, k: scalar_arg(a, 2, "bilinear")? };
        if f.z.len() > f.p.len() {
            return rt("The number of zeros must not exceed the number of poles.");
        }
        out(zpk_values(&zpk_bilinear(&f, fs2), false))
    } else {
        let b = rvec_arg(a, 0, "bilinear")?;
        let den = rvec_arg(a, 1, "bilinear")?;
        if b.is_empty() || den.is_empty() || den.iter().all(|&v| v == 0.0) || b.len() > den.len() {
            return rt("Expected a proper transfer function with a nonzero denominator.");
        }
        let (nb, na) = bilinear_ba(&b, &den, fs2);
        out(vec![real_row(nb), real_row(na)])
    }
}
