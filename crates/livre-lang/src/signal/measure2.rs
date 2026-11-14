//! Spectral and delay measurements: `enbw`, `bandpower`, `meanfreq`, `medfreq`, `finddelay`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["enbw", "bandpower", "meanfreq", "medfreq", "finddelay"];

const PI: f64 = std::f64::consts::PI;

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "enbw" => enbw(a, nargout),
        "finddelay" => finddelay(a, nargout),
        _ => spectral(it, name, a, nargout),
    }
}

fn float_matrix(v: &Value, label: &str, idx: usize) -> R<Matrix> {
    match v {
        Value::Num(m) if m.kind != Kind::Char && m.kind != Kind::Logical => Ok(m.clone()),
        other => rt(format!(
            "Expected input number {idx}, {label}, to be one of these types:  double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64{}",
            if label == "WINDOW" { String::new() } else { format!("  Instead its type was {}.", other.class_name()) }
        )),
    }
}

fn columns(m: &Matrix) -> Vec<Vec<f64>> {
    if m.dims[0] == 1 {
        return vec![m.data.clone()];
    }
    let r = m.dims[0];
    (0..m.numel() / r.max(1)).map(|c| m.data[c * r..(c + 1) * r].to_vec()).collect()
}

fn row_out(v: Vec<f64>) -> Value {
    if v.len() == 1 { scalar_value(v[0]) } else { real_row(v) }
}

fn enbw(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let w = float_matrix(&a[0], "WINDOW", 1)?;
    if w.numel() == 0 || w.dims.len() > 2 || (w.dims[0] > 1 && w.dims[1] > 1) {
        return rt("Expected input number 1, WINDOW, to be a vector.");
    }
    let n = w.numel() as f64;
    let fs = if a.len() == 2 {
        let f = float_matrix(&a[1], "Fs", 2)?;
        if f.numel() != 1 {
            return rt("Expected input number 2, Fs, to be a scalar.");
        }
        f.data[0]
    } else {
        n
    };
    let s1: f64 = w.data.iter().sum();
    let s2: f64 = w.data.iter().map(|v| v * v).sum();
    out(vec![scalar_value(fs * s2 / (s1 * s1))])
}

fn finddelay(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let num = |v: &Value, which: &str| -> R<Matrix> {
        match v {
            Value::Num(m) if m.numel() > 0 && m.kind != Kind::Char => Ok(m.clone()),
            Value::Num(m) if m.numel() == 0 => rt(format!("The {which} input {} must be a vector or a matrix.", if which == "first" { "X" } else { "Y" })),
            _ => rt(format!("The {which} input {} must be numeric.", if which == "first" { "X" } else { "Y" })),
        }
    };
    let x = num(&a[0], "first")?;
    let y = num(&a[1], "second")?;
    let maxlag = if a.len() == 3 {
        match &a[2] {
            Value::Num(m) if m.kind != Kind::Char && m.numel() == 1 && m.im.is_none() => Some(m.data[0].abs()),
            _ => return rt("The third input MAXLAG must be numeric and real."),
        }
    } else {
        None
    };
    let xc = columns(&x);
    let yc = columns(&y);
    let n = xc.len().max(yc.len());
    let mut res = vec![];
    for k in 0..n {
        let xv = &xc[if xc.len() == 1 { 0 } else { k.min(xc.len() - 1) }];
        let yv = &yc[if yc.len() == 1 { 0 } else { k.min(yc.len() - 1) }];
        let lim = maxlag.unwrap_or((xv.len().max(yv.len()) - 1) as f64) as i64;
        let (lx, ly) = (xv.len() as i64, yv.len() as i64);
        let mut best: Option<(f64, i64)> = None;
        for d in -(lx - 1).min(lim)..=(ly - 1).min(lim) {
            let mut s = 0.0;
            for i in 0..lx {
                let j = i + d;
                if j >= 0 && j < ly {
                    s += xv[i as usize] * yv[j as usize];
                }
            }
            let s = s.abs();
            let better = match best {
                None => true,
                Some((bs, bd)) => s > bs || (s == bs && d.abs() < bd.abs()),
            };
            if better {
                best = Some((s, d));
            }
        }
        res.push(best.map_or(0.0, |b| b.1 as f64));
    }
    out(vec![row_out(res)])
}

/// One-sided PSD of a real column with the given window kind, `nfft = N`, as `periodogram` gives.
fn psd(it: &mut Interpreter, x: &[f64], hamming: bool, fs: f64) -> R<(Vec<f64>, Vec<f64>)> {
    let n = x.len();
    let win = if hamming { it.call_function("hamming", vec![scalar_value(n as f64)], 1)?.remove(0) } else { it.call_function("rectwin", vec![scalar_value(n as f64)], 1)?.remove(0) };
    let r = it.call_function("periodogram", vec![real_col(x.to_vec()), win, scalar_value(n as f64), scalar_value(fs)], 2)?;
    Ok((r[0].to_matrix()?.data, r[1].to_matrix()?.data))
}

fn spectral(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let is_text = |v: &Value| matches!(v, Value::Num(m) if m.kind == Kind::Char) || matches!(v, Value::Str(_));
    let mut args: Vec<&Value> = a.iter().collect();
    let mut psd_flag = false;
    if args.len() > 1 && is_text(args[args.len() - 1]) {
        let s = args[args.len() - 1].text().unwrap_or_default().to_lowercase();
        if s != "psd" {
            return rt("Unrecognized input option. 'psd' is the only supported input option.");
        }
        psd_flag = true;
        args.pop();
    }
    let x = float_matrix(args[0], "X", 1)?;
    let two_sided_complex = x.im.is_some();
    let cols = columns(&x);
    let single_vec = x.dims.len() == 2 && (x.dims[0] == 1 || x.dims[1] == 1);
    if !single_vec && x.dims.len() > 2 || x.numel() == 0 {
        return rt("The input must be a non-empty vector or two-dimensional matrix.");
    }
    // PSD form: (pxx, f[, range]).
    let psd_form = psd_flag || (name != "bandpower" && args.len() >= 2 && matches!(args[1], Value::Num(m) if m.numel() > 1));
    let range_arg = |v: &Value| -> R<(f64, f64)> {
        let m = float_matrix(v, "FREQRANGE", 3)?;
        if m.numel() != 2 {
            return if name == "bandpower" {
                rt("Expected input number 3, FREQRANGE, to be an array with number of elements equal to 2.")
            } else {
                rt(format!("Expected input number 3, FREQRANGE, to be of size 1x2, but it is of size {}x{}.", m.dims[0], m.dims[1]))
            };
        }
        if m.data[1] <= m.data[0] {
            return rt("Expected input number 3, FREQRANGE, to be strictly increasing.");
        }
        Ok((m.data[0], m.data[1]))
    };
    let mut results = vec![];
    if psd_form {
        if name != "bandpower" && x.data.iter().any(|&v| v < 0.0) {
            return rt("Expected input number 1, Pxx, to be nonnegative.");
        }
        let fm = float_matrix(args.get(1).ok_or_else(|| crate::error::LangError::new("Not enough input arguments.", 0))?, "F", 2)?;
        if fm.numel() != x.rows_len() {
            return rt(format!("Expected input number 2, F, to be an array with number of elements equal to {}.", x.rows_len()));
        }
        let f = fm.data.clone();
        if name == "bandpower" && x.data.iter().any(|&v| v < 0.0) {
            return rt("Expected input number 1, Pxx, to be nonnegative.");
        }
        let range = match args.get(2) {
            Some(v) => Some(range_arg(v)?),
            None => None,
        };
        if x.data.iter().any(|&v| v < 0.0) {
            return rt("Expected input number 1, Pxx, to be nonnegative.");
        }
        for c in &cols {
            results.push(measure(name, c, &f, range, false)?);
        }
    } else {
        if name == "bandpower" && args.len() == 1 {
            // Mean-square power.
            for c in &cols {
                results.push(c.iter().map(|v| v * v).sum::<f64>() / c.len() as f64);
            }
            return out(vec![row_out(results)]);
        }
        if name == "bandpower" && args.len() == 2 {
            return rt("You must specify both FS and FREQRANGE together.");
        }
        let fs = if args.len() >= 2 {
            let m = float_matrix(args[1], "Fs", 2)?;
            if m.numel() != 1 {
                return rt("Expected input number 2, Fs, to be a scalar.");
            }
            if m.data[0] <= 0.0 {
                return rt("Expected input number 2, Fs, to be positive.");
            }
            m.data[0]
        } else {
            2.0 * PI
        };
        let range = match args.get(2) {
            Some(v) => Some(range_arg(v)?),
            None => None,
        };
        if two_sided_complex {
            return rt("Complex input is not supported.");
        }
        for c in &cols {
            if name == "bandpower" && c.len() < 2 {
                return rt("Index exceeds the number of array elements. Index must not exceed 1.");
            }
            let (pxx, f) = psd(it, c, name == "bandpower", fs)?;
            results.push(measure(name, &pxx, &f, range, true)?);
        }
    }
    out(vec![row_out(results)])
}

fn measure(name: &str, pxx: &[f64], f: &[f64], range: Option<(f64, f64)>, _from_signal: bool) -> R<f64> {
    let rbw0 = if f.len() > 1 { f[1] - f[0] } else { 1.0 };
    let (lo, hi) = range.unwrap_or(if name == "medfreq" { ((f[0] - rbw0 / 2.0).max(0.0), f[f.len() - 1] + rbw0 / 2.0) } else { (f[0], f[f.len() - 1]) });
    if range.is_some() && (lo < f[0] || hi > f[f.len() - 1]) {
        return rt("The frequency range must be within the range of F for the specified input.");
    }
    let idx: Vec<usize> = (0..f.len()).filter(|&i| f[i] >= lo && f[i] <= hi).collect();
    let rbw = if f.len() > 1 { f[1] - f[0] } else { 1.0 };
    match name {
        "bandpower" => {
            let a = (0..f.len()).rev().find(|&i| f[i] <= lo).unwrap_or(0);
            let b = (0..f.len()).find(|&i| f[i] >= hi).unwrap_or(f.len() - 1);
            let mut s: f64 = (a..=b).map(|i| pxx[i]).sum::<f64>() * rbw;
            if range.is_some() && b == f.len() - 1 && hi >= f[f.len() - 1] {
                s -= pxx[f.len() - 1] * rbw;
            }
            Ok(s)
        }
        "meanfreq" => {
            let num: f64 = idx.iter().map(|&i| f[i] * pxx[i]).sum();
            let den: f64 = idx.iter().map(|&i| pxx[i]).sum();
            Ok(num / den)
        }
        _ if range.is_none() => {
            // Cumulative power at the upper edge of each bin, joined from (0, 0).
            let total: f64 = pxx.iter().sum();
            if total.is_nan() || total == 0.0 {
                return Ok(f64::NAN);
            }
            let (mut px, mut py) = (0.0, 0.0);
            let mut acc = 0.0;
            for i in 0..f.len() {
                acc += pxx[i];
                let (cx, cy) = (f[i] + rbw / 2.0, acc / total);
                if cy >= 0.5 {
                    return Ok(if cy == py { cx } else { px + (cx - px) * (0.5 - py) / (cy - py) });
                }
                px = cx;
                py = cy;
            }
            Ok(f64::NAN)
        }
        _ => {
            // Bins are intervals around each frequency, clipped to the range.
            let mut parts: Vec<(f64, f64, f64)> = vec![];
            for i in 0..f.len() {
                let (a, b) = ((f[i] - rbw / 2.0).max(lo), (f[i] + rbw / 2.0).min(hi));
                if b > a {
                    parts.push((a, b, pxx[i]));
                }
            }
            let total: f64 = parts.iter().map(|p| (p.1 - p.0) * p.2).sum();
            if total.is_nan() || total == 0.0 {
                return Ok(f64::NAN);
            }
            let mut acc = 0.0;
            for p in &parts {
                let w = (p.1 - p.0) * p.2;
                if acc + w >= total / 2.0 {
                    return Ok(p.0 + (total / 2.0 - acc) / p.2);
                }
                acc += w;
            }
            Ok(f64::NAN)
        }
    }
}

trait RowsLen {
    fn rows_len(&self) -> usize;
}

impl RowsLen for Matrix {
    fn rows_len(&self) -> usize {
        if self.dims[0] == 1 { self.dims[1] } else { self.dims[0] }
    }
}
