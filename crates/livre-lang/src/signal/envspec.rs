//! `envspectrum`: the envelope spectrum of a signal (demodulation or Hilbert method).

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["envspectrum"];

/// Zero phase FIR filtering: zero padded by half the filter length, filtered, shifted back.
fn fir_apply(it: &mut Interpreter, h: &Matrix, x: &[f64]) -> R<Vec<f64>> {
    let delay = (h.numel() - 1) / 2;
    let n = x.len();
    let mut p = x.to_vec();
    p.extend(std::iter::repeat(0.0).take(delay));
    let y = it.call_function("filter", vec![Value::Num(h.clone()), scalar_value(1.0), real_col(p)], 1)?.remove(0).to_matrix()?;
    Ok(y.data[delay..delay + n].to_vec())
}

fn short(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

pub fn call(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if nargout > 4 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        Value::Num(m) if m.kind == Kind::Char || m.kind == Kind::Logical => return rt("Expected X to be one of these types:  single, double"),
        v => return rt(format!("Expected X to be one of these types:  single, double  Instead its type was {}.", v.class_name())),
    };
    let fsm = match &a[1] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt("Expected Fs to be positive."),
    };
    if fsm.numel() != 1 {
        return rt("Expected Fs to be a scalar.");
    }
    let fs = fsm.data[0];
    if !(fs > 0.0) {
        return rt("Expected Fs to be positive.");
    }
    let mut band: Option<Matrix> = None;
    let mut order: Option<Matrix> = None;
    let mut method = "demod".to_string();
    let mut i = 2;
    while i < a.len() {
        let s = a[i].text().unwrap_or_default();
        let low = s.to_lowercase();
        if i + 1 >= a.len() {
            if matches!(low.as_str(), "band" | "filterorder" | "method") {
                return rt(format!("No value was given for '{s}'. Name-value pair arguments require a name followed by a value."));
            }
            return rt(format!("'{s}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function."));
        }
        match low.as_str() {
            "band" => band = Some(a[i + 1].to_matrix()?),
            "filterorder" => order = Some(a[i + 1].to_matrix()?),
            "method" => {
                let v = a[i + 1].text().unwrap_or_default();
                match v.to_lowercase().as_str() {
                    "demod" | "hilbert" => method = v.to_lowercase(),
                    _ => return rt(format!("Expected Method to match one of these values:  'hilbert', 'demod'  The input, '{v}', did not match any of the valid values.")),
                }
            }
            _ => return rt(format!("'{s}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function.")),
        }
        i += 2;
    }
    let nyq = fs / 2.0;
    let bd = match &band {
        Some(m) => {
            if m.numel() != 2 {
                return rt("Expected Band to be an array with number of elements equal to 2.");
            }
            if m.data.iter().any(|v| *v < 0.0) {
                return rt("Expected Band to be nonnegative.");
            }
            if m.data.iter().any(|v| *v >= nyq) {
                return rt(format!("Expected Band to be an array with all of the values < {}.", short(nyq)));
            }
            if !(m.data[1] > m.data[0]) {
                return rt("Expected Band to be strictly increasing.");
            }
            [m.data[0], m.data[1]]
        }
        None => [fs / 4.0, fs * 3.0 / 8.0],
    };
    let ord = match &order {
        Some(m) => {
            if m.numel() != 1 {
                return rt("Expected FilterOrder to be a scalar.");
            }
            let v = m.data[0];
            if !(v > 0.0) {
                return rt("Expected FilterOrder to be positive.");
            }
            if v.fract() != 0.0 {
                return rt("Expected FilterOrder to be integer-valued.");
            }
            v as usize
        }
        None => 50,
    };
    if x.im.is_some() {
        return rt("Expected X to be real.");
    }
    let vector = x.dims[0] == 1 || x.dims[1] == 1;
    let (n, ch) = if vector { (x.numel(), 1) } else { (x.dims[0], x.dims[1]) };
    if n < 4 {
        return rt("X must have at least 4 samples.");
    }
    let h = if method == "demod" {
        let wc = (bd[1] - bd[0]) / 2.0 / nyq;
        it.call_function("fir1", vec![scalar_value(ord as f64), scalar_value(wc)], 1)?.remove(0).to_matrix()?
    } else {
        it.call_function("fir1", vec![scalar_value(ord as f64), real_row(vec![bd[0] / nyq, bd[1] / nyq]), Value::string("bandpass")], 1)?.remove(0).to_matrix()?
    };
    let nout = if n % 2 == 0 { n / 2 + 1 } else { (n + 1) / 2 };
    let mut es = vec![0.0; nout * ch];
    let mut envs = vec![0.0; n * ch];
    for c in 0..ch {
        let col = &x.data[c * n..(c + 1) * n];
        let mean = col.iter().sum::<f64>() / n as f64;
        let xz: Vec<f64> = col.iter().map(|v| v - mean).collect();
        let env: Vec<f64> = if method == "demod" {
            let f0 = (bd[0] + bd[1]) / 2.0;
            let (mut yr, mut yi) = (vec![0.0; n], vec![0.0; n]);
            for k in 0..n {
                let ph = -2.0 * std::f64::consts::PI * f0 * k as f64 / fs;
                yr[k] = xz[k] * ph.cos();
                yi[k] = xz[k] * ph.sin();
            }
            let lr = fir_apply(it, &h, &yr)?;
            let li = fir_apply(it, &h, &yi)?;
            (0..n).map(|k| 2.0 * lr[k].hypot(li[k])).collect()
        } else {
            let bp = fir_apply(it, &h, &xz)?;
            let xa = it.call_function("hilbert", vec![real_col(bp)], 1)?.remove(0).to_matrix()?;
            let im = xa.im.clone().unwrap_or_else(|| vec![0.0; n]);
            (0..n).map(|k| xa.data[k].hypot(im[k])).collect()
        };
        let em = env.iter().sum::<f64>() / n as f64;
        let e0: Vec<f64> = env.iter().map(|v| v - em).collect();
        let spec = it.call_function("fft", vec![real_col(e0.clone())], 1)?.remove(0).to_matrix()?;
        let sim = spec.im.clone().unwrap_or_else(|| vec![0.0; n]);
        for k in 0..nout {
            let mag = spec.data[k].hypot(sim[k]) / n as f64;
            let interior = k != 0 && !(n % 2 == 0 && k == n / 2);
            es[c * nout + k] = if interior { 2.0 * mag } else { mag };
        }
        envs[c * n..(c + 1) * n].copy_from_slice(&e0);
    }
    let f: Vec<f64> = (0..nout).map(|k| fs * k as f64 / n as f64).collect();
    let t: Vec<f64> = (0..n).map(|k| k as f64 / fs).collect();
    let mut esm = Matrix::from_dims(vec![nout, ch], es);
    let mut envm = Matrix::from_dims(vec![n, ch], envs);
    if x.kind == Kind::Single {
        esm.kind = Kind::Single;
        envm.kind = Kind::Single;
    }
    out(vec![Value::Num(esm), real_col(f), Value::Num(envm), real_col(t)].into_iter().take(nargout.max(1)).collect())
}
