//! `poctave`: octave-band power of a signal (a Butterworth filter bank on the ANSI S1.11
//! center frequencies) or octave smoothing of a power spectral density.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["poctave"];

fn is_char(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.kind == Kind::Char)
}

const NAMED: [&str; 7] = ["filterorder", "bandsperoctave", "frequencylimits", "minthreshold", "weighting", "windowlength", "overlappercent"];

/// ANSI S1.11 center frequencies of the bands that overlap `[lo, hi]`, with their edges.
fn bands(b: f64, lo: f64, hi: f64, edge_limit: f64) -> Vec<(f64, f64, f64)> {
    let g = 10f64.powf(0.3);
    let integer = b.fract() == 0.0;
    let even = integer && (b as i64) % 2 == 0;
    let mut out = vec![];
    for x in -400i64..=400 {
        let xf = x as f64;
        let fc = if even { 1000.0 * g.powf((2.0 * xf + 1.0) / (2.0 * b)) } else { 1000.0 * g.powf(xf / b) };
        let (f1, f2) = (fc / g.powf(1.0 / (2.0 * b)), fc * g.powf(1.0 / (2.0 * b)));
        if f2 > lo && f1 < edge_limit * hi {
            out.push((fc, f1, f2));
        }
    }
    out
}

pub fn call(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        _ => return rt("Invalid data type. Input signal must be a double, single, or a timetable."),
    };
    if a.len() < 2 {
        return rt("Not enough input arguments. Specify sample rate when using with vector or matrix inputs.");
    }
    let fsm = match &a[1] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt("Expected Sample rate to be positive."),
    };
    if fsm.numel() != 1 {
        return rt("Expected Sample rate to be a scalar.");
    }
    let fs = fsm.data[0];
    if !(fs > 0.0) {
        return rt("Expected Sample rate to be positive.");
    }
    // Spectrum type, name-value pairs and the frequency vector of the smoothing syntax.
    let mut kind: Option<String> = None;
    let mut fvec: Option<Matrix> = None;
    let mut order: Option<Matrix> = None;
    let mut bpo: Option<Matrix> = None;
    let mut flim: Option<Matrix> = None;
    let mut minthr: Option<Value> = None;
    let mut weighting = false;
    let mut window = false;
    let mut overlap = false;
    let mut i = 2;
    while i < a.len() {
        match &a[i] {
            v if is_char(v) => {
                let s = v.text().unwrap_or_default();
                let low = s.to_lowercase();
                if low == "power" || low == "spectrogram" {
                    if kind.is_some() {
                        return rt("Too many spectrum types. Specify spectrum type as either 'power' or 'spectrogram'.");
                    }
                    kind = Some(low);
                } else if let Some(name) = NAMED.iter().find(|n| **n == low) {
                    let Some(val) = a.get(i + 1) else { return rt("Incorrect number of input arguments. Each parameter name must be followed by a corresponding value.") };
                    match *name {
                        "filterorder" => order = Some(val.to_matrix()?),
                        "bandsperoctave" => bpo = Some(val.to_matrix()?),
                        "frequencylimits" => flim = Some(val.to_matrix()?),
                        "minthreshold" => minthr = Some(val.clone()),
                        "weighting" => weighting = true,
                        "windowlength" => window = true,
                        _ => overlap = true,
                    }
                    i += 1;
                } else if i + 1 >= a.len() {
                    return rt("Incorrect number of input arguments. Each parameter name must be followed by a corresponding value.");
                } else {
                    return rt(format!("'{s}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function."));
                }
            }
            Value::Num(m) if i == 2 => fvec = Some(m.clone()),
            Value::Num(_) => return rt("Too many input arguments. Specify a sample rate for octave spectrum,  and also a frequency vector for octave smoothing."),
            _ => return rt("Too many input arguments. Specify a sample rate for octave spectrum,  and also a frequency vector for octave smoothing."),
        }
        i += 1;
    }
    let is_spectrogram = kind.as_deref() == Some("spectrogram");
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    if nargout > 2 && !is_spectrogram {
        return rt("Too many output arguments. When spectrum type is 'power', the maximum number of outputs is two.");
    }
    if !is_spectrogram && window {
        return rt("'WindowLength' is supported only when spectrum type is 'spectrogram'.");
    }
    if !is_spectrogram && overlap {
        return rt("'OverlapPercent' is supported only when spectrum type is 'spectrogram'.");
    }
    // Name-value validation.
    let nyq = fs / 2.0;
    if fvec.is_none() && fs < 7.0 {
        return rt("Input signal must be sampled at a rate of at least 7 Hz");
    }
    let n_order = match &order {
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
            if (v as i64) % 2 != 0 {
                return rt("Expected FilterOrder to be even.");
            }
            v as usize
        }
        None => 6,
    };
    let b = match &bpo {
        Some(m) => {
            if m.numel() != 1 {
                return rt("Expected BandsPerOctave to be a scalar.");
            }
            let v = m.data[0];
            if ![1.0, 1.5, 2.0, 3.0, 6.0, 12.0, 24.0, 48.0, 96.0].contains(&v) {
                return rt("BandsPerOctave value must be one of the following:  1, 3/2 , 2 , 3, 6, 12, 24, 48, or 96.");
            }
            v
        }
        None => 1.0,
    };
    let lo_lim = 3.0f64.max(3.0 * fs / 48e3);
    let (flo, fhi) = match &flim {
        Some(m) => {
            if m.numel() != 2 {
                return rt("Expected FrequencyLimits to be an array with number of elements equal to 2.");
            }
            if !(m.data[1] > m.data[0]) {
                return rt("Expected FrequencyLimits to be strictly increasing.");
            }
            if m.data[0] < lo_lim || m.data[1] > nyq {
                return rt(format!("'FrequencyLimits' value is outside the allowed range. Specify limits between  {:.6} and  {:.6} Hz.", lo_lim, nyq));
            }
            (m.data[0], m.data[1])
        }
        None => (lo_lim, nyq),
    };
    let threshold = match &minthr {
        Some(Value::Num(m)) if m.kind != Kind::Char => {
            if m.numel() != 1 {
                return rt("Expected MinThreshold to be a scalar.");
            }
            Some(m.data[0])
        }
        Some(_) => return rt("Expected MinThreshold to be one of these types:  double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64"),
        None => None,
    };
    if weighting {
        return rt(if fvec.is_some() { "Frequency weighting is not supported for octave smoothing." } else { "poctave: the Weighting option is not implemented." });
    }
    if is_spectrogram {
        return rt("poctave: the 'spectrogram' spectrum type is not implemented.");
    }
    let vector = x.dims[0] == 1 || x.dims[1] == 1;
    let (n_rows, ch) = if vector { (x.numel(), 1) } else { (x.dims[0], x.dims[1]) };
    if let Some(fm) = fvec {
        // Octave smoothing of a PSD.
        if x.im.is_some() {
            return rt("Expected Pxx to be real.");
        }
        if x.data.iter().any(|v| *v < 0.0) {
            return rt("Expected Pxx to be nonnegative.");
        }
        if fm.numel() != n_rows {
            return rt(format!("Expected F to be an array with number of elements equal to {n_rows}."));
        }
        let f = fm.data.clone();
        if f.windows(2).any(|w| w[1] <= w[0]) {
            return rt("Expected F to be strictly increasing.");
        }
        if f.iter().any(|v| *v < 0.0 || *v > nyq) {
            return rt("Frequency vector values must be within the Nyquist range (0 to sampleRate/2 Hz).");
        }
        let bl = bands(b, flo, fhi, 1.0);
        let df = if f.len() > 1 { f[1] - f[0] } else { 1.0 };
        let mut data = vec![0.0; bl.len() * ch];
        for c in 0..ch {
            for (bi, &(_, f1, f2)) in bl.iter().enumerate() {
                let f2 = f2.min(nyq);
                let mut acc = 0.0;
                for (k, &fk) in f.iter().enumerate() {
                    let (lo, hi) = (fk - df / 2.0, fk + df / 2.0);
                    let ov = (hi.min(f2) - lo.max(f1)).max(0.0);
                    let mut w = ov / df;
                    if (k == 0 && fk == 0.0) || (k == f.len() - 1 && (fk - nyq).abs() < 1e-12) {
                        // The half bin beyond the edge counts too.
                        w = ov * 2.0 / df;
                    }
                    acc += x.data[c * n_rows + k] * df * w;
                }
                data[c * bl.len() + bi] = acc;
            }
        }
        return finish(bl.iter().map(|b| b.0).collect(), data, ch, threshold, nargout);
    }
    if x.im.is_some() {
        return rt("Expected X to be real.");
    }
    if n_rows < 2 {
        return rt("Input signal must have at least two samples.");
    }
    let bl = bands(b, flo, fhi, 0.9);
    let mut data = vec![0.0; bl.len() * ch];
    for (bi, &(_, f1, f2)) in bl.iter().enumerate() {
        let (zpk_args, nout) = if f2 >= nyq {
            (vec![scalar_value(n_order as f64), scalar_value(f1 / nyq), Value::string("high")], 3)
        } else {
            (vec![scalar_value((n_order / 2) as f64), real_row(vec![f1 / nyq, f2 / nyq]), Value::string("bandpass")], 3)
        };
        let zpk = it.call_function("butter", zpk_args, nout)?;
        let sos = it.call_function("zp2sos", vec![zpk[0].clone(), zpk[1].clone(), zpk[2].clone()], 1)?.remove(0);
        for c in 0..ch {
            let col = Matrix::from_dims(vec![n_rows, 1], x.data[c * n_rows..(c + 1) * n_rows].to_vec());
            let y = it.call_function("sosfilt", vec![sos.clone(), Value::Num(col)], 1)?.remove(0).to_matrix()?;
            data[c * bl.len() + bi] = y.data.iter().map(|v| v * v).sum::<f64>() / n_rows as f64;
        }
    }
    finish(bl.iter().map(|b| b.0).collect(), data, ch, threshold, nargout)
}

fn finish(cf: Vec<f64>, mut data: Vec<f64>, ch: usize, threshold: Option<f64>, nargout: usize) -> Ret {
    if let Some(t) = threshold {
        for v in data.iter_mut() {
            if 10.0 * v.log10() <= t {
                *v = 0.0;
            }
        }
    }
    let nb = cf.len();
    let outs = vec![Value::Num(Matrix::from_dims(vec![nb, ch], data)), real_col(cf)];
    out(outs.into_iter().take(nargout.max(1)).collect())
}
