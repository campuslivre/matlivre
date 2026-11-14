//! Bandwidth measures and the 2-D cross-correlation: `obw`, `powerbw`, `xcorr2`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["obw", "powerbw", "xcorr2"];

const PI: f64 = std::f64::consts::PI;

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if name == "xcorr2" {
        return xcorr2(it, a, nargout);
    }
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 4 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        _ => return rt("Expected input number 1, X, to be one of these types:  double, single"),
    };
    if x.numel() == 0 {
        return rt("Expected X to be nonempty.");
    }
    if a.len() > 4 {
        return rt(if name == "obw" { "Unrecognized input arguments after power percentage, P." } else { "Unrecognized input arguments after dB reference, R." });
    }
    let psd_form = matches!(a.get(1), Some(Value::Num(m)) if m.numel() > 1);
    let mut fsv = 2.0 * PI;
    if !psd_form {
        if let Some(Value::Num(m)) = a.get(1) {
            if m.numel() == 1 {
                if m.data[0] <= 0.0 {
                    return rt("Expected input number 2, Fs, to be positive.");
                }
                fsv = m.data[0];
            }
        }
    }
    let range: Option<(f64, f64)> = match a.get(2) {
        Some(Value::Num(m)) if m.numel() > 0 => {
            if m.numel() != 2 {
                return rt(format!("Expected input number 3, FREQRANGE, to be of size 1x2, but it is of size {}x{}.", m.dims[0], m.dims[1]));
            }
            if m.data[1] <= m.data[0] {
                return rt("Expected input number 3, FREQRANGE, to be strictly increasing.");
            }
            Some((m.data[0], m.data[1]))
        }
        _ => None,
    };
    let level = match a.get(3) {
        Some(Value::Num(m)) if m.numel() == 1 => Some(m.data[0]),
        _ => None,
    };
    if name == "obw" {
        if let Some(p) = level {
            if p <= 0.0 {
                return rt("Expected P to be positive.");
            }
            if p >= 100.0 {
                return rt("Expected P to be a scalar with value < 100.");
            }
        }
    } else if level == Some(0.0) {
        return rt("Expected R to be nonzero.");
    }
    let pct = level.unwrap_or(99.0);
    let db = level.unwrap_or(10.0 * 0.5f64.log10());
    if x.im.is_some() && !psd_form {
        return rt("Complex input is not supported.");
    }
    let mut cols: Vec<(Vec<f64>, Vec<f64>)> = vec![];
    if psd_form {
        let f = match a.get(1) {
            Some(Value::Num(m)) => m.data.clone(),
            _ => vec![],
        };
        let r = x.dims[0];
        for c in 0..x.numel() / r.max(1) {
            cols.push((x.data[c * r..(c + 1) * r].to_vec(), f.clone()));
        }
    } else {
        if x.dims.len() > 2 || x.numel() == 1 {
            return rt("The input must be a non-empty vector or two-dimensional matrix.");
        }
        let parts: Vec<Vec<f64>> = if x.dims[0] == 1 {
            vec![x.data.clone()]
        } else {
            let r = x.dims[0];
            (0..x.numel() / r).map(|c| x.data[c * r..(c + 1) * r].to_vec()).collect()
        };
        for col in parts {
            let n = col.len();
            let win = it.call_function("rectwin", vec![scalar_value(n as f64)], 1)?.remove(0);
            let r = it.call_function("periodogram", vec![real_col(col), win, scalar_value(n as f64), scalar_value(fsv)], 2)?;
            cols.push((r[0].to_matrix()?.data, r[1].to_matrix()?.data));
        }
    }
    let mut res: Vec<[f64; 4]> = vec![];
    for (pxx, f) in &cols {
        let rbw = if f.len() > 1 { f[1] - f[0] } else { 1.0 };
        let (rlo, rhi) = range.unwrap_or((f[0], f[f.len() - 1]));
        if range.is_some() && (rlo < f[0] || rhi > f[f.len() - 1]) {
            return rt("The frequency range must be within the range of F for the specified input.");
        }
        let idx: Vec<usize> = (0..f.len()).filter(|&i| f[i] >= rlo && f[i] <= rhi).collect();
        if idx.is_empty() {
            res.push([f64::NAN; 4]);
            continue;
        }
        if name == "obw" {
            // Bins are intervals around each frequency, clipped to the range.
            let (clo, chi) = if range.is_some() { (rlo, rhi) } else { (f64::NEG_INFINITY, f64::INFINITY) };
            let parts: Vec<(f64, f64, f64)> = idx
                .iter()
                .map(|&i| (((f[i] - rbw / 2.0).max(clo)), (f[i] + rbw / 2.0).min(chi), pxx[i]))
                .filter(|p| p.1 > p.0)
                .collect();
            let total_clipped: f64 = parts.iter().map(|p| (p.1 - p.0) * p.2).sum();
            let total = if range.is_some() { total_clipped } else { idx.iter().map(|&i| pxx[i]).sum::<f64>() * rbw };
            let tail = (100.0 - pct) / 200.0;
            let mut acc = 0.0;
            let (mut lo, mut hi) = (f64::NAN, f64::NAN);
            for p in &parts {
                let w = (p.1 - p.0) * p.2 / total_clipped;
                if lo.is_nan() && acc + w >= tail {
                    lo = p.0 + (tail - acc) / (p.2 / total_clipped);
                }
                if hi.is_nan() && acc + w >= 1.0 - tail {
                    hi = p.0 + (1.0 - tail - acc) / (p.2 / total_clipped);
                }
                acc += w;
            }
            res.push([hi - lo, lo, hi, pct / 100.0 * total]);
        } else {
            let (mut ik, mut pk) = (idx[0], f64::NEG_INFINITY);
            for &i in &idx {
                if pxx[i] > pk {
                    pk = pxx[i];
                    ik = i;
                }
            }
            if pk == 0.0 {
                res.push([f64::NAN; 4]);
                continue;
            }
            let refp = if range.is_some() { idx.iter().map(|&i| pxx[i]).sum::<f64>() / idx.len() as f64 } else { pk };
            let dbv: Vec<f64> = pxx.iter().map(|&v| 10.0 * (v / refp).log10()).collect();
            let first = idx[0];
            let last = *idx.last().unwrap();
            let mut lo = f[first];
            let mut j = ik;
            while j > first && db < 0.0 {
                j -= 1;
                if dbv[j] <= db {
                    lo = f[j] + (db - dbv[j]) / (dbv[j + 1] - dbv[j]) * (f[j + 1] - f[j]);
                    break;
                }
            }
            let mut hi = f[last];
            let mut k = ik;
            while k < last && db < 0.0 {
                k += 1;
                if dbv[k] <= db {
                    hi = f[k - 1] + (db - dbv[k - 1]) / (dbv[k] - dbv[k - 1]) * (f[k] - f[k - 1]);
                    break;
                }
            }
            let mut power = 0.0;
            for i in 0..f.len() {
                let (a0, b0) = ((f[i] - rbw / 2.0).max(lo), (f[i] + rbw / 2.0).min(hi));
                if b0 > a0 {
                    power += (b0 - a0) * pxx[i];
                }
            }
            res.push([hi - lo, lo, hi, power]);
        }
    }
    let n = res.len();
    let pick = |k: usize| -> Value {
        let v: Vec<f64> = res.iter().map(|r| r[k]).collect();
        if n == 1 { scalar_value(v[0]) } else { real_row(v) }
    };
    let mut outv: Vec<Value> = (0..4).map(pick).collect();
    outv.truncate(nargout.max(1));
    out(outv)
}

fn xcorr2(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let ok = |v: &Value| matches!(v, Value::Num(m) if m.kind != Kind::Char);
    if !ok(&a[0]) || (a.len() > 1 && !ok(&a[1])) {
        return rt("Invalid data type. First and second arguments must be numeric or logical.");
    }
    let am = a[0].to_matrix()?;
    let bm = if a.len() > 1 { a[1].to_matrix()? } else { am.clone() };
    if am.numel() == 0 || bm.numel() == 0 {
        let dims = if am.numel() == 0 { bm.dims.clone() } else { am.dims.clone() };
        let sz: usize = dims.iter().product();
        return out(vec![Value::Num(Matrix::from_dims(dims, vec![0.0; sz]))]);
    }
    // conv2(a, rot180(conj(b)))
    let (r, c) = (bm.dims[0], bm.dims[1]);
    let n = bm.numel();
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    for i in 0..r {
        for j in 0..c {
            let src = j * r + i;
            let dst = (c - 1 - j) * r + (r - 1 - i);
            re[dst] = bm.data[src];
            im[dst] = -bm.im_at(src);
        }
    }
    let mut rot = if bm.im.is_some() { Matrix::complex(vec![r, c], re, im) } else { Matrix::from_dims(vec![r, c], re) };
    rot.kind = bm.kind;
    let res = it.call_function("conv2", vec![Value::Num(am), Value::Num(rot)], 1)?;
    Ok(Some(res))
}
