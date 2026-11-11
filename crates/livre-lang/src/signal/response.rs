//! Frequency responses: `freqz`, `grpdelay`, `phasez` (digital) and `freqs` (analog).
//!
//! All need outputs; with none MATLAB plots the response, which is not supported here. When the
//! frequencies are given as a vector the results take that vector's orientation, otherwise they
//! are columns.

use crate::builtins::Ret;
use crate::cplx::{poly_roots, C};
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["freqz", "freqs", "grpdelay", "phasez"];

const PI: f64 = std::f64::consts::PI;

fn shaped_complex(h: &[C], row: bool) -> Value {
    let dims = if row { vec![1, h.len()] } else { vec![h.len(), 1] };
    Value::Num(Matrix::complex(dims, h.iter().map(|c| c.re).collect(), h.iter().map(|c| c.im).collect()))
}

fn shaped_real(v: Vec<f64>, row: bool) -> Value {
    if row { real_row(v) } else { real_col(v) }
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout == 0 {
        return rt(format!("{name} without output arguments plots the response, which is not supported; request outputs."));
    }
    match name {
        "freqs" => freqs(a),
        "freqz" => freqz(a, nargout),
        _ => digital_measure(name, a, nargout),
    }
}

/// Numerator, denominator and the index of the first frequency-related argument.
fn filter_args(a: &[Value], name: &str) -> R<(Vec<C>, Vec<C>, usize)> {
    let b = cvec_arg(a, 0, name)?;
    let mut rest = 1;
    // The second argument is the denominator unless it is a flag.
    let den = if a.len() > 1 && text_arg(a, 1).is_none() {
        rest = 2;
        cvec_arg(a, 1, name)?
    } else {
        vec![C::new(1.0, 0.0)]
    };
    if b.is_empty() || den.is_empty() {
        return rt("Expected numerator and denominator to be nonempty.");
    }
    Ok((b, den, rest))
}

struct Grid {
    omega: Vec<f64>,
    reported: Vec<f64>,
    row: bool,
}

/// Frequencies in rad/sample and the vector reported back, from `[n | w] [fs] ['whole']`.
fn frequency_grid(a: &[Value], rest: usize, name: &str) -> R<Grid> {
    let (mut nw, mut fs, mut whole) = (None::<(Vec<f64>, bool)>, None::<f64>, false);
    for i in rest..a.len() {
        match text_arg(a, i) {
            Some(s) if s == "whole" => whole = true,
            Some(s) if s == "half" => whole = false,
            Some(_) => return rt("Unrecognized string input, valid inputs are \"whole\" and \"half\"."),
            None => {
                let m = crate::builtins::m_arg(a, i, name)?;
                let v = rvec_arg(a, i, name)?;
                if nw.is_none() {
                    // An empty argument means the default number of points.
                    nw = Some((v, m.dims[0] == 1));
                } else if v.len() == 1 && fs.is_none() {
                    fs = Some(v[0]);
                } else if v.is_empty() && fs.is_none() {
                    fs = None;
                } else {
                    return rt("Too many numeric arguments.");
                }
            }
        }
    }
    if fs.is_some_and(|f| f <= 0.0) {
        return rt("Expected Fs to be positive.");
    }
    Ok(match nw {
        Some((v, row)) if v.len() > 1 => {
            let om: Vec<f64> = v.iter().map(|&f| fs.map_or(f, |fs| 2.0 * PI * f / fs)).collect();
            Grid { omega: om, reported: v, row }
        }
        other => {
            let n = match other {
                Some((v, _)) if v.len() == 1 => v[0],
                _ => 512.0,
            };
            if n < 1.0 || n.fract() != 0.0 {
                return rt(if name == "grpdelay" { "NaN and Inf not allowed." } else { "Array indices must be positive integers or logical values." });
            }
            let n = n as usize;
            let span = if whole { 2.0 * PI } else { PI };
            let om: Vec<f64> = (0..n).map(|i| span * i as f64 / n as f64).collect();
            let rep = om.iter().map(|&w| fs.map_or(w, |fs| w * fs / (2.0 * PI))).collect();
            Grid { omega: om, reported: rep, row: false }
        }
    })
}

/// B(e^{jw}) for coefficients in powers of z^-1.
fn eval_z(coef: &[C], w: f64) -> C {
    let x = C::new(w.cos(), -w.sin());
    let rev: Vec<C> = coef.iter().rev().copied().collect();
    polyval_c(&rev, x)
}

fn freqz(a: &[Value], nargout: usize) -> Ret {
    let (b, den, rest) = filter_args(a, "freqz")?;
    let g = frequency_grid(a, rest, "freqz")?;
    let h: Vec<C> = g.omega.iter().map(|&w| eval_z(&b, w).div(eval_z(&den, w))).collect();
    let mut outs = vec![shaped_complex(&h, g.row)];
    if nargout >= 2 {
        outs.push(shaped_real(g.reported, g.row));
    }
    out(outs)
}

/// Group delay (samples) for the frequency `w`, or `None` at a singular point.
fn group_delay_at(c: &[C], cr: &[C], shift: f64, w: f64) -> Option<f64> {
    let d = eval_z(c, w);
    if d.abs() < 1e-13 { None } else { Some(eval_z(cr, w).div(d).re - shift) }
}

/// Phase of the response on a fine path through the requested frequencies, unwrapped along the
/// path so that jumps larger than pi between its points are removed (NaN where the response is
/// zero).
fn continuous_phase(omega: &[f64], b: &[C], den: &[C]) -> Vec<f64> {
    let scale: f64 = b.iter().map(|c| c.abs()).sum::<f64>().max(1.0);
    let raw = |w: f64| -> f64 {
        let h = eval_z(b, w).div(eval_z(den, w));
        if h.abs() <= 1e-12 * scale { f64::NAN } else { h.arg() }
    };
    let mut result = Vec::with_capacity(omega.len());
    let (mut last_raw, mut offset) = (None::<f64>, 0.0);
    let advance = |r: f64, last_raw: &mut Option<f64>, offset: &mut f64| {
        if r.is_nan() {
            return;
        }
        if let Some(prev) = *last_raw {
            let d = r - prev;
            if d > PI {
                *offset -= 2.0 * PI * ((d - PI) / (2.0 * PI)).ceil();
            } else if d < -PI {
                *offset += 2.0 * PI * ((-d - PI) / (2.0 * PI)).ceil();
            }
        }
        *last_raw = Some(r);
    };
    for (i, &w) in omega.iter().enumerate() {
        if i > 0 {
            let from = omega[i - 1];
            let steps = (((w - from).abs() / (PI / 8.0)).ceil() as usize).max(1);
            for k in 1..steps {
                let r = raw(from + (w - from) * k as f64 / steps as f64);
                advance(r, &mut last_raw, &mut offset);
            }
        }
        let r = raw(w);
        advance(r, &mut last_raw, &mut offset);
        result.push(if r.is_nan() { f64::NAN } else { r + offset });
    }
    result
}

/// `grpdelay` (in samples) and `phasez` (unwrapped, in radians); both return columns.
fn digital_measure(name: &str, a: &[Value], nargout: usize) -> Ret {
    let (b, den, rest) = filter_args(a, name)?;
    let g = frequency_grid(a, rest, name)?;
    let values: Vec<f64> = if name == "grpdelay" {
        // c = conv(b, fliplr(conj(a))); gd = Re(sum k c_k e^{-jw k} / sum c_k e^{-jw k}) - (len(a) - 1).
        let rev_conj: Vec<C> = den.iter().rev().map(|c| c.conj()).collect();
        let mut c = vec![C::new(0.0, 0.0); b.len() + rev_conj.len() - 1];
        for (i, &x) in b.iter().enumerate() {
            for (j, &y) in rev_conj.iter().enumerate() {
                c[i + j] = c[i + j].add(x.mul(y));
            }
        }
        let cr: Vec<C> = c.iter().enumerate().map(|(k, v)| C::new(v.re * k as f64, v.im * k as f64)).collect();
        let shift = (den.len() - 1) as f64;
        let raw: Vec<Option<f64>> = g.omega.iter().map(|&w| group_delay_at(&c, &cr, shift, w)).collect();
        // At a zero of the response take the mean of the nearest regular neighbours.
        (0..raw.len())
            .map(|i| {
                raw[i].unwrap_or_else(|| {
                    let left = (0..i).rev().find_map(|j| raw[j]);
                    let right = (i + 1..raw.len()).find_map(|j| raw[j]);
                    match (left, right) {
                        (Some(l), Some(r)) => (l + r) / 2.0,
                        (Some(v), None) | (None, Some(v)) => v,
                        _ => 0.0,
                    }
                })
            })
            .collect()
    } else {
        continuous_phase(&g.omega, &b, &den)
    };
    let mut outs = vec![real_col(values)];
    if nargout >= 2 {
        outs.push(real_col(g.reported));
    }
    out(outs)
}

fn freqs(a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    let b = cvec_arg(a, 0, "freqs")?;
    let den = cvec_arg(a, 1, "freqs")?;
    if b.is_empty() || den.is_empty() {
        return rt("Expected numerator and denominator to be nonempty.");
    }
    let (w, row): (Vec<f64>, bool) = match a.get(2) {
        Some(v) if !matches!(v, Value::Num(m) if m.numel() == 0) => {
            let m = crate::builtins::m_arg(a, 2, "freqs")?;
            let v = rvec_arg(a, 2, "freqs")?;
            if v.len() > 1 {
                (v, m.dims[0] == 1)
            } else {
                (auto_frequencies(&b, &den, positive_integer(v[0])?), false)
            }
        }
        _ => (auto_frequencies(&b, &den, 200), false),
    };
    let h: Vec<C> = w
        .iter()
        .map(|&f| {
            let s = C::new(0.0, f);
            polyval_c(&b, s).div(polyval_c(&den, s))
        })
        .collect();
    out(vec![shaped_complex(&h, row), shaped_real(w, row)])
}

fn positive_integer(v: f64) -> R<usize> {
    if v <= 0.0 {
        return rt("Expected N to be positive.");
    }
    if v.fract() != 0.0 {
        return rt("Expected N to be integer-valued.");
    }
    Ok(v as usize)
}

/// `n` log-spaced frequencies covering the roots of the numerator and denominator: from
/// 10^(ceil(log10(min |root|)) - 2) to 10^(floor(log10(max |root|)) + 1).
fn auto_frequencies(b: &[C], a: &[C], n: usize) -> Vec<f64> {
    let mags: Vec<f64> = poly_roots(b).into_iter().chain(poly_roots(a)).map(|r| r.abs()).filter(|&m| m > 0.0).collect();
    let (lo, hi) = if mags.is_empty() {
        (-2.0, 1.0)
    } else {
        let min = mags.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = mags.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        (min.log10().ceil() - 2.0, max.log10().floor() + 1.0)
    };
    (0..n).map(|i| 10f64.powf(if n == 1 { lo } else { lo + (hi - lo) * i as f64 / (n - 1) as f64 })).collect()
}
