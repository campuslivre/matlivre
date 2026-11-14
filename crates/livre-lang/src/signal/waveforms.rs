//! Waveforms: `square`, `sawtooth`, `rectpuls`, `diric`, `chirp`, `gmonopuls`.

use crate::builtins::{m_arg, one, Ret};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::signal::poly::{scalar_arg, text_arg};
use crate::value::*;

pub const NAMES: &[&str] = &["square", "sawtooth", "rectpuls", "diric", "chirp", "gmonopuls", "tripuls", "gauspuls"];

type R<T> = Result<T, LangError>;

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    match name {
        "tripuls" => return tripuls(a),
        "gauspuls" => return gauspuls(a, nargout),
        "diric" => return diric(a),
        "chirp" => return chirp(a),
        "gmonopuls" => return gmonopuls(a),
        _ => {}
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let t = m_arg(a, 0, name)?;
    if t.im.is_some() {
        return rt(format!("Expected input number 1, {}, to be real.", "t"));
    }
    // The optional scalar argument: duty cycle, width.
    let label = match name {
        "square" => "duty",
        "sawtooth" => "width",
        _ => "w",
    };
    let opt = |default: f64| -> R<f64> {
        if a.len() < 2 {
            return Ok(default);
        }
        let m = m_arg(a, 1, name)?;
        if m.numel() != 1 {
            return rt(format!("Expected input number 2, {label}, to be a scalar."));
        }
        Ok(m.data[0])
    };
    let tau = std::f64::consts::TAU;
    let y: Vec<f64> = match name {
        // A positive value over the first `duty` percent of each 2*pi period, -1 over the rest;
        // the wave never takes the value 0.
        "square" => {
            let duty = opt(50.0)?;
            t.data.iter().map(|&v| if period_pos(v) < duty / 100.0 * tau { 1.0 } else { -1.0 }).collect()
        }
        // Rises from -1 to 1 over the first `width` of each period and falls back over the rest.
        "sawtooth" => {
            let width = opt(1.0)?;
            if width < 0.0 {
                return rt("Expected input number 2, width, to be a scalar with value >= 0.");
            }
            if width > 1.0 {
                return rt("Expected input number 2, width, to be a scalar with value <= 1.");
            }
            t.data
                .iter()
                .map(|&v| {
                    let p = period_pos(v);
                    let peak = width * tau;
                    if p < peak {
                        -1.0 + 2.0 * p / peak
                    } else {
                        1.0 - 2.0 * (p - peak) / (tau - peak)
                    }
                })
                .collect()
        }
        // Unit-height pulse of width `w` centred on 0, open on the right.
        _ => {
            let w = opt(1.0)?;
            t.data.iter().map(|&v| if v.abs() < w / 2.0 || v == -w / 2.0 { 1.0 } else { 0.0 }).collect()
        }
    };
    one(Value::Num(Matrix::from_dims(t.dims.clone(), y)))
}

const TIME_TYPES: &str = "double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64";

fn scalar_option(a: &[Value], i: usize, name: &str, label: &str, default: f64) -> R<f64> {
    if a.len() <= i {
        return Ok(default);
    }
    let m = m_arg(a, i, name)?;
    if m.numel() != 1 {
        return rt(format!("Expected {label} to be a scalar."));
    }
    Ok(m.data[0])
}

/// Unit-height triangular pulse of width `w` centred on 0 with its apex at `s * w / 2`.
fn tripuls(a: &[Value]) -> Ret {
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    let t = m_arg(a, 0, "tripuls")?;
    if t.kind == Kind::Char {
        return rt(format!("Expected input to be one of these types:  {TIME_TYPES}"));
    }
    if t.im.is_some() {
        return rt("Expected input to be real.");
    }
    let opt = |i: usize, default: f64| -> R<f64> {
        if a.len() <= i {
            return Ok(default);
        }
        let m = m_arg(a, i, "tripuls")?;
        if m.numel() != 1 {
            return rt("Expected input to be a scalar.");
        }
        Ok(m.data[0])
    };
    let w = opt(1, 1.0)?;
    let s = opt(2, 0.0)?;
    if !(-1.0..=1.0).contains(&s) {
        return rt("Skew value must be in the range -1<=k<=1.");
    }
    let half = w.abs() / 2.0;
    let apex = s * half;
    let y = t
        .data
        .iter()
        .map(|&v| {
            if half == 0.0 {
                if v == 0.0 { 1.0 } else { 0.0 }
            } else if v < -half || v > half {
                0.0
            } else if v < apex {
                (v + half) / (half * (1.0 + s))
            } else if v == apex {
                1.0
            } else {
                (half - v) / (half * (1.0 - s))
            }
        })
        .collect();
    one(Value::Num(Matrix::from_dims(t.dims.clone(), y)))
}

/// Gaussian-modulated sinusoid: `[yi, yq, ye] = gauspuls(t, fc, bw, bwr)` and the cutoff time
/// `gauspuls('cutoff', fc, bw, bwr, tpe)`.
fn gauspuls(a: &[Value], nargout: usize) -> Ret {
    let cutoff = match &a[0] {
        Value::Str(_) => true,
        Value::Num(m) => m.kind == Kind::Char,
        _ => false,
    };
    if a.len() > if cutoff { 5 } else { 4 } {
        return rt("Too many input arguments.");
    }
    if cutoff && a[0].text().ok().map(|s| s.to_lowercase()).as_deref() != Some("cutoff") {
        return rt("When first input is a character vector, its value must be 'cutoff'.");
    }
    let fc = scalar_option(a, 1, "gauspuls", "FC", 1000.0)?;
    let bw = scalar_option(a, 2, "gauspuls", "BW", 0.5)?;
    let bwr = scalar_option(a, 3, "gauspuls", "BWR", -6.0)?;
    let tpe = if cutoff { scalar_option(a, 4, "gauspuls", "TPE", -60.0)? } else { -60.0 };
    if fc < 0.0 {
        return rt("Frequency, FC, must be >= 0 Hz.");
    }
    if bw <= 0.0 {
        return rt("Fractional bandwidth, BW, must be > 0.");
    }
    if bwr >= 0.0 || tpe >= 0.0 {
        return rt("Bandwidth reference level BWR must be < 0 dB.");
    }
    let ref_level = 10f64.powf(bwr / 20.0);
    let alpha = -(std::f64::consts::PI * fc * bw).powi(2) / (4.0 * ref_level.ln());
    if cutoff {
        let tpe_level = 10f64.powf(tpe / 20.0);
        return one(Value::Num(Matrix::scalar((-tpe_level.ln() / alpha).sqrt())));
    }
    let t = m_arg(a, 0, "gauspuls")?;
    if t.im.is_some() {
        return rt("Expected input to be real.");
    }
    let env: Vec<f64> = t.data.iter().map(|&v| (-alpha * v * v).exp()).collect();
    let phase = |v: f64| std::f64::consts::TAU * fc * v;
    let make = |v: Vec<f64>| Value::Num(Matrix::from_dims(t.dims.clone(), v));
    let yi = make(t.data.iter().zip(&env).map(|(&v, &e)| e * phase(v).cos()).collect());
    if nargout <= 1 {
        return one(yi);
    }
    let yq = make(t.data.iter().zip(&env).map(|(&v, &e)| e * phase(v).sin()).collect());
    Ok(Some(vec![yi, yq, make(env)]))
}

/// Position of `t` within its 2*pi period, in [0, 2*pi).
fn period_pos(t: f64) -> f64 {
    let p = t.rem_euclid(std::f64::consts::TAU);
    if p.is_nan() { f64::NAN } else { p }
}

/// Dirichlet function sin(n x / 2) / (n sin(x / 2)), with its limit at multiples of 2 pi.
fn diric(a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let x = m_arg(a, 0, "diric")?;
    if x.im.is_some() {
        return rt("Expected input number 1, x, to be real.");
    }
    let nm = m_arg(a, 1, "diric")?;
    if nm.numel() != 1 {
        return rt("Expected input number 2, N, to be a scalar.");
    }
    let n = nm.data[0];
    if n < 1.0 {
        return rt("Expected input number 2, N, to be a scalar with value >= 1.");
    }
    if n.fract() != 0.0 {
        return rt("Expected input number 2, N, to be integer-valued.");
    }
    let tau = std::f64::consts::TAU;
    let y = x
        .data
        .iter()
        .map(|&v| {
            let m = (v / tau).round();
            if (v - m * tau).abs() < 1e-13 * (1.0 + v.abs()) {
                // Limit at x = 2 pi m is (-1)^(m (n - 1)).
                if (m * (n - 1.0)) % 2.0 == 0.0 { 1.0 } else { -1.0 }
            } else {
                (n * v / 2.0).sin() / (n * (v / 2.0).sin())
            }
        })
        .collect();
    one(Value::Num(Matrix::from_dims(x.dims.clone(), y)))
}

/// Swept-frequency cosine: chirp(t, f0, t1, f1 [, method [, phi [, shape]]] [, 'complex']).
/// The defaults are f0 = 0 (1e-6 for the logarithmic method), t1 = 1, f1 = 100 and phi = 0
/// degrees. The phase is 2 pi times the integral of the documented instantaneous-frequency law.
///
/// For the quadratic method the default shape is `f0 + beta t^2` (concave for an upsweep, convex
/// for a downsweep); the other shape is the time-reversed law about the last sample of `t`.
fn chirp(a: &[Value]) -> Ret {
    if a.len() > 8 {
        return rt("Too many input arguments.");
    }
    let t = m_arg(a, 0, "chirp")?;
    if t.im.is_some() {
        return rt("Expected input number 1, t, to be real.");
    }
    let opt = |i: usize, default: f64| -> R<f64> {
        match a.get(i) {
            Some(v) if !matches!(v, Value::Num(m) if m.numel() == 0) => scalar_arg(a, i, "chirp"),
            _ => Ok(default),
        }
    };
    let method = match text_arg(a, 4) {
        Some(s) => s,
        None => "linear".to_string(),
    };
    let kind = if !method.is_empty() && "linear".starts_with(&method) {
        0
    } else if !method.is_empty() && "quadratic".starts_with(&method) {
        1
    } else if !method.is_empty() && "logarithmic".starts_with(&method) {
        2
    } else {
        return rt(format!(
            "Expected input to match one of these values:\n\n'linear', 'quadratic', 'logarithmic'\n\nThe input, '{method}', did not match any of the valid values."
        ));
    };
    let f0 = opt(1, if kind == 2 { 1e-6 } else { 0.0 })?;
    let t1 = opt(2, 1.0)?;
    let f1 = opt(3, 100.0)?;
    let phi = opt(5, 0.0)?;
    let (mut shape, mut complex) = (None::<String>, false);
    for i in 5..a.len() {
        match text_arg(a, i).as_deref() {
            Some("complex") => complex = true,
            Some("convex") => shape = Some("convex".into()),
            Some("concave") => shape = Some("concave".into()),
            Some(other) => return rt(format!("Unknown option '{other}'.")),
            None => {}
        }
    }
    if t1 <= 0.0 {
        return rt("Expected T1 to be positive.");
    }
    if kind == 2 && f0 < 1e-6 {
        return rt("F0 must be greater than or equal to 1e-6 for a log-sweep.");
    }
    let tau = std::f64::consts::TAU;
    let beta = (f1 - f0) / (t1 * t1);
    let tmax = t.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let default_shape = if f1 > f0 { "concave" } else { "convex" };
    let reversed = kind == 1 && shape.as_deref().is_some_and(|s| s != default_shape);
    let phase = |x: f64| -> f64 {
        let base = match kind {
            0 => tau * (f0 * x + (f1 - f0) / (2.0 * t1) * x * x),
            1 if reversed => {
                let s = tmax - x;
                -tau * (f1 * s - beta * s * s * s / 3.0)
            }
            1 => tau * (f0 * x + beta * x * x * x / 3.0),
            _ => {
                let b = (f1 / f0).powf(1.0 / t1);
                tau * f0 * (b.powf(x) - 1.0) / b.ln()
            }
        };
        base + phi.to_radians()
    };
    if complex {
        let (re, im): (Vec<f64>, Vec<f64>) = t.data.iter().map(|&v| (phase(v).cos(), phase(v).sin())).unzip();
        one(Value::Num(Matrix::complex(t.dims.clone(), re, im)))
    } else {
        one(Value::Num(Matrix::from_dims(t.dims.clone(), t.data.iter().map(|&v| phase(v).cos()).collect())))
    }
}

/// Unit-amplitude Gaussian monopulse sqrt(e) (t / s) exp(-(t / s)^2 / 2) with s = 1 / (2 pi fc);
/// the default centre frequency is 1 kHz and the peak value 1 is reached at t = s.
fn gmonopuls(a: &[Value]) -> Ret {
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let t = m_arg(a, 0, "gmonopuls")?;
    if t.im.is_some() {
        return rt("Expected input number 1, t, to be real.");
    }
    let fc = if a.len() == 2 { scalar_arg(a, 1, "gmonopuls")? } else { 1000.0 };
    if fc < 0.0 {
        return rt("Expected input number 2, fc, to be nonnegative.");
    }
    let sigma = 1.0 / (2.0 * std::f64::consts::PI * fc);
    let y = t.data.iter().map(|&v| std::f64::consts::E.sqrt() * (v / sigma) * (-(v / sigma) * (v / sigma) / 2.0).exp()).collect();
    one(Value::Num(Matrix::from_dims(t.dims.clone(), y)))
}
