//! `pmtm`: the multitaper power spectral density estimate (Slepian and sine tapers; adaptive,
//! eigenvalue-weighted and uniformly weighted combinations).

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;
use std::f64::consts::PI;

pub const NAMES: &[&str] = &["pmtm"];

#[derive(Clone, Copy, PartialEq)]
enum Method {
    Adapt,
    Eigen,
    Unity,
}

#[derive(Clone, Copy, PartialEq)]
enum Range {
    Default,
    OneSided,
    TwoSided,
    Centered,
}

const OPTIONS: &str = "Expected input to match one of these values:  'onesided', 'twosided', 'centered', 'whole', 'half', 'adapt', 'unity', 'eigen', 'confidencelevel', 'droplasttaper', 'tapers'  The input, '";

fn tapers_sine(a: &[Value]) -> bool {
    a.windows(2).any(|w| matches!(&w[0], v if is_char(v) && v.text().map_or(false, |t| t.eq_ignore_ascii_case("tapers"))) && w[1].text().map_or(false, |t| t.eq_ignore_ascii_case("sine")))
}

fn is_char(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.kind == Kind::Char)
}

/// Standard normal-free chi-square quantile through the interpreter.
fn chi2inv(it: &mut Interpreter, p: f64, dof: &[f64]) -> R<Vec<f64>> {
    let half: Vec<f64> = dof.iter().map(|d| d / 2.0).collect();
    let r = it.call_function("gammaincinv", vec![scalar_value(p), real_col(half)], 1)?;
    Ok(r[0].to_matrix()?.data.iter().map(|v| 2.0 * v).collect())
}

pub fn call(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 3 {
        return rt("One or more output arguments not assigned during call to \"varargout\".");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        Value::Num(_) => return rt("Expected x to be one of these types:  single, double"),
        v => return rt(format!("Expected x to be one of these types:  single, double  Instead its type was {}.", v.class_name())),
    };
    if x.numel() == 0 {
        return rt("Expected x to be nonempty.");
    }
    let complex = x.im.is_some();
    let vector = x.dims[0] == 1 || x.dims[1] == 1;
    let (n, ch) = if vector { (x.numel(), 1) } else { (x.dims[0], x.dims[1]) };
    let xre = x.data.clone();
    let xim = x.im.clone().unwrap_or_else(|| vec![0.0; x.numel()]);
    // Options.
    let mut method: Option<Method> = None;
    let mut range = Range::Default;
    let mut conf = 0.95;
    let mut conf_given = false;
    let mut drop_last = true;
    let mut sine = false;
    let mut nw = 4.0;
    let mut tapers_given: Option<(Matrix, Vec<f64>)> = None;
    let mut ntp: Option<f64> = None;
    let mut nfft: Option<usize> = None;
    let mut freqs: Option<Vec<f64>> = None;
    let mut fs: Option<f64> = None;
    let mut sine_m: Option<Vec<f64>> = None;
    let mut numeric_pos = 0;
    let mut i = 1;
    while i < a.len() {
        match &a[i] {
            v if is_char(v) => {
                let s = v.text().unwrap_or_default();
                match s.to_lowercase().as_str() {
                    "adapt" => method = Some(Method::Adapt),
                    "eigen" => method = Some(Method::Eigen),
                    "unity" => method = Some(Method::Unity),
                    "onesided" | "half" => {
                        if range == Range::TwoSided {
                            return rt("The 'onesided' and 'twosided' options cannot be specified together.");
                        }
                        range = Range::OneSided;
                    }
                    "twosided" | "whole" => {
                        if range == Range::OneSided {
                            return rt("The 'onesided' and 'twosided' options cannot be specified together.");
                        }
                        range = Range::TwoSided;
                    }
                    "centered" => range = Range::Centered,
                    "confidencelevel" => {
                        i += 1;
                        let Some(v) = a.get(i) else { return rt("A value was not supplied for the option 'confidencelevel'.") };
                        let ok = matches!(v, Value::Num(m) if m.kind != Kind::Char && m.numel() == 1 && m.im.is_none() && m.data[0] > 0.0 && m.data[0] < 1.0);
                        if !ok {
                            return rt("The confidence level must be a real scalar value greater than 0 and less than 1.");
                        }
                        conf = v.scalar()?;
                        conf_given = true;
                    }
                    "droplasttaper" => {
                        i += 1;
                        let Some(v) = a.get(i) else { return rt("A value was not supplied for the option 'droplasttaper'.") };
                        drop_last = v.scalar()? != 0.0;
                    }
                    "tapers" => {
                        i += 1;
                        let Some(v) = a.get(i) else { return rt("A value was not supplied for the option 'tapers'.") };
                        let t = v.text().unwrap_or_default().to_lowercase();
                        match t.as_str() {
                            "sine" => sine = true,
                            "slepian" => sine = false,
                            _ => return rt(format!("Expected TAPERS to match one of these values:  'sine', 'slepian'  The input, '{}', did not match any of the valid values.", v.text().unwrap_or_default())),
                        }
                    }
                    _ => return rt(format!("{OPTIONS}{s}', did not match any of the valid values.")),
                }
            }
            Value::Cell(c) => {
                let vals: Vec<f64> = c.data.iter().filter_map(|v| v.scalar().ok()).collect();
                if vals.is_empty() {
                    return rt("Size inputs must be integers.");
                }
                nw = vals[0];
                ntp = vals.get(1).copied();
                let _ = vals.len();
                numeric_pos = numeric_pos.max(1);
            }
            Value::Num(m) => {
                match numeric_pos {
                    0 => {
                        // nw, or e with v following
                        if m.numel() > 1 {
                            match a.get(i + 1) {
                                Some(Value::Num(vm)) if vm.numel() == m.dims[1] => {
                                    tapers_given = Some((m.clone(), vm.data.clone()));
                                    i += 1;
                                }
                                _ if tapers_sine(a) => {
                                    sine_m = Some(m.data.clone());
                                    nw = m.data[0];
                                }
                                _ => return rt("Must provide V with E matrix."),
                            }
                        } else if m.numel() == 1 {
                            nw = m.data[0];
                            sine_m = Some(vec![m.data[0]]);
                        }
                        numeric_pos = 1;
                    }
                    1 => {
                        if m.numel() == 1 {
                            let v = m.data[0];
                            if v.fract() != 0.0 {
                                return rt("Size inputs must be integers.");
                            }
                            if v < 1.0 {
                                return rt("Array indices must be positive integers or logical values.");
                            }
                            nfft = Some(v as usize);
                        } else if m.numel() > 1 {
                            freqs = Some(m.data.clone());
                        }
                        numeric_pos = 2;
                    }
                    2 => {
                        if m.numel() == 1 {
                            if !(m.data[0] > 0.0) {
                                return rt("Expected FS to be positive.");
                            }
                            fs = Some(m.data[0]);
                        } else if m.numel() == 0 {
                            fs = Some(1.0);
                        } else {
                            return rt("Expected FS to be a scalar.");
                        }
                        numeric_pos = 3;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        i += 1;
    }
    let method = method.unwrap_or(Method::Adapt);
    // Tapers.
    let (tap, lam): (Vec<Vec<f64>>, Vec<f64>) = if sine {
        let w = sine_m.clone().unwrap_or_else(|| vec![7.0]);
        let (k, wt): (usize, Vec<f64>) = if w.len() == 1 { (w[0] as usize, vec![1.0 / w[0]; w[0] as usize]) } else { (w.len(), w.clone()) };
        let t = (0..k)
            .map(|j| (0..n).map(|s| (2.0 / (n as f64 + 1.0)).sqrt() * (PI * (j as f64 + 1.0) * (s as f64 + 1.0) / (n as f64 + 1.0)).sin()).collect())
            .collect();
        (t, wt)
    } else if let Some((e, v)) = &tapers_given {
        let k = e.dims[1];
        let keep = if drop_last { k.saturating_sub(1) } else { k };
        ((0..keep).map(|j| e.data[j * e.dims[0]..(j + 1) * e.dims[0]].to_vec()).collect(), v[..keep].to_vec())
    } else {
        if drop_last && nw < 1.25 {
            return rt("Time-bandwidth product 'NW' should be greater than or equal to 1.25 when 'Droplasttaper' is true.");
        }
        if nw >= n as f64 / 2.0 {
            return rt("Time-bandwidth product NW must be less than N/2.");
        }
        let mut args = vec![scalar_value(n as f64), scalar_value(nw)];
        if let Some(k) = ntp {
            args.push(scalar_value(k));
        }
        let r = it.call_function("dpss", args, 2)?;
        let e = r[0].to_matrix()?;
        let v = r[1].to_matrix()?.data;
        let k = e.dims[1];
        let keep = if drop_last { k.saturating_sub(1) } else { k };
        ((0..keep).map(|j| e.data[j * n..(j + 1) * n].to_vec()).collect(), v[..keep].to_vec())
    };
    let k = tap.len();
    if k == 0 {
        return rt("Time-bandwidth product 'NW' should be greater than or equal to 1.25 when 'Droplasttaper' is true.");
    }
    // Frequencies.
    let nfft_v = nfft.unwrap_or_else(|| 256.max(1usize << (n as f64).log2().ceil() as u32));
    let twosided_default = complex;
    if range == Range::OneSided && complex {
        return rt("A one-sided PSD cannot be computed for a complex-valued signal.");
    }
    let range = if range == Range::Default { if twosided_default { Range::TwoSided } else { Range::OneSided } } else { range };
    let norm = match fs {
        Some(f) => f,
        None => 2.0 * PI,
    };
    // Eigenspectra for each channel: sk[c][t][f] over the evaluation frequencies.
    let (wgrid, nf): (Vec<f64>, usize) = if let Some(f) = &freqs {
        let w: Vec<f64> = match fs {
            Some(fsv) => f.iter().map(|v| 2.0 * PI * v / fsv).collect(),
            None => f.clone(),
        };
        let l = w.len();
        (w, l)
    } else {
        let nn = nfft_v;
        let all: Vec<f64> = (0..nn).map(|j| 2.0 * PI * j as f64 / nn as f64).collect();
        let sel: Vec<f64> = match range {
            Range::OneSided => {
                let cnt = if nn % 2 == 0 { nn / 2 + 1 } else { (nn + 1) / 2 };
                all[..cnt].to_vec()
            }
            Range::TwoSided => all,
            Range::Centered => {
                let start: i64 = if nn % 2 == 0 { -((nn / 2) as i64 - 1) } else { -((nn as i64 - 1) / 2) };
                (0..nn as i64).map(|j| 2.0 * PI * (start + j) as f64 / nn as f64).collect()
            }
            Range::Default => all,
        };
        let l = sel.len();
        (sel, l)
    };
    let (eval_w, sel): (Vec<f64>, Vec<usize>) = if freqs.is_some() {
        (wgrid.clone(), (0..nf).collect())
    } else {
        let nn = nfft_v;
        let all: Vec<f64> = (0..nn).map(|j| 2.0 * PI * j as f64 / nn as f64).collect();
        let sel = wgrid.iter().map(|w| ((w / (2.0 * PI) * nn as f64).round() as i64).rem_euclid(nn as i64) as usize).collect();
        (all, sel)
    };
    let nev = eval_w.len();
    let mut pxx = vec![vec![0.0; nev]; ch];
    let mut eff_dof: Vec<Vec<f64>> = vec![vec![0.0; nev]; ch];
    for c in 0..ch {
        let xr = &xre[c * n..(c + 1) * n];
        let xi = &xim[c * n..(c + 1) * n];
        let sig2: f64 = (0..n).map(|s| xr[s] * xr[s] + xi[s] * xi[s]).sum::<f64>() / n as f64;
        // Eigenspectra at the evaluation frequencies (aliased when nfft < n).
        let mut sk = vec![vec![0.0; nev]; k];
        for t in 0..k {
            for (fi, &w) in eval_w.iter().enumerate() {
                let (mut re, mut im) = (0.0, 0.0);
                for s in 0..n {
                    let ph = w * s as f64;
                    let (cs, sn) = (ph.cos(), -ph.sin());
                    let (vr, vi) = (xr[s] * tap[t][s], xi[s] * tap[t][s]);
                    re += vr * cs - vi * sn;
                    im += vr * sn + vi * cs;
                }
                sk[t][fi] = re * re + im * im;
            }
        }
        for fi in 0..nev {
            let (val, dof) = match method {
                Method::Unity => ((0..k).map(|t| sk[t][fi]).sum::<f64>() / k as f64, 2.0 * k as f64),
                Method::Eigen => {
                    let s: f64 = (0..k).map(|t| lam[t] * sk[t][fi]).sum::<f64>() / k as f64;
                    let sw: f64 = (0..k).map(|t| lam[t]).sum();
                    let sw2: f64 = (0..k).map(|t| lam[t] * lam[t]).sum();
                    let _ = (sw, sw2);
                    (s, 2.0 * k as f64)
                }
                Method::Adapt => {
                    let mut s = (sk[0][fi] + sk[1.min(k - 1)][fi]) / 2.0;
                    let mut d2 = vec![0.0; k];
                    // The iteration is run for all frequencies together, below.
                    let _ = (&mut s, &mut d2);
                    (0.0, 0.0)
                }
            };
            pxx[c][fi] = val;
            eff_dof[c][fi] = dof;
        }
        if method == Method::Adapt {
            let mut s: Vec<f64> = (0..nev).map(|fi| (sk[0][fi] + sk[1.min(k - 1)][fi]) / 2.0).collect();
            let mut d2 = vec![vec![0.0; nev]; k];
            for _it in 0..200 {
                let mut num = 0.0;
                let mut den = 0.0;
                let mut sn = vec![0.0; nev];
                for fi in 0..nev {
                    let (mut a1, mut a2) = (0.0, 0.0);
                    for t in 0..k {
                        let d = lam[t].sqrt() * s[fi] / (lam[t] * s[fi] + (1.0 - lam[t]) * sig2);
                        d2[t][fi] = d * d;
                        a1 += d * d * sk[t][fi];
                        a2 += d * d;
                    }
                    sn[fi] = a1 / a2;
                    num += (sn[fi] - s[fi]).abs();
                    den += s[fi];
                }
                s = sn;
                let _ = den;
                if num < 5e-4 * sig2 {
                    break;
                }
            }
            for fi in 0..nev {
                pxx[c][fi] = s[fi];
                let sw: f64 = (0..k).map(|t| d2[t][fi]).sum();
                let _ = sw;
                eff_dof[c][fi] = 2.0 * k as f64;
            }
        }
    }
    let pxx: Vec<Vec<f64>> = pxx.iter().map(|r| sel.iter().map(|&j| r[j]).collect()).collect();
    let eff_dof: Vec<Vec<f64>> = eff_dof.iter().map(|r| sel.iter().map(|&j| r[j]).collect()).collect();
    // Scale to a PSD and fold the one-sided spectrum.
    let mut p = vec![vec![0.0; nf]; ch];
    let one = freqs.is_none() && range == Range::OneSided;
    for c in 0..ch {
        for fi in 0..nf {
            let mut v = pxx[c][fi] / norm;
            if one {
                let last = nf - 1;
                let nyq = nfft_v % 2 == 0 && fi == last;
                if fi != 0 && !nyq {
                    v *= 2.0;
                }
            }
            p[c][fi] = v;
        }
    }
    let wout: Vec<f64> = match fs {
        Some(f) => {
            if freqs.is_some() {
                freqs.clone().unwrap()
            } else {
                wgrid.iter().map(|w| w * f / (2.0 * PI)).collect()
            }
        }
        None => wgrid.clone(),
    };
    // Matrices with columns per channel.
    let mut pdata = vec![0.0; nf * ch];
    for c in 0..ch {
        pdata[c * nf..(c + 1) * nf].copy_from_slice(&p[c]);
    }
    let row_out = freqs.as_ref().is_some() && matches!(a.iter().find(|v| matches!(v, Value::Num(m) if m.numel() > 1 && m.dims[0] == 1 && m.kind != Kind::Char)), Some(_)) && ch == 1 && freqs.as_ref().map_or(false, |f| f.len() > 1) && !has_col_freqs(a);
    let pm = Value::Num(if row_out { Matrix::from_dims(vec![1, nf], pdata) } else { Matrix::from_dims(vec![nf, ch], pdata) });
    let mut outs = vec![pm];
    if nargout > 2 {
        let lo_p = (1.0 - conf) / 2.0;
        let hi_p = 1.0 - lo_p;
        let mut cdata = vec![0.0; nf * 2 * ch];
        for c in 0..ch {
            let lo = chi2inv(it, lo_p, &eff_dof[c])?;
            let hi = chi2inv(it, hi_p, &eff_dof[c])?;
            for fi in 0..nf {
                cdata[(2 * c) * nf + fi] = p[c][fi] * eff_dof[c][fi] / hi[fi];
                cdata[(2 * c + 1) * nf + fi] = p[c][fi] * eff_dof[c][fi] / lo[fi];
            }
        }
        outs.push(Value::Num(Matrix::from_dims(vec![nf, 2 * ch], cdata)));
    }
    let wv = if row_out { real_row(wout) } else { real_col(wout) };
    if conf_given && outs.len() == 2 {
        outs.insert(1, wv);
    } else {
        outs.push(wv);
    }
    if nargout <= 2 && outs.len() == 2 {
        // [pxx, w]
    }
    out(outs.into_iter().take(nargout.max(1)).collect())
}

fn has_col_freqs(a: &[Value]) -> bool {
    a.iter().skip(1).any(|v| matches!(v, Value::Num(m) if m.numel() > 1 && m.dims[1] == 1 && m.kind != Kind::Char))
}
