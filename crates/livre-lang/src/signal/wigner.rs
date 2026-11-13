//! `wvd` and `xwvd`: the (cross) Wigner-Ville distribution built from the products
//! `x((n + m) / 2) y*((n - m) / 2)` of samples that both lie on the sampling grid, evaluated at
//! `N` frequencies and `2 N` half-sample times.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;
use std::f64::consts::PI;

pub const NAMES: &[&str] = &["wvd", "xwvd"];

type C = (f64, f64);

fn is_char(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.kind == Kind::Char)
}

fn signal_arg(v: &Value, pos: usize, label: &str) -> R<Matrix> {
    match v {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => {
            if !(m.dims.len() == 2 && (m.dims[0] == 1 || m.dims[1] == 1)) || m.numel() == 0 && false {
                return rt(format!("Expected input number {pos}, {label}, to be a vector."));
            }
            Ok(m.clone())
        }
        Value::Num(m) => rt(format!("Expected input number {pos}, {label}, to be one of these types:  single, double  Instead its type was {}.", v_class(m))).or_else(|e: crate::error::LangError| {
            if matches!(m.kind, Kind::Char) {
                rt(format!("Expected input number {pos}, {label}, to be one of these types:  single, double"))
            } else {
                Err(e)
            }
        }),
        other => rt(format!("Expected input number {pos}, {label}, to be one of these types:  single, double  Instead its type was {}.", other.class_name())),
    }
}

fn v_class(m: &Matrix) -> String {
    Value::Num(m.clone()).class_name()
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    let cross = name == "xwvd";
    let need = if cross { 2 } else { 1 };
    if a.len() < need {
        return rt("Not enough input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let x = signal_arg(&a[0], 1, if cross { "X" } else { "X" })?;
    let y = if cross { Some(signal_arg(&a[1], 2, "Y")?) } else { None };
    if let Some(y) = &y {
        if y.numel() != x.numel() {
            return rt("Inputs must have equal length.");
        }
    }
    let single = x.kind == Kind::Single || y.as_ref().map_or(false, |m| m.kind == Kind::Single);
    // Remaining arguments: sample rate, 'smoothedPseudo' windows, name-value pairs.
    let mut fs: Option<f64> = None;
    let mut smoothed = false;
    let mut threshold = f64::NEG_INFINITY;
    let mut values_only = 0;
    let mut i = need;
    while i < a.len() {
        match &a[i] {
            v if is_char(v) => {
                let s = v.text().unwrap_or_default();
                match s.to_lowercase().as_str() {
                    "smoothedpseudo" => smoothed = true,
                    "minthreshold" => {
                        i += 1;
                        match a.get(i) {
                            Some(Value::Num(m)) if m.kind != Kind::Char => {
                                if m.numel() != 1 {
                                    return rt("Expected MinThreshold to be a scalar.");
                                }
                                threshold = m.data[0];
                            }
                            _ => return rt("Expected MinThreshold to be one of these types:  double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64"),
                        }
                    }
                    "numfrequencypoints" | "numtimepoints" => {
                        if !smoothed {
                            return rt("NumTimePoints and NumFrequencyPoints parameters can only be specified when calculating the smoothed pseudo Wigner-Ville distribution.");
                        }
                        i += 1;
                    }
                    _ if i + 1 >= a.len() => return rt("Too many value-only inputs."),
                    _ => return rt(format!("'{s}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function.")),
                }
            }
            Value::Num(m) => {
                if values_only == 0 && !smoothed {
                    if m.numel() != 1 {
                        return rt("Expected sample rate to be a scalar.");
                    }
                    if !(m.data[0] > 0.0) {
                        return rt("Expected sample rate to be positive.");
                    }
                    fs = Some(m.data[0]);
                } else {
                    return rt("Too many value-only inputs.");
                }
                values_only += 1;
            }
            _ => return rt("Too many value-only inputs."),
        }
        i += 1;
    }
    if smoothed {
        return rt("wvd: the smoothed pseudo Wigner-Ville distribution is not implemented.");
    }
    if cross {
        return rt("xwvd: not implemented (the cross distribution's rule was not recovered).");
    }
    let n0 = x.numel();
    if n0 < 3 {
        return rt("Input signal must have length greater than 2.");
    }
    // Complex sequences (zero appended to an odd length; real inputs become analytic).
    let prepare = |it: &mut Interpreter, m: &Matrix| -> R<Vec<C>> {
        let len = m.numel();
        let v = if m.im.is_none() {
            // The analytic signal; wvd takes it from the record zero padded to twice its length.
            let mut padded = m.data.clone();
            if !cross {
                padded.extend(std::iter::repeat(0.0).take(len));
            }
            let pm = Matrix::from_dims(vec![padded.len(), 1], padded);
            it.call_function("hilbert", vec![Value::Num(pm)], 1)?.remove(0).to_matrix()?
        } else {
            let mut m2 = m.clone();
            m2.dims = vec![len, 1];
            m2
        };
        let im = v.im.clone().unwrap_or_else(|| vec![0.0; v.numel()]);
        Ok((0..len).map(|k| (v.data[k], im[k])).collect())
    };
    let pad = |mut m: Matrix| -> Matrix {
        if m.numel() % 2 == 1 {
            m.data.push(0.0);
            if let Some(im) = m.im.as_mut() {
                im.push(0.0);
            }
            let n = m.data.len();
            m.dims = vec![n, 1];
        }
        m
    };
    let xp = pad(x.clone());
    let n = xp.numel();
    let xs = prepare(it, &xp)?;
    let ys = match &y {
        Some(ym) => prepare(it, &pad(ym.clone()))?,
        None => xs.clone(),
    };
    let nt = 2 * n;
    let mut re = vec![0.0; n * nt];
    let mut im = vec![0.0; n * nt];
    for nu in 0..nt {
        for m in -(n as i64 / 2)..=(n as i64 / 2) {
            if (nu as i64 + m) % 2 != 0 {
                continue;
            }
            let (ai, bi) = ((nu as i64 + m) / 2, (nu as i64 - m) / 2);
            if ai < 0 || bi < 0 || ai >= n as i64 || bi >= n as i64 {
                continue;
            }
            // xwvd(x, y) pairs y at the later sample with the conjugate of x at the earlier one.
            let (xa, yb) = if cross { (ys[ai as usize], xs[bi as usize]) } else { (xs[ai as usize], ys[bi as usize]) };
            let prod = (xa.0 * yb.0 + xa.1 * yb.1, xa.1 * yb.0 - xa.0 * yb.1);
            for k in 0..n {
                let ph = -PI * k as f64 * m as f64 / n as f64;
                let (c, s) = (ph.cos(), ph.sin());
                re[k + n * nu] += prod.0 * c - prod.1 * s;
                im[k + n * nu] += prod.0 * s + prod.1 * c;
            }
        }
    }
    if threshold.is_finite() {
        for k in 0..re.len() {
            if re[k] < threshold {
                re[k] = 0.0;
                im[k] = 0.0;
            }
        }
    }
    let fmax = fs.map_or(1.0, |f| f / 2.0);
    let freq: Vec<f64> = (0..n).map(|k| fmax * k as f64 / (n as f64 - 1.0)).collect();
    let time: Vec<f64> = (0..nt).map(|k| k as f64 / 2.0 / fs.unwrap_or(1.0)).collect();
    let d = if cross { Matrix::complex(vec![n, nt], re, im) } else { Matrix::from_dims(vec![n, nt], re) };
    let mut dm = d;
    if single {
        dm.kind = Kind::Single;
    }
    let outs = vec![Value::Num(dm), real_col(freq), real_col(time)];
    out(outs.into_iter().take(nargout.max(1)).collect())
}
