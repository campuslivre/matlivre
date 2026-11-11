//! IIR filter design from analog prototypes: `butter`, `cheby1`, `cheby2`.
//!
//! The steps follow the documented design procedure: an analog lowpass prototype, a
//! frequency transformation to the requested band, and (for digital filters) the bilinear
//! transform with frequencies pre-warped for a sample rate of 2 (normalized frequency 1 is the
//! Nyquist frequency). Analog designs return the numerator padded with leading zeros to the
//! length of the denominator.

use crate::builtins::{m_arg, Ret};
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::analog::*;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["butter", "cheby1", "cheby2"];

#[derive(Clone, Copy, PartialEq)]
enum Band {
    Low,
    High,
    Pass,
    Stop,
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    let ripple = name != "butter";
    let fixed = if ripple { 3 } else { 2 };
    if a.len() < fixed {
        return rt("Not enough input arguments.");
    }
    if a.len() > fixed + 2 {
        return rt("Too many input arguments.");
    }
    let nm = m_arg(a, 0, name)?;
    if nm.numel() != 1 {
        return rt("Expected N to be a scalar.");
    }
    if nm.data[0] <= 0.0 {
        return rt("Expected N to be positive.");
    }
    if nm.data[0].fract() != 0.0 {
        return rt("Expected N to be integer-valued.");
    }
    let n = nm.data[0] as usize;
    let r = if ripple { scalar_arg(a, 1, name)? } else { 0.0 };
    if ripple && r < 0.0 {
        return rt(if name == "cheby1" { "Expected Rp to be nonnegative." } else { "Expected Rs to be nonnegative." });
    }
    let wn = rvec_arg(a, fixed - 1, name)?;
    if wn.is_empty() || wn.len() > 2 {
        return rt("Wn must be a one or two element vector.");
    }
    // Remaining arguments: a band type and/or "s" for an analog design ("z" is the default).
    let mut analog = false;
    let mut band = None;
    let mut band_name = String::new();
    for i in fixed..a.len() {
        let Some(s) = text_arg(a, i) else {
            return rt("Option character vector must be one of 'high', 'stop', 'low', 'bandpass', 'z' or 's'.");
        };
        match s.as_str() {
            "s" => analog = true,
            "z" => analog = false,
            "low" => band = Some(Band::Low),
            "high" => band = Some(Band::High),
            "bandpass" => band = Some(Band::Pass),
            "stop" => band = Some(Band::Stop),
            _ => return rt("Option character vector must be one of 'high', 'stop', 'low', 'bandpass', 'z' or 's'."),
        }
        if !matches!(s.as_str(), "s" | "z") {
            band_name = s;
        }
    }
    let band = band.unwrap_or(if wn.len() == 1 { Band::Low } else { Band::Pass });
    let two = matches!(band, Band::Pass | Band::Stop);
    if two != (wn.len() == 2) {
        let shown = if band_name.is_empty() { if two { "bandpass" } else { "low" } } else { band_name.as_str() };
        return rt(format!("For the '{shown}' filter option, Wn must be a {} element vector.", if two { 2 } else { 1 }));
    }
    if analog {
        if wn.iter().any(|&w| w <= 0.0) {
            return rt("The cutoff frequencies must be greater than zero.");
        }
    } else if wn.iter().any(|&w| w <= 0.0 || w >= 1.0) {
        return rt("The cutoff frequencies must be within the interval of (0,1).");
    }

    let proto = match name {
        "butter" => buttap(n),
        "cheby1" => cheb1ap(n, r),
        _ => cheb2ap(n, r),
    };
    // Band edges in rad/s of the analog prototype's domain.
    let u: Vec<f64> = wn.iter().map(|&w| if analog { w } else { 4.0 * (std::f64::consts::PI * w / 2.0).tan() }).collect();
    let f = match band {
        Band::Low => zpk_lp2lp(&proto, u[0]),
        Band::High => zpk_lp2hp(&proto, u[0]),
        Band::Pass => zpk_lp2bp(&proto, (u[0] * u[1]).sqrt(), u[1] - u[0]),
        Band::Stop => zpk_lp2bs(&proto, (u[0] * u[1]).sqrt(), u[1] - u[0]),
    };
    let f = if analog { f } else { zpk_bilinear(&f, 4.0) };
    match nargout {
        0..=2 => {
            let (mut b, den) = zpk_to_ba(&f);
            while b.len() < den.len() {
                b.insert(0, 0.0);
            }
            out(vec![real_row(b), real_row(den)])
        }
        3 => out(vec![col_value(&f.z), col_value(&f.p), scalar_value(f.k)]),
        _ => rt("State-space output is not supported yet; request two or three outputs."),
    }
}
