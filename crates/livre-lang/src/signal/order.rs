//! Filter order selection: `buttord`, `cheb1ord`, `cheb2ord`, `ellipord`.
//!
//! Edges are prewarped for digital filters, the band cases are brought to a lowpass of
//! passband edge 1 whose stopband edge is `nat`, and the order and the cutoffs follow from the
//! lowpass formulas of each approximation.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::elliptic::{ellip_k, ellip_kp};
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["buttord", "cheb1ord", "cheb2ord", "ellipord"];

const PI: f64 = std::f64::consts::PI;

#[derive(Clone, Copy, PartialEq)]
enum Kind4 {
    Low,
    High,
    Pass,
    Stop,
}

struct Setup {
    kind: Kind4,
    /// Passband edges (prewarped, with the band-stop passband adjusted to be symmetric).
    wp: Vec<f64>,
    nat: f64,
}

fn numeric(a: &[Value], i: usize, label: &str, name: &str) -> R<Matrix> {
    match &a[i] {
        Value::Num(m) if m.kind != Kind::Char => Ok(m.clone()),
        other => rt(format!("Input {label} in {name} must be numeric, instead it was {}.", other.class_name())),
    }
}

fn setup(wp: &[f64], ws: &[f64], digital: bool) -> R<Setup> {
    let pre = |v: &[f64]| -> Vec<f64> { v.iter().map(|&w| if digital { (PI * w / 2.0).tan() } else { w }).collect() };
    let (p, s) = (pre(wp), pre(ws));
    if wp.len() == 1 {
        let low = wp[0] < ws[0];
        let nat = if low { s[0] / p[0] } else { p[0] / s[0] };
        return Ok(Setup { kind: if low { Kind4::Low } else { Kind4::High }, wp: p, nat });
    }
    let pass = ws[0] < wp[0];
    let ok = if pass { ws[0] < wp[0] && wp[0] < wp[1] && wp[1] < ws[1] } else { wp[0] < ws[0] && ws[0] < ws[1] && ws[1] < wp[1] };
    if !ok {
        return rt("The passband and stopband cutoff frequencies should not overlap.");
    }
    let (mut p, mut s) = (p, s);
    if pass {
        if p[0] * p[1] > s[0] * s[1] {
            s[0] = p[0] * p[1] / s[1];
        } else {
            s[1] = p[0] * p[1] / s[0];
        }
    } else if p[0] * p[1] < s[0] * s[1] {
        p[0] = s[0] * s[1] / p[1];
    } else {
        p[1] = s[0] * s[1] / p[0];
    }
    let bw = p[1] - p[0];
    let wo2 = p[0] * p[1];
    let nat = if pass {
        s.iter().map(|&w| ((w * w - wo2) / (bw * w)).abs()).fold(f64::INFINITY, f64::min)
    } else {
        s.iter().map(|&w| (bw * w / (wo2 - w * w)).abs()).fold(f64::INFINITY, f64::min)
    };
    Ok(Setup { kind: if pass { Kind4::Pass } else { Kind4::Stop }, wp: p, nat })
}

/// Band edges (or the single cutoff) whose lowpass equivalent is the cutoff `oc` of a lowpass with
/// passband edge 1.
fn from_lowpass(st: &Setup, oc: f64, digital: bool) -> Vec<f64> {
    let w: Vec<f64> = match st.kind {
        Kind4::Low => vec![st.wp[0] * oc],
        Kind4::High => vec![st.wp[0] / oc],
        Kind4::Pass | Kind4::Stop => {
            let bw = st.wp[1] - st.wp[0];
            let wo2 = st.wp[0] * st.wp[1];
            let h = if st.kind == Kind4::Pass { bw * oc / 2.0 } else { bw / (2.0 * oc) };
            let r = (h * h + wo2).sqrt();
            if st.kind == Kind4::Pass { vec![r - h, r + h] } else { vec![r - h, r + h] }
        }
    };
    if digital { w.iter().map(|&x| 2.0 / PI * x.atan()).collect() } else { w }
}

fn row(v: &[f64]) -> Value {
    Value::Num(Matrix::from_dims(vec![1, v.len()], v.to_vec()))
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 4 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 5 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let wpm = numeric(a, 0, "Wp", name)?;
    let wsm = numeric(a, 1, "Ws", name)?;
    let rpm = numeric(a, 2, "Rp", name)?;
    let rsm = numeric(a, 3, "Rs", name)?;
    let mut digital = true;
    if a.len() == 5 {
        match a[4].text().ok().as_deref() {
            Some("s") => digital = false,
            Some("z") => digital = true,
            _ => return rt("Invalid option for final argument."),
        }
    }
    let wp = wpm.data.clone();
    let ws = wsm.data.clone();
    if wp.len() != ws.len() {
        return rt("The frequency vectors must both be the same length.");
    }
    if digital {
        if wp.iter().chain(&ws).any(|&w| w <= 0.0 || w >= 1.0) {
            return rt("The cutoff frequencies must be within the interval of (0,1).");
        }
    } else if wp.iter().chain(&ws).any(|&w| w <= 0.0) {
        return rt("The cutoff frequencies must be non-negative for analog filters.");
    }
    if rpm.numel() != 1 || rsm.numel() != 1 {
        return rt(match name {
            "buttord" => "Matrix dimensions must agree.",
            _ => "Incorrect dimensions for raising a matrix to a power. Check that the matrix is square and the power is a scalar. To operate on each element of the matrix individually, use POWER (.^) for elementwise power.",
        });
    }
    if name == "ellipord" && (rpm.im.is_some() || rsm.im.is_some()) {
        return rt("Input arguments must be real.");
    }
    let (rp, rs) = if name == "ellipord" { (rpm.data[0], rsm.data[0]) } else { (rpm.data[0].abs(), rsm.data[0].abs()) };
    if wp.len() > 2 {
        return rt("The frequency vectors must both be the same length.");
    }
    let st = setup(&wp, &ws, digital)?;
    let ratio = (10f64.powf(0.1 * rs) - 1.0) / (10f64.powf(0.1 * rp) - 1.0);
    let nat = st.nat;
    let mut n_im = 0.0;
    let n;
    let wn: Vec<f64>;
    match name {
        "buttord" => {
            n = (ratio.log10() / (2.0 * nat.log10())).ceil();
            let oc = nat / (10f64.powf(0.1 * rs) - 1.0).powf(1.0 / (2.0 * n));
            wn = from_lowpass(&st, oc, digital);
        }
        "cheb1ord" | "cheb2ord" => {
            let den = nat.acosh();
            if ratio >= 1.0 {
                n = (ratio.sqrt().acosh() / den).ceil();
            } else {
                n = 0.0;
                n_im = (ratio.sqrt().acos() / den).ceil();
            }
            if name == "cheb1ord" {
                wn = if digital { wp.clone() } else { st.wp.clone() };
            } else if digital {
                wn = ws.clone();
            } else {
                let os = (ratio.sqrt().acosh() / n).cosh();
                wn = from_lowpass(&st, os, false);
            }
        }
        _ => {
            let k1sq = (10f64.powf(0.1 * rp) - 1.0) / (10f64.powf(0.1 * rs) - 1.0);
            let k1 = k1sq.sqrt();
            if !(0.0..=1.0).contains(&k1) || k1.is_nan() {
                return rt("M must be in the range 0 <= M <= 1.");
            }
            let k = 1.0 / nat;
            let kk = ellip_k(k1);
            n = if k1 == 1.0 { 0.0 } else { (ellip_k(k) * ellip_kp(k1) / (ellip_kp(k) * kk)).ceil() };
            wn = wp.clone();
        }
    }
    let nv = if n_im != 0.0 {
        Value::Num(Matrix::complex(vec![1, 1], vec![n], vec![n_im]))
    } else {
        Value::Num(Matrix::scalar(n))
    };
    let mut outv = vec![nv, row(&wn)];
    outv.truncate(nargout.max(1));
    out(outv)
}
