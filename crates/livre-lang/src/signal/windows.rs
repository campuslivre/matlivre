//! Spectral windows: `rectwin`, `bartlett`, `triang`, `hamming`, `hann`, `blackman`,
//! `blackmanharris`, `nuttallwin`, `flattopwin`, `kaiser`, `tukeywin`, `gausswin`, `parzenwin`,
//! `bohmanwin`, `barthannwin` and `chebwin`.
//!
//! Every window comes back as a column vector. The cosine-sum windows accept the `"symmetric"`
//! (default) and `"periodic"` flags; the periodic form is the symmetric window of length L + 1
//! without its last point. The symmetric half is computed once and reflected, so the window is
//! exactly symmetric.

use crate::builtins::{m_arg, one, Ret};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "rectwin", "bartlett", "triang", "hamming", "hann", "blackman", "blackmanharris", "nuttallwin", "flattopwin", "kaiser", "tukeywin", "gausswin",
    "parzenwin", "bohmanwin", "barthannwin", "chebwin",
];

const TAU: f64 = std::f64::consts::TAU;
const PI: f64 = std::f64::consts::PI;

/// Cosine-sum coefficients a0, a1, ...: w(n) = a0 - a1 cos(2 pi n / (L-1)) + a2 cos(4 pi n / (L-1)) - ...
fn cosine_coefficients(name: &str) -> Option<&'static [f64]> {
    Some(match name {
        "hamming" => &[0.54, 0.46],
        "hann" => &[0.5, 0.5],
        "blackman" => &[0.42, 0.5, 0.08],
        "blackmanharris" => &[0.35875, 0.48829, 0.14128, 0.01168],
        "nuttallwin" => &[0.3635819, 0.4891775, 0.1365995, 0.0106411],
        "flattopwin" => &[0.21557895, 0.41663158, 0.277263158, 0.083578947, 0.006947368],
        _ => return None,
    })
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let l = length(it, a, name)?;
    // The optional second argument: a sampling flag, or a shape parameter.
    let max_args = if matches!(name, "rectwin" | "bartlett" | "triang" | "parzenwin" | "bohmanwin" | "barthannwin") { 1 } else { 2 };
    if a.len() > max_args {
        return rt("Too many input arguments.");
    }
    let w = if let Some(c) = cosine_coefficients(name) {
        let periodic = periodic_flag(a)?;
        padded(l, periodic, |n, m| {
            let x = TAU * n as f64 / (m - 1) as f64;
            c.iter().enumerate().map(|(k, &ck)| if k % 2 == 0 { ck } else { -ck } * (k as f64 * x).cos()).sum::<f64>()
        })
    } else {
        match name {
            "rectwin" => vec![1.0; l],
            "bartlett" => padded(l, false, |n, m| {
                let big_n = (m - 1) as f64;
                let n = n as f64;
                if n <= big_n / 2.0 { 2.0 * n / big_n } else { 2.0 - 2.0 * n / big_n }
            }),
            "triang" => triang(l),
            "kaiser" => {
                let beta = shape(a, 0.5, name, "beta", false)?;
                let i0b = bessel_i0(beta);
                padded(l, false, |n, m| {
                    let half = (m - 1) as f64 / 2.0;
                    let r = (n as f64 - half) / half;
                    bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0b
                })
            }
            "tukeywin" => tukey(l, shape(a, 0.5, name, "r", false)?),
            "gausswin" => {
                let alpha = shape(a, 2.5, name, "alpha", true)?;
                padded(l, false, |n, m| {
                    let half = (m - 1) as f64 / 2.0;
                    let x = alpha * (n as f64 - half) / half;
                    (-0.5 * x * x).exp()
                })
            }
            "parzenwin" => parzen(l),
            "chebwin" => chebwin(l, shape(a, 100.0, name, "r", true)?)?,
            "bohmanwin" => padded(l, false, |n, m| {
                if n == 0 || n == m - 1 {
                    return 0.0;
                }
                let x = (-1.0 + 2.0 * n as f64 / (m - 1) as f64).abs();
                (1.0 - x) * (PI * x).cos() + (PI * x).sin() / PI
            }),
            _ => padded(l, false, |n, m| {
                let x = n as f64 / (m - 1) as f64 - 0.5;
                0.62 - 0.48 * x.abs() + 0.38 * (TAU * x).cos()
            }),
        }
    };
    one(Value::Num(Matrix::col(w)))
}

/// The window length L: a nonnegative scalar, rounded to the nearest integer (with MATLAB's
/// warning when it was not one).
fn length(it: &mut Interpreter, a: &[Value], name: &str) -> R<usize> {
    let m = match &a[0] {
        Value::Num(m) if m.kind == Kind::Char => {
            return rt("Expected input number 1, N, to be one of these types:\n\ndouble, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64");
        }
        _ => m_arg(a, 0, name)?,
    };
    if m.numel() != 1 {
        return rt("Expected N to be a scalar.");
    }
    let v = m.data[0];
    if v.is_nan() || v.is_infinite() {
        return rt("Expected input number 1, N, to be finite.");
    }
    if v < 0.0 {
        return rt("Expected N to be nonnegative.");
    }
    if v.fract() != 0.0 {
        it.emit(crate::interp::OutputKind::Warning("Rounding order to nearest integer.".into()));
    }
    Ok(v.round() as usize)
}

/// `"periodic"` (true) or `"symmetric"` (false, the default) as the second argument.
fn periodic_flag(a: &[Value]) -> R<bool> {
    let Some(v) = a.get(1) else { return Ok(false) };
    let Ok(s) = v.text() else {
        return rt("Expected input to be one of these types:\n\nchar, string\n\nInstead its type was double.");
    };
    let low = s.to_lowercase();
    if !low.is_empty() && "periodic".starts_with(&low) {
        Ok(true)
    } else if !low.is_empty() && "symmetric".starts_with(&low) {
        Ok(false)
    } else {
        rt(format!(
            "Expected input number 2, sflag, to match one of these values:\n\n'symmetric', 'periodic'\n\nThe input, '{s}', did not match any of the valid values."
        ))
    }
}

/// The shape parameter (beta, r, alpha) as the optional second argument.
fn shape(a: &[Value], default: f64, name: &str, label: &str, nonneg: bool) -> R<f64> {
    if a.len() < 2 {
        return Ok(default);
    }
    if name == "kaiser" && matches!(&a[1], Value::Num(m) if m.kind == Kind::Char) {
        return rt("Input BTA in kaiser must be numeric, instead it was char.");
    }
    let m = m_arg(a, 1, name)?;
    if m.numel() != 1 {
        return rt(format!("Expected input number 2, {label}, to be a scalar."));
    }
    if m.im.is_some() || !m.data[0].is_finite() {
        return rt(format!("Expected input number 2, {label}, to be finite."));
    }
    if nonneg && m.data[0] < 0.0 {
        return rt(format!("Expected input number 2, {label}, to be nonnegative."));
    }
    Ok(m.data[0])
}

/// A window of length `l`: empty for 0, `[1]` for 1. Otherwise `f(n, m)` is the value at
/// index `n` of the symmetric window of length `m` (`m` is `l`, or `l + 1` for the periodic
/// form), computed for the first half and reflected.
fn padded(l: usize, periodic: bool, f: impl Fn(usize, usize) -> f64) -> Vec<f64> {
    if l == 0 {
        return Vec::new();
    }
    if l == 1 {
        return vec![1.0];
    }
    let m = if periodic { l + 1 } else { l };
    let half = m.div_ceil(2);
    let mut w: Vec<f64> = (0..half).map(|n| f(n, m)).collect();
    for n in half..m {
        w.push(w[m - 1 - n]);
    }
    w.truncate(l);
    w
}

fn triang(l: usize) -> Vec<f64> {
    if l == 0 {
        return Vec::new();
    }
    let half = l.div_ceil(2);
    let lf = l as f64;
    let mut w: Vec<f64> = (1..=half)
        .map(|n| {
            let n = n as f64;
            if l % 2 == 1 { 2.0 * n / (lf + 1.0) } else { (2.0 * n - 1.0) / lf }
        })
        .collect();
    for n in half..l {
        w.push(w[l - 1 - n]);
    }
    w
}

/// Tukey (tapered cosine) window: a rectangle for r <= 0, a Hann window for r >= 1.
fn tukey(l: usize, r: f64) -> Vec<f64> {
    if r <= 0.0 {
        return vec![1.0; l];
    }
    if r >= 1.0 {
        return padded(l, false, |n, m| 0.5 - 0.5 * (TAU * n as f64 / (m - 1) as f64).cos());
    }
    padded(l, false, |n, m| {
        let x = n as f64 / (m - 1) as f64;
        if x < r / 2.0 {
            0.5 * (1.0 + (TAU / r * (x - r / 2.0)).cos())
        } else if x < 1.0 - r / 2.0 {
            1.0
        } else {
            0.5 * (1.0 + (TAU / r * (x - 1.0 + r / 2.0)).cos())
        }
    })
}

fn parzen(l: usize) -> Vec<f64> {
    padded(l, false, |n, m| {
        let big_n = m as f64;
        let k = (n as f64 - (m - 1) as f64 / 2.0).abs();
        let a = k / (big_n / 2.0);
        if k <= (big_n - 1.0) / 4.0 { 1.0 - 6.0 * a * a + 6.0 * a * a * a } else { 2.0 * (1.0 - a).powi(3) }
    })
}

/// Modified Bessel function of the first kind, order 0, by its power series.
fn bessel_i0(x: f64) -> f64 {
    let q = x * x / 4.0;
    let (mut term, mut sum) = (1.0, 1.0);
    for k in 1..500 {
        term *= q / (k as f64 * k as f64);
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

/// Dolph-Chebyshev window with sidelobes `r` dB below the main lobe, scaled to a peak of 1.
/// Built from the inverse DFT of the Chebyshev polynomial sampled on the unit circle.
fn chebwin(l: usize, r: f64) -> R<Vec<f64>> {
    if l == 0 {
        return Ok(Vec::new());
    }
    if l == 1 {
        return Ok(vec![1.0]);
    }
    let order = (l - 1) as f64;
    let beta = ((10f64.powf(r / 20.0)).acosh() / order).cosh();
    let cheb = |x: f64| -> f64 {
        if x > 1.0 {
            (order * x.acosh()).cosh()
        } else if x < -1.0 {
            // T_order(-x) = (-1)^order T_order(x)
            let s = if l % 2 == 1 { 1.0 } else { -1.0 };
            s * (order * (-x).acosh()).cosh()
        } else {
            (order * x.acos()).cos()
        }
    };
    let p: Vec<f64> = (0..l).map(|k| cheb(beta * (PI * k as f64 / l as f64).cos())).collect();
    // Real part of the DFT of p (odd length) or of p rotated by exp(j pi k / l) (even length).
    let dft = |k: usize| -> f64 {
        (0..l)
            .map(|m| {
                let mut ang = -TAU * ((k * m) % l) as f64 / l as f64;
                if l % 2 == 0 {
                    ang += PI * m as f64 / l as f64;
                }
                p[m] * ang.cos()
            })
            .sum()
    };
    let spec: Vec<f64> = (0..l).map(dft).collect();
    let mut w: Vec<f64> = if l % 2 == 1 {
        let n = l.div_ceil(2);
        (1..n).rev().map(|i| spec[i]).chain(spec[..n].iter().copied()).collect()
    } else {
        let n = l / 2 + 1;
        (1..n).rev().map(|i| spec[i]).chain(spec[1..n].iter().copied()).collect()
    };
    let peak = w.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    for v in w.iter_mut() {
        *v /= peak;
    }
    Ok(w)
}
