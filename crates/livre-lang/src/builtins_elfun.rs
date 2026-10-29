//! The rest of MATLAB's elementary math (`toolbox/matlab/elfun`): reciprocal trigonometric and
//! hyperbolic functions and their inverses, degree variants, `atan2d`, the `real*` functions
//! that refuse complex results, `pow2`, `nextpow2`, `unwrap` and `cplxpair`.
//!
//! Most are compositions, as MATLAB defines them (`cot(x)` is `1./tan(x)`), so domain errors
//! and complex branches come from the underlying function.

use std::f64::consts::PI;

use crate::ast::BinOp;
use crate::builtins::{default_dim, dim_arg, m_arg, one, slices, unary_math, Ret};
use crate::cplx::C;
use crate::error::{rt, LangError};
use crate::ops;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "sec", "csc", "cot", "sech", "csch", "coth", "secd", "cscd", "cotd", "asec", "acsc", "acot", "asech", "acsch", "acoth", "asecd", "acscd",
    "acotd", "atan2d", "pow2", "realpow", "reallog", "realsqrt", "nextpow2", "unwrap", "cplxpair",
];

fn math(name: &str, m: &Matrix) -> R<Matrix> {
    unary_math(name, m).expect("unary_math covers the base functions")
}

fn recip(m: &Matrix) -> R<Matrix> {
    ops::num_binary(BinOp::Div, &Matrix::scalar(1.0), m)
}

/// Radians to degrees for a possibly complex result.
fn to_degrees(mut m: Matrix) -> Matrix {
    let k = 180.0 / PI;
    m.data.iter_mut().for_each(|x| *x *= k);
    if let Some(im) = &mut m.im {
        im.iter_mut().for_each(|x| *x *= k);
    }
    m
}

/// An inverse reciprocal function, `base(1./x)`; at `x = 0` the argument is infinite and the
/// base function's branch formulas give MATLAB's limits (`asec(0)` is `0 + Inf*i`).
fn inverse(base: &str, m: &Matrix) -> R<Matrix> {
    math(base, &recip(m)?)
}

/// `nextpow2` of one magnitude: the smallest p with 2^p >= |x|.
fn nextpow2(x: f64) -> f64 {
    let a = x.abs();
    if a == 0.0 || a.is_nan() || a.is_infinite() {
        return if a == 0.0 { 0.0 } else { a };
    }
    // frexp: a = f * 2^e with f in [0.5, 1); a power of two (f == 0.5) needs one less.
    let (a, shift) = if a < f64::MIN_POSITIVE { (a * 2f64.powi(54), 54) } else { (a, 0) };
    let bits = a.to_bits();
    let e = ((bits >> 52) & 0x7ff) as i64 - 1022 - shift;
    let power_of_two = bits & ((1u64 << 52) - 1) == 0;
    (if power_of_two { e - 1 } else { e }) as f64
}

/// MATLAB's unwrap along one slice: jumps of at least `tol` become their 2*pi complement. The
/// correction is kept as a whole number of turns K and applied as `x - K*2*pi`, which rounds
/// as MATLAB does.
fn unwrap_slice(p: &mut [f64], tol: f64) {
    let mut turns = 0.0;
    let mut prev = p.iter().copied().find(|v| v.is_finite()).unwrap_or(0.0);
    for x in p.iter_mut().skip(1) {
        // Non-finite values pass through; unwrapping resumes from the last finite one.
        if !x.is_finite() {
            continue;
        }
        let dp = *x - prev;
        prev = *x;
        // mod(dp + pi, 2*pi) - pi, with mod computed as MATLAB does (a - floor(a/m)*m).
        let (a, m) = (dp + PI, 2.0 * PI);
        let mut dps = a - (a / m).floor() * m - PI;
        if dps == -PI && dp > 0.0 {
            dps = PI;
        }
        if dp.abs() >= tol {
            turns += ((dp - dps) / (2.0 * PI)).round();
        }
        *x -= turns * 2.0 * PI;
    }
}

/// Sorts one slice into complex conjugate pairs (negative imaginary part first, by increasing
/// real part) followed by the real numbers in increasing order.
fn cplxpair_slice(z: &[C], tol: f64) -> R<Vec<C>> {
    let near_real = |c: &C| c.im.abs() <= tol * c.abs();
    let mut reals: Vec<f64> = z.iter().filter(|c| near_real(c)).map(|c| c.re).collect();
    let mut cplx: Vec<C> = z.iter().filter(|c| !near_real(c)).copied().collect();
    reals.sort_by(|a, b| a.total_cmp(b));
    cplx.sort_by(|a, b| a.re.total_cmp(&b.re).then(a.im.abs().total_cmp(&b.im.abs())).then(a.im.total_cmp(&b.im)));
    let mut out = Vec::with_capacity(z.len());
    while let Some(first) = cplx.first().copied() {
        cplx.remove(0);
        let mate = cplx.iter().position(|c| (c.re - first.re).abs() <= tol * first.abs() && (c.im + first.im).abs() <= tol * first.abs());
        let Some(k) = mate else { return rt("Complex numbers can't be paired.") };
        let second = cplx.remove(k);
        let (neg, pos) = if first.im < 0.0 { (first, second) } else { (second, first) };
        // A pair is stored as exact conjugates, from the average of the two.
        let re = (neg.re + pos.re) / 2.0;
        let im = (pos.im - neg.im) / 2.0;
        out.push(C::new(re, -im));
        out.push(C::new(re, im));
    }
    out.extend(reals.into_iter().map(|r| C::new(r, 0.0)));
    Ok(out)
}

pub fn call(name: &str, a: &[Value], _nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    let x = m_arg(a, 0, name)?;
    let out = match name {
        "sec" => recip(&math("cos", &x)?)?,
        "csc" => recip(&math("sin", &x)?)?,
        "cot" => recip(&math("tan", &x)?)?,
        "sech" => recip(&math("cosh", &x)?)?,
        "csch" => recip(&math("sinh", &x)?)?,
        "coth" => recip(&math("tanh", &x)?)?,
        "secd" => recip(&math("cosd", &x)?)?,
        "cscd" => recip(&math("sind", &x)?)?,
        "cotd" => recip(&math("tand", &x)?)?,
        "asec" => inverse("acos", &x)?,
        "acsc" => inverse("asin", &x)?,
        "acot" => inverse("atan", &x)?,
        "asech" => inverse("acosh", &x)?,
        "acsch" => inverse("asinh", &x)?,
        "acoth" => inverse("atanh", &x)?,
        "asecd" => to_degrees(inverse("acos", &x)?),
        "acscd" => to_degrees(inverse("asin", &x)?),
        "acotd" => to_degrees(inverse("atan", &x)?),
        "atan2d" => {
            let y = x;
            let x = m_arg(a, 1, name)?;
            if y.is_complex() || x.is_complex() {
                return rt("Inputs must be real.");
            }
            ops::broadcast(&y, &x, |y, x| 180.0 / PI * y.atan2(x))?
        }
        "pow2" => match a.get(1) {
            None => ops::num_binary(BinOp::Pow, &Matrix::scalar(2.0), &x)?,
            Some(_) => {
                let e = m_arg(a, 1, name)?;
                ops::num_binary(BinOp::Mul, &x, &ops::num_binary(BinOp::Pow, &Matrix::scalar(2.0), &e)?)?
            }
        },
        "realpow" => {
            let mut r = ops::num_binary(BinOp::Pow, &x, &m_arg(a, 1, name)?)?;
            r.narrow();
            if r.is_complex() {
                return rt("Realpow produced complex result.");
            }
            r
        }
        "reallog" | "realsqrt" => {
            if x.is_complex() || x.data.iter().any(|&v| v < 0.0) {
                return rt(if name == "reallog" { "Reallog produced complex result." } else { "Realsqrt produced complex result." });
            }
            math(if name == "reallog" { "log" } else { "sqrt" }, &x)?
        }
        "nextpow2" => {
            let mags: Vec<f64> = (0..x.numel()).map(|i| if x.is_complex() { C::new(x.data[i], x.im_at(i)).abs() } else { x.data[i] }).collect();
            let out = Matrix::from_dims(x.dims.clone(), mags.into_iter().map(nextpow2).collect());
            match x.kind {
                Kind::Single | Kind::Int(_) => out.cast_to(x.kind),
                _ => out,
            }
        }
        "unwrap" => {
            if x.is_complex() {
                return rt("Input must be real.");
            }
            let tol = match a.get(1) {
                Some(Value::Num(t)) if !t.is_empty() => t.data[0].max(PI),
                _ => PI,
            };
            let dim = if a.len() > 2 { dim_arg(a, 2, &x)? } else { default_dim(&x.dims) };
            let mut out = x.clone().cast_to(if x.kind == Kind::Single { Kind::Single } else { Kind::Double });
            let (_, sl) = slices(&x.dims, dim);
            for idx in sl {
                let mut v: Vec<f64> = idx.iter().map(|&i| out.data[i]).collect();
                unwrap_slice(&mut v, tol);
                for (&i, val) in idx.iter().zip(v) {
                    out.data[i] = val;
                }
            }
            out
        }
        "cplxpair" => {
            let tol = match a.get(1) {
                Some(Value::Num(t)) if !t.is_empty() => t.data[0],
                _ => 100.0 * f64::EPSILON,
            };
            let dim = if a.len() > 2 { dim_arg(a, 2, &x)? } else { default_dim(&x.dims) };
            let mut re = vec![0.0; x.numel()];
            let mut im = vec![0.0; x.numel()];
            let (_, sl) = slices(&x.dims, dim);
            for idx in sl {
                let z: Vec<C> = idx.iter().map(|&i| C::new(x.data[i], x.im_at(i))).collect();
                for (&i, c) in idx.iter().zip(cplxpair_slice(&z, tol)?) {
                    re[i] = c.re;
                    im[i] = c.im;
                }
            }
            if x.is_complex() { Matrix::complex(x.dims.clone(), re, im) } else { Matrix::from_dims(x.dims.clone(), re) }
        }
        _ => unreachable!(),
    };
    one(Value::Num(out))
}
