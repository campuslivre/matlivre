//! `plomb`: the Lomb-Scargle periodogram of unevenly sampled data.

use crate::builtins::Ret;
use crate::error::rt;
use crate::signal::poly::*;
use crate::value::*;
use std::f64::consts::PI;

pub const NAMES: &[&str] = &["plomb"];

#[derive(Clone, Copy, PartialEq)]
enum Scale {
    Psd,
    Power,
    Normalized,
}

fn is_char(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.kind == Kind::Char)
}

/// Lomb-Scargle sums `(sum a^2/c^2 ...)` at one frequency (hertz), for mean-removed `x`.
fn lomb_at(t: &[f64], x: &[f64], f: f64) -> f64 {
    let w = 2.0 * PI * f;
    let (mut s2, mut c2) = (0.0, 0.0);
    for &tk in t {
        s2 += (2.0 * w * tk).sin();
        c2 += (2.0 * w * tk).cos();
    }
    let tau = s2.atan2(c2) / (2.0 * w);
    let (mut xc, mut xs, mut cc, mut ss) = (0.0, 0.0, 0.0, 0.0);
    for (k, &tk) in t.iter().enumerate() {
        let (c, s) = ((w * (tk - tau)).cos(), (w * (tk - tau)).sin());
        xc += x[k] * c;
        xs += x[k] * s;
        cc += c * c;
        ss += s * s;
    }
    0.5 * (xc * xc / cc + xs * xs / ss)
}

/// Adds `val` at the fractional grid coordinate `g` (0-based) of `grid` by extirpolation: the
/// weights are the Lagrange basis polynomials of the four grid nodes around `g`, so that
/// interpolating the grid back at `g` gives `val`.
fn extirpolate(grid: &mut [f64], g: f64, val: f64) {
    let m = grid.len();
    let first = ((g.floor() as i64) - 1).clamp(0, m as i64 - 4) as usize;
    for i in 0..4 {
        let mut w = 1.0;
        for k in 0..4 {
            if k != i {
                w *= (g - (first + k) as f64) / (i as f64 - k as f64);
            }
        }
        grid[first + i] += val * w;
    }
}

/// In-place radix-2 FFT with kernel exp(+i 2 pi jk / n); the length is a power of two.
fn fft_pos(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = 2.0 * PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = ((ang * k as f64).cos(), (ang * k as f64).sin());
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

/// The Lomb-Scargle spectrum normalised by twice the variance at `j / period`, `j = 1..=nout`,
/// from the data sums sum y cos, sum y sin (at w) and sum cos, sum sin (at 2 w) that are
/// evaluated for all frequencies at once by spreading the samples on a regular grid and taking
/// FFTs (Press and Rybicki, 1989).
fn fast_lomb(t: &[f64], x: &[f64], oversample: f64, period: f64, nout: usize, hifac: f64) -> Vec<f64> {
    let n = x.len();
    let nf = n as f64;
    let mean = x.iter().sum::<f64>() / nf;
    let var = x.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (nf - 1.0);
    if var == 0.0 {
        return vec![0.0; nout];
    }
    // Grid size: a power of two, at least 4 points per frequency of interest on each of the
    // two halves of the spectrum, and never fewer than 128.
    let want = oversample * hifac * nf * 4.0;
    let mut half = 64usize;
    while (half as f64) < want {
        half <<= 1;
    }
    let size = half * 2;
    let sz = size as f64;
    let scale = sz / period;
    let mut data_grid = vec![0.0; size];
    let mut pair_grid = vec![0.0; size];
    for k in 0..n {
        let g = (t[k] - t[0]) * scale % sz;
        extirpolate(&mut data_grid, g, x[k] - mean);
        extirpolate(&mut pair_grid, (2.0 * g) % sz, 1.0);
    }
    let (mut dr, mut di) = (data_grid, vec![0.0; size]);
    let (mut pr, mut pi_) = (pair_grid, vec![0.0; size]);
    fft_pos(&mut dr, &mut di);
    fft_pos(&mut pr, &mut pi_);
    (1..=nout)
        .map(|j| {
            // c = sum cos(2 w t), s = sum sin(2 w t): the phase of the pair sum gives the
            // offset tau with tan(2 w tau) = s / c.
            let (c, s) = (pr[j], pi_[j]);
            let r = c.hypot(s);
            let (cos2, sin2) = if r == 0.0 { (0.0, 0.0) } else { (c / r, s / r) };
            let cos_wtau = ((1.0 + cos2) / 2.0).sqrt();
            let sin_wtau = ((1.0 - cos2) / 2.0).sqrt().copysign(sin2);
            let (xc, xs) = (dr[j], di[j]);
            let a_cos = xc * cos_wtau + xs * sin_wtau;
            let a_sin = xs * cos_wtau - xc * sin_wtau;
            let den_cos = 0.5 * nf + 0.5 * r;
            let den_sin = 0.5 * nf - 0.5 * r;
            0.5 * (a_cos * a_cos / den_cos + a_sin * a_sin / den_sin) / var
        })
        .collect()
}

pub fn call(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 3 {
        return rt("One or more output arguments not assigned during call to \"plomb\".");
    }
    let x = match &a[0] {
        Value::Num(m) if m.kind != Kind::Char && m.kind != Kind::Logical => m.clone(),
        Value::Num(_) => return rt("Expected input number 1, x, to be one of these types:  double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64"),
        v => return rt(format!("Expected input number 1, x, to be one of these types:  double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64  Instead its type was {}.", v.class_name())),
    };
    if x.im.is_some() {
        return rt("Expected input number 1, x, to be real.");
    }
    let single = x.kind == Kind::Single;
    let vector = x.dims[0] == 1 || x.dims[1] == 1;
    let (n_rows, ch) = if vector { (x.numel(), 1) } else { (x.dims[0], x.dims[1]) };
    let empty_t = Value::Num(Matrix::from_dims(vec![0, 0], vec![]));
    let a: Vec<Value> = if a.len() < 2 { vec![a[0].clone(), empty_t] } else { a.to_vec() };
    let a = &a[..];
    // Time base.
    let mut normalized_freq = false;
    let mut fs_given = false;
    let tvals: Vec<f64> = match &a[1] {
        Value::Num(m) if m.kind != Kind::Char && m.numel() == 0 => {
            normalized_freq = true;
            (0..n_rows).map(|k| k as f64).collect()
        }
        Value::Num(m) if m.kind != Kind::Char && m.numel() == 1 => {
            if !(m.data[0] > 0.0) {
                return rt("Expected input number 2, Fs, to be an array with all of the values > 0.");
            }
            fs_given = true;
            (0..n_rows).map(|k| k as f64 / m.data[0]).collect()
        }
        Value::Num(m) if m.kind != Kind::Char => {
            if m.numel() != n_rows {
                return rt("The signal (X) and time vector (T) should have same lengths.");
            }
            let t = m.data.clone();
            let valid: Vec<f64> = t.iter().cloned().filter(|v| !v.is_nan()).collect();
            if valid.windows(2).any(|w| w[1] <= w[0]) {
                return rt("Expected input number 2, T, to be strictly increasing.");
            }
            t
        }
        _ => return rt("Conversion to double from cell is not possible."),
    };
    let _ = fs_given;
    // Remaining arguments.
    let mut fmax: Option<f64> = None;
    let mut ofac: Option<f64> = None;
    let mut fvec: Option<Vec<f64>> = None;
    let mut scale: Option<Scale> = None;
    let mut pd: Option<Vec<f64>> = None;
    let mut numeric_pos = 0;
    let mut i = 2;
    while i < a.len() {
        match &a[i] {
            v if is_char(v) => {
                let s = v.text().unwrap_or_default();
                match s.to_lowercase().as_str() {
                    "psd" => scale = Some(Scale::Psd),
                    "power" => scale = Some(Scale::Power),
                    "normalized" => scale = Some(Scale::Normalized),
                    "pd" => {
                        i += 1;
                        let Some(v) = a.get(i) else { return rt("Pd values are missing.") };
                        match v {
                            Value::Num(m) if m.kind != Kind::Char => {
                                if m.data.iter().any(|p| *p <= 0.0) {
                                    return rt("Expected Pd to be positive.");
                                }
                                if m.data.iter().any(|p| *p > 1.0) {
                                    return rt("Expected Pd to be an array with all of the values <= 1.");
                                }
                                pd = Some(m.data.clone());
                            }
                            other => return rt(format!("{} is an invalid input.", other.text().unwrap_or_default())),
                        }
                    }
                    _ => return rt(format!("{s} is an invalid input.")),
                }
            }
            Value::Num(m) => {
                match numeric_pos {
                    0 => {
                        if m.numel() > 1 {
                            if m.data.iter().any(|v| !v.is_finite()) {
                                return rt("Expected input number 3, FVEC, to be finite.");
                            }
                            fvec = Some(m.data.clone());
                        } else if m.numel() == 1 {
                            fmax = Some(m.data[0]);
                        } else if m.numel() == 1 {
                        }
                    }
                    1 => {
                        if m.numel() != 1 {
                            return rt("Expected input number 4, OFAC, to be a scalar.");
                        }
                        if !(m.data[0] > 0.0) {
                            return rt("Expected input number 4, OFAC, to be positive.");
                        }
                        if m.data[0].fract() != 0.0 {
                            return rt("Expected input number 4, OFAC, to be integer-valued.");
                        }
                        if fvec.is_some() {
                            return rt("You cannot specify OFAC when frequency vector is specified.");
                        }
                        ofac = Some(m.data[0]);
                    }
                    _ => return rt(format!("{}  is an ambiguous inputs.", a.len())),
                }
                numeric_pos += 1;
            }
            _ => return rt("Conversion to double from cell is not possible."),
        }
        i += 1;
    }
    if fvec.is_some() && pd.is_some() {
        return rt("Probability of Detection is not available when frequency vector is specified.");
    }
    let scale = scale.unwrap_or(Scale::Psd);
    let ofac_v = ofac.unwrap_or(4.0);
    // Per channel.
    let mut results: Vec<Vec<f64>> = vec![];
    let mut grid: Vec<f64> = vec![];
    let mut pths: Vec<Vec<f64>> = vec![];
    for c in 0..ch {
        let col = &x.data[c * n_rows..(c + 1) * n_rows];
        let mut tt = vec![];
        let mut xx = vec![];
        for k in 0..n_rows {
            if !col[k].is_nan() && !tvals[k].is_nan() {
                tt.push(tvals[k]);
                xx.push(col[k]);
            }
        }
        let n = xx.len();
        if n < 2 {
            return rt("Not enough data points to estimate the spectrum.");
        }
        let ts_full = {
            let all: Vec<f64> = tvals.iter().cloned().filter(|v| !v.is_nan()).collect();
            (all[all.len() - 1] - all[0]) / (n_rows as f64 - 1.0)
        };
        let span = tt[n - 1] - tt[0];
        let ts = span / (n as f64 - 1.0);
        let mean = xx.iter().sum::<f64>() / n as f64;
        let xm: Vec<f64> = xx.iter().map(|v| v - mean).collect();
        let var = xm.iter().map(|v| v * v).sum::<f64>() / (n as f64 - 1.0);
        let to_hz = if normalized_freq { 1.0 / (2.0 * PI) } else { 1.0 };
        let freqs: Vec<f64> = match &fvec {
            Some(f) => f.iter().map(|v| v * to_hz).collect(),
            None => {
                let fm = fmax.map(|v| v * to_hz).unwrap_or(1.0 / (2.0 * ts));
                let fmin = 1.0 / (ofac_v * n as f64 * ts);
                let cnt = (fm / fmin).round();
                if !(cnt >= 1.0) {
                    return rt("FMAX specified is too small for computation.");
                }
                (1..=cnt as usize).map(|k| fmin * k as f64).collect()
            }
        };
        let factor = match scale {
            Scale::Psd => 2.0 * var * ts_full,
            Scale::Power => 2.0 * var / n as f64,
            Scale::Normalized => 1.0,
        };
        let mut p: Vec<f64> = if fvec.is_some() {
            freqs.iter().map(|&f| lomb_at(&tt, &xm, f) / var * factor).collect()
        } else {
            let fm = fmax.map(|v| v * to_hz).unwrap_or(1.0 / (2.0 * ts));
            let hifac = fm * 2.0 * ts;
            fast_lomb(&tt, &xx, ofac_v, ofac_v * n as f64 * ts, freqs.len(), hifac).into_iter().map(|v| v * factor).collect()
        };
        let mut pf = freqs.clone();
        if normalized_freq {
            pf = pf.iter().map(|f| 2.0 * PI * f).collect();
            if scale == Scale::Psd {
                p.iter_mut().for_each(|v| *v /= 2.0 * PI);
            }
        }
        if let Some(pdv) = &pd {
            let hf = fmax.map(|v| v * to_hz).unwrap_or(1.0 / (2.0 * ts)) * 2.0 * ts;
            let m = (n as f64 * hf).round();
            pths.push(pdv.iter().map(|&q| factor * (-(1.0 - q.powf(1.0 / m)).ln())).collect());
        }
        if c == 0 {
            grid = pf;
        }
        results.push(p);
    }
    let nf = grid.len();
    let mut data = vec![0.0; nf * ch];
    for (c, r) in results.iter().enumerate() {
        data[c * nf..(c + 1) * nf].copy_from_slice(r);
    }
    let mut outs = vec![Value::Num(Matrix::from_dims(vec![nf, ch], data)), real_col(grid)];
    if nargout > 2 {
        let np = pd.as_ref().map_or(0, |p| p.len());
        let mut d = vec![0.0; np * ch];
        for (c, r) in pths.iter().enumerate() {
            d[c * np..(c + 1) * np].copy_from_slice(r);
        }
        outs.push(Value::Num(Matrix::from_dims(vec![np, ch], d)));
    }
    out(outs.into_iter().take(nargout.max(1)).collect())
}
