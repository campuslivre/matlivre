//! `stft` and `istft`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["stft", "istft"];

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if name == "stft" { stft(it, a, nargout) } else { istft(it, a, nargout) }
}

const STFT_PARAMS: &[&str] = &["Window", "OverlapLength", "FFTLength", "FrequencyRange", "OutputTimeDimension"];
const ISTFT_PARAMS: &[&str] = &["Window", "OverlapLength", "FFTLength", "Method", "ConjugateSymmetric", "FrequencyRange", "InputTimeDimension"];

struct Opts {
    fs: Option<f64>,
    vals: Vec<Option<Value>>,
}

fn parse_opts(fname: &str, a: &[Value], params: &[&str]) -> R<Opts> {
    let mut fs = None;
    let mut i = 1;
    if let Some(Value::Num(m)) = a.get(1) {
        if m.kind != Kind::Char {
            if m.numel() != 1 {
                return rt("Optional value-only input can contain time information in the form of a scalar sample rate or a scalar sample time duration.");
            }
            if !(m.data[0] > 0.0) {
                return rt("Expected sample rate to be positive.");
            }
            fs = Some(m.data[0]);
            i = 2;
        }
    }
    let mut vals: Vec<Option<Value>> = vec![None; params.len()];
    while i < a.len() {
        let name = match &a[i] {
            Value::Num(m) if m.kind == Kind::Char => a[i].text().unwrap_or_default(),
            Value::Str(_) => a[i].text().unwrap_or_default(),
            _ => return rt("Expected a string scalar or character vector for the parameter name."),
        };
        let l = name.to_lowercase();
        let mut hits: Vec<usize> = (0..params.len()).filter(|&k| params[k].to_lowercase() == l).collect();
        if hits.is_empty() && !l.is_empty() {
            hits = (0..params.len()).filter(|&k| params[k].to_lowercase().starts_with(&l)).collect();
        }
        if hits.len() != 1 {
            return rt(format!("'{name}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for {fname}."));
        }
        if i + 1 >= a.len() {
            return rt(format!("No value was given for '{name}'. Name-value pair arguments require a name followed by a value."));
        }
        vals[hits[0]] = Some(a[i + 1].clone());
        i += 2;
    }
    Ok(Opts { fs, vals })
}

fn choice(v: &Value, label: &str, options: &[&str]) -> R<usize> {
    let list = options.iter().map(|o| format!("'{o}'")).collect::<Vec<_>>().join(", ");
    let is_text = matches!(v, Value::Num(m) if m.kind == Kind::Char) || matches!(v, Value::Str(_));
    if !is_text {
        return rt(format!("Expected {label} to match one of these values:  {list}  The input did not match any of the valid values."));
    }
    let s = v.text().unwrap_or_default();
    let l = s.to_lowercase();
    match options.iter().position(|o| !l.is_empty() && o.starts_with(&l)) {
        Some(i) => Ok(i),
        None => rt(format!("Expected {label} to match one of these values:  {list}  The input, '{s}', did not match any of the valid values.")),
    }
}

struct Common {
    win: Vec<f64>,
    ov: usize,
    nfft: usize,
}

fn common(it: &mut Interpreter, o: &Opts) -> R<Common> {
    let win: Vec<f64> = match &o.vals[0] {
        Some(Value::Num(m)) if m.kind != Kind::Char => {
            if m.numel() == 0 {
                return rt("Expected Window to be nonempty.");
            }
            if m.dims.len() > 2 || (m.dims[0] != 1 && m.dims[1] != 1) {
                return rt("Expected Window to be a vector.");
            }
            m.data.clone()
        }
        Some(_) => return rt("Expected Window to be one of these types:  double, single"),
        None => {
            let w = it.call_function("hann", vec![scalar_value(128.0), Value::chars("periodic")], 1)?.remove(0).to_matrix()?;
            w.data.clone()
        }
    };
    let nw = win.len();
    let ov = match &o.vals[1] {
        Some(Value::Num(m)) => {
            if m.numel() != 1 {
                return rt("Expected OverlapLength to be a scalar.");
            }
            let v = m.data[0];
            if v < 0.0 {
                return rt("Expected OverlapLength to be nonnegative.");
            }
            if v.fract() != 0.0 {
                return rt("Expected OverlapLength to be integer-valued.");
            }
            if v >= nw as f64 {
                return rt(format!("Expected OverlapLength to be a scalar with value < {nw}."));
            }
            v as usize
        }
        Some(_) => return rt("Expected OverlapLength to be numeric."),
        None => (0.75 * nw as f64).floor() as usize,
    };
    let nfft = match &o.vals[2] {
        Some(Value::Num(m)) => {
            if m.numel() != 1 {
                return rt(format!("Expected FFTLength to be a scalar with value >= {nw}."));
            }
            let v = m.data[0];
            if v.fract() != 0.0 {
                return rt("Expected FFTLength to be integer-valued.");
            }
            if v < nw as f64 {
                return rt(format!("Expected FFTLength to be a scalar with value >= {nw}."));
            }
            v as usize
        }
        Some(_) => return rt("Expected FFTLength to be numeric."),
        None => nw,
    };
    Ok(Common { win, ov, nfft })
}

fn mk(dims: Vec<usize>, re: Vec<f64>, im: Option<Vec<f64>>, single: bool) -> Value {
    let mut m = match im {
        Some(im) if im.iter().any(|v| *v != 0.0) => Matrix::complex(dims, re, im),
        _ => Matrix::from_dims(dims, re),
    };
    if single {
        m.data.iter_mut().for_each(|e| *e = *e as f32 as f64);
        if let Some(i) = m.im.as_mut() {
            i.iter_mut().for_each(|e| *e = *e as f32 as f64);
        }
        m.kind = Kind::Single;
    }
    Value::Num(m)
}

/// Bin index (in FFT order) of row `r` for each frequency range; also the frequency index k.
fn row_bins(range: usize, n: usize) -> Vec<i64> {
    match range {
        0 => (0..n as i64).collect(),                        // twosided
        1 => (0..=(n / 2) as i64).collect(),                 // onesided
        _ => {
            let start = if n % 2 == 0 { -(n as i64 / 2 - 1) } else { -((n as i64 - 1) / 2) };
            (0..n as i64).map(|r| start + r).collect()
        }
    }
}

fn stft(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        _ => return rt("Input signal data type must be double, single, or timetable."),
    };
    if x.numel() == 0 {
        return rt("Expected X to be nonempty.");
    }
    let o = parse_opts("stft", a, STFT_PARAMS)?;
    let single = x.kind == Kind::Single;
    let (rows, cols) = (x.dims[0], x.dims[1]);
    let (len, nch) = if rows == 1 { (cols, 1) } else { (rows, cols) };
    if len < 2 {
        return rt("Input signal must have at least 2 samples.");
    }
    let c = common(it, &o)?;
    let range = match &o.vals[3] {
        Some(v) => choice(v, "FrequencyRange", &["twosided", "onesided", "centered"])?,
        None => 2,
    };
    let down = match &o.vals[4] {
        Some(v) => choice(v, "OutputTimeDimension", &["acrosscolumns", "downrows"])? == 1,
        None => false,
    };
    let nw = c.win.len();
    if len < nw {
        return rt(format!("Input signal length must be greater than or equal to the window length, {nw}."));
    }
    if range == 1 && x.im.is_some() {
        return rt("Input signal must be a real-valued vector, matrix, or timetable when 'FrequencyRange' is specified as 'onesided'.");
    }
    let hop = nw - c.ov;
    let nseg = (len - c.ov) / hop;
    let bins = row_bins(range, c.nfft);
    let nrow = bins.len();
    let xim = x.im.clone().unwrap_or_else(|| vec![0.0; x.numel()]);
    let mut out_re = vec![0.0; nrow * nseg * nch];
    let mut out_im = vec![0.0; nrow * nseg * nch];
    for ch in 0..nch {
        let mut re = vec![0.0; nw * nseg];
        let mut im = vec![0.0; nw * nseg];
        for k in 0..nseg {
            for j in 0..nw {
                re[j + nw * k] = x.data[ch * len + k * hop + j] * c.win[j];
                im[j + nw * k] = xim[ch * len + k * hop + j] * c.win[j];
            }
        }
        let seg = if x.im.is_some() { Value::Num(Matrix::complex(vec![nw, nseg], re, im)) } else { Value::Num(Matrix::from_dims(vec![nw, nseg], re)) };
        let f = it.call_function("fft", vec![seg, scalar_value(c.nfft as f64)], 1)?.remove(0).to_matrix()?;
        let fim = f.im.clone().unwrap_or_else(|| vec![0.0; f.numel()]);
        for k in 0..nseg {
            for (r, b) in bins.iter().enumerate() {
                let idx = b.rem_euclid(c.nfft as i64) as usize;
                let (vr, vi) = (f.data[idx + c.nfft * k], fim[idx + c.nfft * k]);
                if down {
                    out_re[k + nseg * (r + nrow * ch)] = vr;
                    out_im[k + nseg * (r + nrow * ch)] = vi;
                } else {
                    out_re[r + nrow * (k + nseg * ch)] = vr;
                    out_im[r + nrow * (k + nseg * ch)] = vi;
                }
            }
        }
    }
    let dims = if down {
        if nch == 1 { vec![nseg, nrow] } else { vec![nseg, nrow, nch] }
    } else if nch == 1 {
        vec![nrow, nseg]
    } else {
        vec![nrow, nseg, nch]
    };
    let mut outs = vec![mk(dims, out_re, Some(out_im), single)];
    if nargout > 1 {
        let fsc = o.fs;
        let fv: Vec<f64> = bins.iter().map(|&b| match fsc { Some(fs) => fs * b as f64 / c.nfft as f64, None => 2.0 * std::f64::consts::PI * b as f64 / c.nfft as f64 }).collect();
        outs.push(mk(vec![nrow, 1], fv, None, single));
    }
    if nargout > 2 {
        let tv: Vec<f64> = (0..nseg).map(|k| (nw as f64 / 2.0 + (hop * k) as f64) / o.fs.unwrap_or(1.0)).collect();
        outs.push(mk(vec![nseg, 1], tv, None, single));
    }
    out(outs)
}

fn istft(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let s = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        _ => return rt("Input data type must be a matrix or an N-D array with double or single precision."),
    };
    let o = parse_opts("istft", a, ISTFT_PARAMS)?;
    let single = s.kind == Kind::Single;
    let c = common(it, &o)?;
    let wola = match &o.vals[3] {
        Some(v) => choice(v, "Method", &["ola", "wola"])? == 1,
        None => true,
    };
    let range = match &o.vals[5] {
        Some(v) => choice(v, "FrequencyRange", &["twosided", "onesided", "centered"])?,
        None => 2,
    };
    let down = match &o.vals[6] {
        Some(v) => choice(v, "InputTimeDimension", &["acrosscolumns", "downrows"])? == 1,
        None => false,
    };
    let mut conj = match &o.vals[4] {
        Some(Value::Num(m)) if m.numel() == 1 => m.data[0] != 0.0,
        Some(_) => return rt("Expected ConjugateSymmetric to be a scalar."),
        None => false,
    };
    let nw = c.win.len();
    // dimensions: freq x time x ch (or time x freq x ch)
    let d0 = s.dims[0];
    let d1 = s.dims.get(1).copied().unwrap_or(1);
    let nch = s.dims.iter().skip(2).product::<usize>().max(1);
    let (nrow, nseg) = if down { (d1, d0) } else { (d0, d1) };
    if range != 1 && nrow < nw {
        return rt(format!("The number of rows in input S must be greater than or equal to the window length, {nw}."));
    }
    let n = c.nfft;
    let expect = if range == 1 { n / 2 + 1 } else { n };
    if nrow != expect {
        return rt(format!("The number of rows in input S must be equal to the number of frequency samples, {expect}."));
    }
    if range == 1 {
        conj = true;
    }
    let sim = s.im.clone().unwrap_or_else(|| vec![0.0; s.numel()]);
    let bins = row_bins(range, n);
    let hop = nw - c.ov;
    let total = (nseg.max(1) - 1) * hop + nw;
    let mut x_re = vec![0.0; total * nch];
    let mut x_im = vec![0.0; total * nch];
    let mut complex_out = false;
    for ch in 0..nch {
        // assemble the full spectrum of every segment in FFT order
        let mut fr = vec![0.0; n * nseg];
        let mut fi = vec![0.0; n * nseg];
        for k in 0..nseg {
            for (r, b) in bins.iter().enumerate() {
                let src = if down { k + nseg * (r + nrow * ch) } else { r + nrow * (k + nseg * ch) };
                let idx = b.rem_euclid(n as i64) as usize;
                fr[idx + n * k] = s.data[src];
                fi[idx + n * k] = sim[src];
            }
            if range == 1 || conj {
                for kk in 1..n {
                    let mirror = n - kk;
                    if kk > mirror {
                        let (vr, vi) = (fr[mirror + n * k], fi[mirror + n * k]);
                        fr[kk + n * k] = vr;
                        fi[kk + n * k] = -vi;
                    }
                }
                // DC (and Nyquist) bins must be real for the symmetric inverse
                fi[n * k] = 0.0;
                if n % 2 == 0 {
                    fi[n / 2 + n * k] = 0.0;
                }
            }
        }
        let spec = Value::Num(Matrix::complex(vec![n, nseg], fr, fi));
        let inv = it.call_function("ifft", vec![spec], 1)?.remove(0).to_matrix()?;
        let iim = inv.im.clone().unwrap_or_else(|| vec![0.0; inv.numel()]);
        if !conj && iim.iter().any(|v| v.abs() > 1e-10) {
            complex_out = true;
        }
        let mut den = vec![0.0; total];
        for k in 0..nseg {
            for j in 0..nw {
                let pos = k * hop + j;
                let wv = c.win[j];
                if wola {
                    x_re[ch * total + pos] += inv.data[j + n * k] * wv;
                    x_im[ch * total + pos] += iim[j + n * k] * wv;
                    den[pos] += wv * wv;
                } else {
                    x_re[ch * total + pos] += inv.data[j + n * k];
                    x_im[ch * total + pos] += iim[j + n * k];
                    den[pos] += wv;
                }
            }
        }
        for pos in 0..total {
            if den[pos] != 0.0 {
                x_re[ch * total + pos] /= den[pos];
                x_im[ch * total + pos] /= den[pos];
            } else {
                x_re[ch * total + pos] = 0.0;
                x_im[ch * total + pos] = 0.0;
            }
        }
    }
    let dims = vec![total, nch];
    let xo = mk(dims, x_re, if complex_out { Some(x_im) } else { None }, single);
    let mut outs = vec![xo];
    if nargout > 1 {
        let fs = o.fs.unwrap_or(1.0);
        outs.push(mk(vec![total, 1], (0..total).map(|i| i as f64 / fs).collect(), None, single));
    }
    out(outs)
}
