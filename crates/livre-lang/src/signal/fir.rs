//! Window-method FIR design: `fir1`.

use crate::builtins::{m_arg, Ret};
use crate::error::rt;
use crate::interp::{Interpreter, OutputKind};
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["fir1"];

const PI: f64 = std::f64::consts::PI;

fn sinc(x: f64) -> f64 {
    if x == 0.0 { 1.0 } else { (PI * x).sin() / (PI * x) }
}

/// Symmetric Hamming window of length `n + 1`.
fn hamming(n: usize) -> Vec<f64> {
    let mut w = vec![0.0; n + 1];
    for k in 0..=n / 2 {
        let v = 0.54 - 0.46 * (2.0 * PI * k as f64 / n as f64).cos();
        w[k] = v;
        w[n - k] = v;
    }
    w
}

pub fn call(it: &mut Interpreter, _name: &str, a: &[Value], _nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 6 {
        return rt("Too many input arguments.");
    }
    let nm = m_arg(a, 0, "fir1")?;
    if nm.numel() != 1 || nm.data[0] <= 0.0 {
        return rt("Expected input number 1, n, to be positive.");
    }
    if nm.data[0].fract() != 0.0 {
        return rt("Filter order must be a real, positive integer.");
    }
    let mut n = nm.data[0] as usize;
    let wn = rvec_arg(a, 1, "fir1")?;
    if wn.is_empty() {
        return rt("Expected input number 2, Wn, to be nonempty.");
    }
    if wn.iter().any(|&w| w <= 0.0) && wn.len() == 1 {
        return rt("Expected input number 2, Wn, to be positive.");
    }
    if wn.iter().any(|&w| w < 0.0 || w > 1.0) || (wn.len() == 1 && wn[0] >= 1.0) || (wn.len() > 1 && (wn[0] <= 0.0 || wn[wn.len() - 1] >= 1.0)) {
        return rt("Frequencies must fall in range between 0 and 1.");
    }
    if wn.windows(2).any(|p| p[0] > p[1]) {
        return rt("Frequencies must be increasing.");
    }

    // Band type, window and scaling from the remaining arguments.
    let mut dc1: Option<bool> = None;
    let mut high = false;
    let mut stop = false;
    let mut window: Option<Vec<f64>> = None;
    let mut scale = true;
    for i in 2..a.len() {
        match text_arg(a, i) {
            Some(s) => match s.as_str() {
                "scale" => scale = true,
                "noscale" => scale = false,
                "low" => dc1 = Some(true),
                "high" => {
                    dc1 = Some(false);
                    high = true;
                }
                "bandpass" => dc1 = Some(false),
                "stop" => {
                    dc1 = Some(true);
                    stop = true;
                }
                "dc-0" => dc1 = Some(false),
                "dc-1" => dc1 = Some(true),
                _ => {
                    let shown = match &a[i] {
                        Value::Num(_) => a[i].text().unwrap_or_default(),
                        _ => s.clone(),
                    };
                    return rt(format!(
                        "Expected input to match one of these values:\n\n'LOW', 'HIGH', 'BANDPASS', 'STOP', 'DC-0', 'DC-1', 'NOSCALE', 'SCALE'\n\nThe input, '{shown}', did not match any of the valid values."
                    ));
                }
            },
            None => window = Some(rvec_arg(a, i, "fir1")?),
        }
    }
    let _ = (high, stop);
    // Passbands as (low, high) pairs between the edges 0, wn..., 1.
    let mut edges = vec![0.0];
    edges.extend(&wn);
    edges.push(1.0);
    let dc1 = dc1.unwrap_or(match wn.len() {
        1 => true,
        2 => false,
        _ => false,
    });
    let bands: Vec<(f64, f64)> = (0..edges.len() - 1).filter(|i| (i % 2 == 0) == dc1).map(|i| (edges[i], edges[i + 1])).collect();
    if n % 2 == 1 && bands.last().is_some_and(|b| b.1 == 1.0) {
        it.emit(OutputKind::Warning("Odd order symmetric FIR filters must have a gain of zero at the Nyquist frequency. The order is being increased by one.".into()));
        n += 1;
    }

    let w = match window {
        Some(w) if w.len() == n + 1 => w,
        Some(_) => return rt("Window length must be the same as the filter length."),
        None => hamming(n),
    };
    let half = n as f64 / 2.0;
    let mut b: Vec<f64> = (0..=n)
        .map(|k| {
            let m = k as f64 - half;
            bands.iter().map(|&(lo, hi)| hi * sinc(hi * m) - lo * sinc(lo * m)).sum::<f64>() * w[k]
        })
        .collect();

    if scale {
        let (lo, hi) = bands[0];
        let f = if lo == 0.0 {
            0.0
        } else if hi == 1.0 {
            1.0
        } else {
            (lo + hi) / 2.0
        };
        let (re, im) = b.iter().enumerate().fold((0.0, 0.0), |(re, im), (k, &v)| {
            let ang = -PI * f * k as f64;
            (re + v * ang.cos(), im + v * ang.sin())
        });
        let gain = re.hypot(im);
        for v in b.iter_mut() {
            *v /= gain;
        }
    }
    out(vec![real_row(b)])
}
