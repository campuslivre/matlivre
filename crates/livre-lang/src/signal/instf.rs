//! `instfreq` with the Hilbert method: the phase derivative of the analytic signal.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;
use std::f64::consts::PI;

pub const NAMES: &[&str] = &["instfreq"];

pub fn call(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        Value::Num(_) => return rt(if matches!(&a[0], Value::Num(m) if m.kind == Kind::Char || m.kind == Kind::Logical) { "Expected Input Signal to be one of these types:  single, double".to_string() } else { format!("Expected Input Signal to be one of these types:  single, double  Instead its type was {}.", a[0].class_name()) }),
        v => return rt(format!("Expected Input Signal to be one of these types:  single, double  Instead its type was {}.", v.class_name())),
    };
    if a.len() < 2 {
        return rt("Not enough input arguments. Specify a sample rate, a sample time, or a vector of time values.");
    }
    if x.numel() == 0 {
        return rt("Expected Input Signal to be nonempty.");
    }
    let vector = x.dims[0] == 1 || x.dims[1] == 1;
    let (n, ch) = if vector { (x.numel(), 1) } else { (x.dims[0], x.dims[1]) };
    // Time base.
    let tm = match &a[1] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt("Not enough input arguments. Specify a sample rate, a sample time, or a vector of time values."),
    };
    let mut method = "tfmoment".to_string();
    let mut limits = false;
    let mut i = 2;
    while i < a.len() {
        let s = a[i].text().unwrap_or_default();
        let low = s.to_lowercase();
        if i + 1 >= a.len() {
            if matches!(low.as_str(), "method" | "frequencylimits") {
                return rt(format!("No value was given for '{s}'. Name-value pair arguments require a name followed by a value."));
            }
            return rt(format!("'{s}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function."));
        }
        match low.as_str() {
            "method" => {
                let v = a[i + 1].text().unwrap_or_default();
                match v.to_lowercase().as_str() {
                    "tfmoment" | "hilbert" => method = v.to_lowercase(),
                    _ => return rt(format!("Expected Method to match one of these values:  'tfmoment', 'hilbert'  The input, '{v}', did not match any of the valid values.")),
                }
            }
            "frequencylimits" => limits = true,
            _ => return rt(format!("'{s}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function.")),
        }
        i += 2;
    }
    let (fs, times): (f64, Vec<f64>) = if tm.numel() == 1 {
        if !(tm.data[0] > 0.0) {
            return rt("Expected Fs to be positive.");
        }
        (tm.data[0], (0..n).map(|k| k as f64 / tm.data[0]).collect())
    } else {
        if tm.numel() != n {
            return rt("Time values vector length must equal input signal length.");
        }
        if tm.data.windows(2).any(|w| w[1] <= w[0]) {
            return rt("Expected Tv to be strictly increasing.");
        }
        let step = (tm.data[n - 1] - tm.data[0]) / (n as f64 - 1.0);
        if method == "hilbert" && tm.data.windows(2).any(|w| ((w[1] - w[0]) - step).abs() > 1e-9 * step.abs().max(1.0)) {
            return rt("Time values must be uniformly spaced when 'Method' value is 'hilbert'.");
        }
        (1.0 / step, tm.data.clone())
    };
    if method == "hilbert" && limits {
        return rt("'FrequencyLimits' are not supported when 'Method' value is 'hilbert'.");
    }
    if method != "hilbert" {
        return rt("instfreq: the default 'tfmoment' method is not implemented (it rests on pspectrum's spectrogram).");
    }
    let mut data = vec![0.0; (n - 1) * ch];
    for c in 0..ch {
        let col = Matrix::from_dims(vec![n, 1], x.data[c * n..(c + 1) * n].to_vec());
        let xa = it.call_function("hilbert", vec![Value::Num(col)], 1)?.remove(0).to_matrix()?;
        let im = xa.im.clone().unwrap_or_else(|| vec![0.0; n]);
        let ph: Vec<f64> = (0..n).map(|k| im[k].atan2(xa.data[k])).collect();
        // Unwrap, then difference.
        let mut un = ph.clone();
        for k in 1..n {
            let mut d = ph[k] - ph[k - 1];
            d -= 2.0 * PI * (d / (2.0 * PI)).round();
            un[k] = un[k - 1] + d;
        }
        for k in 0..n - 1 {
            data[c * (n - 1) + k] = (un[k + 1] - un[k]) / (2.0 * PI) * fs;
        }
    }
    let t: Vec<f64> = (0..n - 1).map(|k| (times[k] + times[k + 1]) / 2.0).collect();
    let mut f = Matrix::from_dims(vec![n - 1, ch], data);
    if x.kind == Kind::Single {
        f.kind = Kind::Single;
    }
    out(vec![Value::Num(f), real_col(t)].into_iter().take(nargout.max(1)).collect())
}
