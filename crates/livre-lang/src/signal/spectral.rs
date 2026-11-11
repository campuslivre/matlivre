//! Nonparametric spectral estimation: `periodogram`, `pwelch`, `mscohere` and `spectrogram`.
//!
//! Real input gives a one-sided estimate by default and complex input a two-sided one;
//! frequencies are in rad/sample, or in Hz when a sample rate is given. Matrix input is processed
//! column by column (except for `spectrogram`, which takes a vector).

use crate::builtins::{m_arg, Ret};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["periodogram", "pwelch", "mscohere", "spectrogram"];

const PI: f64 = std::f64::consts::PI;

#[derive(Clone, Copy, PartialEq)]
enum Sides {
    One,
    Two,
    Centered,
}

#[derive(Default)]
struct Opts {
    window: Option<Vec<f64>>,
    window_len: Option<usize>,
    noverlap: Option<usize>,
    nfft: Option<usize>,
    fs: Option<f64>,
    sides: Option<Sides>,
    power: bool,
}

impl Default for Sides {
    fn default() -> Self {
        Sides::One
    }
}

const BAD_OPTION: &str = "'half', 'onesided', 'whole', 'twosided', 'adapt', 'unity', 'eigen', 'corr', 'ev', 'centered', 'power', 'psd', 'ms', 'reassigned', 'confidencelevel', 'mean', 'maxhold', 'minhold', 'mimo'";

/// Positional arguments after the signal(s): `slots` names them in order ("window",
/// "noverlap", "nfft", "fs"); strings are the range and scaling flags.
fn parse_opts(a: &[Value], start: usize, slots: &[&str], name: &str) -> R<Opts> {
    let mut o = Opts::default();
    let mut next = 0;
    for i in start..a.len() {
        if let Some(s) = text_arg(a, i) {
            match s.as_str() {
                "onesided" | "half" => o.sides = Some(Sides::One),
                "twosided" | "whole" => o.sides = Some(Sides::Two),
                "centered" => o.sides = Some(Sides::Centered),
                "power" => o.power = true,
                "psd" => o.power = false,
                "yaxis" | "xaxis" if name == "spectrogram" => {}
                _ => {
                    let shown = a[i].text().unwrap_or_default();
                    if ["adapt", "unity", "eigen", "corr", "ev", "ms", "reassigned", "confidencelevel", "mean", "maxhold", "minhold", "mimo"].contains(&s.as_str()) {
                        return rt(format!("The '{shown}' option is not supported."));
                    }
                    return rt(format!("Expected input to match one of these values:\n\n{BAD_OPTION}\n\nThe input, '{shown}', did not match any of the valid values."));
                }
            }
            continue;
        }
        let Some(&slot) = slots.get(next) else { return rt("Too many input arguments.") };
        next += 1;
        let m = m_arg(a, i, name)?;
        if m.numel() == 0 {
            // An empty sample rate means 1 Hz.
            if slot == "fs" {
                o.fs = Some(1.0);
            }
            continue;
        }
        match slot {
            "window" => {
                if m.numel() == 1 {
                    let l = m.data[0];
                    if l <= 0.0 {
                        return rt("Expected WINDOW to be positive.");
                    }
                    o.window_len = Some(l.round() as usize);
                } else {
                    o.window = Some(rvec_arg(a, i, name)?);
                }
            }
            "noverlap" => {
                if m.data[0] < 0.0 {
                    return rt("Expected NOVERLAP to be nonnegative.");
                }
                o.noverlap = Some(m.data[0].round() as usize);
            }
            "nfft" => {
                if m.numel() != 1 {
                    return rt("Expected NFFT to be a scalar.");
                }
                if m.data[0] <= 0.0 {
                    return rt("Expected NFFT to be positive.");
                }
                if m.data[0].fract() != 0.0 {
                    return rt("Expected NFFT to be integer-valued.");
                }
                o.nfft = Some(m.data[0] as usize);
            }
            _ => {
                let v = scalar_arg(a, i, name)?;
                if v <= 0.0 {
                    return rt("Expected fs to be positive.");
                }
                o.fs = Some(v);
            }
        }
    }
    Ok(o)
}

/// The signal as columns (a vector is one column): real parts, imaginary parts (if complex).
struct Signal {
    re: Vec<Vec<f64>>,
    im: Option<Vec<Vec<f64>>>,
    len: usize,
}

fn signal_arg(a: &[Value], i: usize, name: &str, matrix_ok: bool) -> R<Signal> {
    let m = m_arg(a, i, name)?;
    if m.numel() == 0 {
        return rt("Expected X to be nonempty.");
    }
    if m.dims.len() > 2 {
        return rt("Expected X to be a vector or a matrix.");
    }
    let vector = m.dims[0] == 1 || m.dims[1] == 1;
    if !vector && !matrix_ok {
        return rt("Expected X to be a vector.");
    }
    let split = |data: &[f64]| -> Vec<Vec<f64>> {
        if vector { vec![data.to_vec()] } else { (0..m.dims[1]).map(|c| data[c * m.dims[0]..(c + 1) * m.dims[0]].to_vec()).collect() }
    };
    let complex = m.im.as_ref().is_some_and(|im| im.iter().any(|&v| v != 0.0));
    Ok(Signal { re: split(&m.data), im: if complex { m.im.as_ref().map(|im| split(im)) } else { None }, len: if vector { m.numel() } else { m.dims[0] } })
}

fn hamming(l: usize) -> Vec<f64> {
    if l == 1 {
        return vec![1.0];
    }
    let mut w = vec![0.0; l];
    for k in 0..=(l - 1) / 2 {
        let v = 0.54 - 0.46 * (2.0 * PI * k as f64 / (l - 1) as f64).cos();
        w[k] = v;
        w[l - 1 - k] = v;
    }
    w
}

/// FFTs (length `nfft`) of the windowed segments: `result[s]` is the spectrum of segment s.
fn segment_spectra(it: &mut Interpreter, xr: &[f64], xi: Option<&[f64]>, win: &[f64], noverlap: usize, nfft: usize) -> R<Vec<Vec<(f64, f64)>>> {
    let l = win.len();
    let step = l - noverlap;
    let k = (xr.len() - noverlap) / step;
    let mut re = vec![0.0; nfft * k];
    let mut im = vec![0.0; nfft * k];
    for s in 0..k {
        for j in 0..l {
            // A segment longer than nfft is wrapped around (summed modulo nfft).
            re[s * nfft + j % nfft] += xr[s * step + j] * win[j];
            if let Some(xi) = xi {
                im[s * nfft + j % nfft] += xi[s * step + j] * win[j];
            }
        }
    }
    let m = if xi.is_some() { Matrix::complex(vec![nfft, k], re, im) } else { Matrix::new(nfft, k, re) };
    match it.call_function("fft", vec![Value::Num(m)], 1)?.into_iter().next() {
        Some(Value::Num(f)) => {
            let imv = f.im.clone().unwrap_or_else(|| vec![0.0; f.data.len()]);
            Ok((0..k).map(|s| (0..nfft).map(|i| (f.data[s * nfft + i], imv[s * nfft + i])).collect()).collect())
        }
        _ => Err(LangError::new("fft returned an unexpected value.", 0)),
    }
}

/// Window, overlap and FFT length with the function-specific defaults applied.
struct Plan {
    win: Vec<f64>,
    noverlap: usize,
    nfft: usize,
}

fn plan(o: &Opts, n: usize, welch: bool) -> R<Plan> {
    let win = if let Some(w) = &o.window {
        w.clone()
    } else if let Some(l) = o.window_len {
        hamming(l)
    } else if welch {
        // As close to, but not more than, 8 segments with 50% overlap.
        let l = (n as f64 / 4.5).floor() as usize;
        if l < 2 {
            return rt("The input record needs more than eight samples in order to use the default window and overlap.");
        }
        hamming(l)
    } else {
        vec![1.0; n]
    };
    let l = win.len();
    if l > n {
        return rt("The length of the segments cannot be greater than the length of the input signal.");
    }
    let noverlap = if welch { o.noverlap.unwrap_or(l / 2) } else { 0 };
    if noverlap >= l {
        return rt("The number of samples to overlap must be less than the length of the segments.");
    }
    let nfft = o.nfft.unwrap_or_else(|| 256.max(l.next_power_of_two()));
    Ok(Plan { win, noverlap, nfft })
}

/// Which FFT bins are reported, in order, and the matching frequency vector.
struct Range {
    bins: Vec<usize>,
    freqs: Vec<f64>,
    onesided: bool,
}

fn range(o: &Opts, nfft: usize, complex: bool) -> R<Range> {
    let sides = o.sides.unwrap_or(if complex { Sides::Two } else { Sides::One });
    if complex && sides == Sides::One {
        return rt("A 'onesided' PSD cannot be computed for a complex signal.");
    }
    let unit = o.fs.unwrap_or(2.0 * PI) / nfft as f64;
    let (bins, ks): (Vec<usize>, Vec<f64>) = match sides {
        Sides::One => {
            let npts = if nfft % 2 == 0 { nfft / 2 + 1 } else { nfft.div_ceil(2) };
            ((0..npts).collect(), (0..npts).map(|i| i as f64).collect())
        }
        Sides::Two => ((0..nfft).collect(), (0..nfft).map(|i| i as f64).collect()),
        Sides::Centered => {
            let lo = -((nfft.div_ceil(2) as i64) - 1);
            let hi = (nfft / 2) as i64;
            ((lo..=hi).map(|k| k.rem_euclid(nfft as i64) as usize).collect(), (lo..=hi).map(|k| k as f64).collect())
        }
    };
    Ok(Range { bins, freqs: ks.into_iter().map(|k| k * unit).collect(), onesided: sides == Sides::One })
}

fn double_one_sided(p: &mut [f64], nfft: usize) {
    let last = p.len() - 1;
    for (i, v) in p.iter_mut().enumerate() {
        if i != 0 && !(nfft % 2 == 0 && i == last) {
            *v *= 2.0;
        }
    }
}

fn matrix_of(columns: Vec<Vec<f64>>) -> Matrix {
    let rows = columns.first().map_or(0, |c| c.len());
    let n = columns.len();
    Matrix::new(rows, n, columns.into_iter().flatten().collect())
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout == 0 {
        return rt(format!("{name} without output arguments plots the estimate, which is not supported; request outputs."));
    }
    if name == "mscohere" {
        return mscohere(it, a, nargout);
    }
    if name == "spectrogram" {
        return spectrogram(it, a, nargout);
    }
    let welch = name == "pwelch";
    let x = signal_arg(a, 0, name, true)?;
    let slots: &[&str] = if welch { &["window", "noverlap", "nfft", "fs"] } else { &["window", "nfft", "fs"] };
    let o = parse_opts(a, 1, slots, name)?;
    if !welch && (o.window.as_ref().is_some_and(|w| w.len() != x.len) || o.window_len.is_some_and(|l| l != x.len)) {
        return rt("The window must be a vector of the same length as the signal.");
    }
    let pl = plan(&o, x.len, welch)?;
    let rg = range(&o, pl.nfft, x.im.is_some())?;
    let scale = if o.power {
        pl.win.iter().sum::<f64>().powi(2)
    } else {
        o.fs.unwrap_or(2.0 * PI) * pl.win.iter().map(|w| w * w).sum::<f64>()
    };
    let mut columns = Vec::new();
    for c in 0..x.re.len() {
        let xi = x.im.as_ref().map(|im| im[c].as_slice());
        let spectra = segment_spectra(it, &x.re[c], xi, &pl.win, pl.noverlap, pl.nfft)?;
        let k = spectra.len() as f64;
        let mut p: Vec<f64> = rg.bins.iter().map(|&i| spectra.iter().map(|s| s[i].0 * s[i].0 + s[i].1 * s[i].1).sum::<f64>() / k / scale).collect();
        if rg.onesided {
            double_one_sided(&mut p, pl.nfft);
        }
        columns.push(p);
    }
    let mut outs = vec![Value::Num(matrix_of(columns))];
    if nargout >= 2 {
        outs.push(real_col(rg.freqs));
    }
    out(outs)
}

fn mscohere(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    let x = signal_arg(a, 0, "mscohere", true)?;
    let y = signal_arg(a, 1, "mscohere", true)?;
    if x.len != y.len || x.re.len() != y.re.len() {
        return rt("The number of rows of the two inputs must be equal to calculate the cross spectral density.");
    }
    let o = parse_opts(a, 2, &["window", "noverlap", "nfft", "fs"], "mscohere")?;
    let pl = plan(&o, x.len, true)?;
    let rg = range(&o, pl.nfft, x.im.is_some() || y.im.is_some())?;
    let mut columns = Vec::new();
    for c in 0..x.re.len() {
        let xi = x.im.as_ref().map(|im| im[c].as_slice());
        let yi = y.im.as_ref().map(|im| im[c].as_slice());
        let sx = segment_spectra(it, &x.re[c], xi, &pl.win, pl.noverlap, pl.nfft)?;
        let sy = segment_spectra(it, &y.re[c], yi, &pl.win, pl.noverlap, pl.nfft)?;
        let col: Vec<f64> = rg
            .bins
            .iter()
            .map(|&i| {
                let (mut pxx, mut pyy, mut re, mut im) = (0.0, 0.0, 0.0, 0.0);
                for s in 0..sx.len() {
                    let (xr, xim) = sx[s][i];
                    let (yr, yim) = sy[s][i];
                    pxx += xr * xr + xim * xim;
                    pyy += yr * yr + yim * yim;
                    // X conj(Y)
                    re += xr * yr + xim * yim;
                    im += xim * yr - xr * yim;
                }
                (re * re + im * im) / (pxx * pyy)
            })
            .collect();
        columns.push(col);
    }
    let mut outs = vec![Value::Num(matrix_of(columns))];
    if nargout >= 2 {
        outs.push(real_col(rg.freqs));
    }
    out(outs)
}

/// Short-time Fourier transform: `[s, w, t, ps] = spectrogram(x, window, noverlap, nfft, fs)`.
/// `s` is the plain FFT of each windowed segment (frequency down the rows, time across the
/// columns); `ps` is the power spectral density (or power with `"power"`), one-sided for real
/// input.
fn spectrogram(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let x = signal_arg(a, 0, "spectrogram", false)?;
    let o = parse_opts(a, 1, &["window", "noverlap", "nfft", "fs"], "spectrogram")?;
    let pl = plan(&o, x.len, true)?;
    let rg = range(&o, pl.nfft, x.im.is_some())?;
    let xi = x.im.as_ref().map(|im| im[0].as_slice());
    let spectra = segment_spectra(it, &x.re[0], xi, &pl.win, pl.noverlap, pl.nfft)?;
    let npts = rg.bins.len();
    let k = spectra.len();
    let (mut re, mut im) = (Vec::with_capacity(npts * k), Vec::with_capacity(npts * k));
    for seg in &spectra {
        for &b in &rg.bins {
            re.push(seg[b].0);
            im.push(seg[b].1);
        }
    }
    let rate = o.fs.unwrap_or(2.0 * PI);
    let step = pl.win.len() - pl.noverlap;
    let t: Vec<f64> = (0..k).map(|s| (s * step) as f64 + pl.win.len() as f64 / 2.0).map(|v| v / rate).collect();
    let mut outs = vec![Value::Num(Matrix::complex(vec![npts, k], re.clone(), im.clone()))];
    if nargout >= 2 {
        outs.push(real_col(rg.freqs.clone()));
    }
    if nargout >= 3 {
        outs.push(real_row(t));
    }
    if nargout >= 4 {
        let scale = if o.power { pl.win.iter().sum::<f64>().powi(2) } else { rate * pl.win.iter().map(|w| w * w).sum::<f64>() };
        let mut ps = vec![0.0; npts * k];
        for s in 0..k {
            let mut col: Vec<f64> = (0..npts).map(|i| (re[s * npts + i].powi(2) + im[s * npts + i].powi(2)) / scale).collect();
            if rg.onesided {
                double_one_sided(&mut col, pl.nfft);
            }
            ps[s * npts..(s + 1) * npts].copy_from_slice(&col);
        }
        outs.push(Value::Num(Matrix::new(npts, k, ps)));
    }
    out(outs)
}
