//! `tsa`: the time-synchronous average of a signal from tachometer pulse times.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["tsa"];

pub fn call(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 3 {
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
    let tm = match &a[1] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt("Expected T to be finite."),
    };
    let tpm = match &a[2] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt("Expected TP to be a vector."),
    };
    // Name-value options.
    let mut nrot = 1.0;
    let mut ppr = 1.0;
    let mut rf = 1.0;
    let mut method = "linear".to_string();
    let mut i = 3;
    while i < a.len() {
        let s = a[i].text().unwrap_or_default();
        let low = s.to_lowercase();
        if i + 1 >= a.len() {
            if matches!(low.as_str(), "numrotations" | "pulsesperrotation" | "resamplefactor" | "method") {
                return rt(format!("No value was given for '{s}'. Name-value pair arguments require a name followed by a value."));
            }
            return rt(format!("'{s}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function."));
        }
        let num = |v: &Value| v.scalar().unwrap_or(f64::NAN);
        match low.as_str() {
            "numrotations" => {
                nrot = num(&a[i + 1]);
                if !(nrot > 0.0) {
                    return rt("Expected NumRotations to be positive.");
                }
                if nrot.fract() != 0.0 {
                    return rt("Expected NumRotations to be integer-valued.");
                }
            }
            "pulsesperrotation" => {
                ppr = num(&a[i + 1]);
                if !(ppr > 0.0) {
                    return rt("Expected PulsesPerRotation to be positive.");
                }
            }
            "resamplefactor" => {
                rf = num(&a[i + 1]);
                if !(rf > 0.0) {
                    return rt("Expected ResampleFactor to be positive.");
                }
                if rf.fract() != 0.0 {
                    return rt("Expected ResampleFactor to be integer-valued.");
                }
            }
            "method" => {
                let v = a[i + 1].text().unwrap_or_default();
                match v.to_lowercase().as_str() {
                    "linear" | "spline" | "pchip" | "fft" => method = v.to_lowercase(),
                    _ => return rt(format!("Expected Method to match one of these values:  'linear', 'spline', 'pchip', 'fft'  The input, '{v}', did not match any of the valid values.")),
                }
            }
            _ => return rt(format!("'{s}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function.")),
        }
        i += 2;
    }
    if !(x.dims.len() == 2 && (x.dims[0] == 1 || x.dims[1] == 1)) {
        return rt("Expected X to be a vector.");
    }
    if x.numel() == 0 {
        return rt("Expected X to be nonempty.");
    }
    if x.im.is_some() {
        return rt("Expected X to be real.");
    }
    let n = x.numel();
    // Sample times.
    let times: Vec<f64> = if tm.numel() == 1 {
        let fs = tm.data[0];
        (0..n).map(|k| k as f64 / fs).collect()
    } else {
        if tm.numel() != n {
            return rt(format!("Expected T to be an array with number of elements equal to {n}."));
        }
        tm.data.clone()
    };
    if times.iter().any(|v| !v.is_finite()) {
        return rt("Expected T to be finite.");
    }
    if times.iter().any(|v| *v < 0.0) {
        return rt("Expected T to be nonnegative.");
    }
    let fs = if tm.numel() == 1 { tm.data[0] } else { (n as f64 - 1.0) / (times[n - 1] - times[0]) };
    // Pulses.
    let pulses: Vec<f64> = if tpm.numel() == 1 {
        let step = tpm.data[0];
        if !step.is_finite() {
            return rt("Expected TP to be a vector.");
        }
        if step <= 0.0 {
            return rt("Input signal must contain at least 1 complete revolutions.");
        }
        let mut v = vec![];
        let mut k = 0.0;
        while times[0] + k * step <= times[n - 1] + 1.0 / fs + 1e-9 {
            v.push(times[0] + k * step);
            k += 1.0;
        }
        v
    } else {
        let kept: Vec<f64> = tpm.data.iter().cloned().filter(|v| v.is_finite() && *v >= 0.0).collect();
        if kept.windows(2).any(|w| w[1] <= w[0]) {
            return rt("Expected TP to be strictly increasing.");
        }
        kept
    };
    let inside: Vec<f64> = pulses.iter().cloned().filter(|p| *p >= times[0] - 1e-12 && *p <= times[n - 1] + 1.0 / fs + 1e-9).collect();
    // Pulse time at a (possibly fractional) pulse index.
    let last = inside.len() as f64 - 1.0;
    let pt = |idx: f64| -> f64 {
        let i = (idx.floor() as usize).min(inside.len().saturating_sub(2));
        let f = idx - i as f64;
        inside[i] + f * (inside[(i + 1).min(inside.len() - 1)] - inside[i])
    };
    let nrot_u = nrot as usize;
    let nwin = if inside.len() < 2 { 0 } else { ((last + 1e-9) / (ppr * nrot)).floor() as usize };
    if nwin == 0 {
        return rt(format!("Input signal must contain at least {} complete revolutions.", nrot_u));
    }
    let windows: Vec<usize> = (0..nwin).collect();
    // One rotation lasts as long as the shortest rotation; a window holds `nrot` of them.
    let nrots = (((last + 1e-9) / ppr).floor() as usize).max(1);
    let rot = (0..nrots).map(|j| pt((j as f64 + 1.0) * ppr) - pt(j as f64 * ppr)).fold(f64::INFINITY, f64::min);
    let mut m = ((rot * fs + 0.5 - 1e-9).floor() as usize) * nrot_u;
    let mut period = rot * nrot;
    let mut rpm = 60.0 / rot;
    let mut acc;
    if method == "fft" {
        // Segments of whole samples (pulses rounded to samples), spectra cut to the shortest and averaged.
        let idx = |p: f64| ((p - times[0]) * fs + 0.5).floor() as usize;
        let segs: Vec<Vec<f64>> = windows
            .iter()
            .map(|&w| {
                let (a0, a1) = (pt(w as f64 * ppr * nrot), pt((w as f64 + 1.0) * ppr * nrot));
                x.data[idx(a0)..idx(a1).min(n)].to_vec()
            })
            .collect();
        let mn = segs.iter().map(|s| s.len()).min().unwrap_or(0);
        let mean_win = segs.iter().map(|s| s.len() as f64).sum::<f64>() / segs.len() as f64 / fs;
        period = mean_win;
        rpm = 60.0 / (mean_win / nrot);
        m = mn;
        let mut sr = vec![0.0; mn];
        let mut si = vec![0.0; mn];
        for s in &segs {
            let f = it.call_function("fft", vec![real_col(s.clone())], 1)?.remove(0).to_matrix()?;
            let fi = f.im.clone().unwrap_or_else(|| vec![0.0; s.len()]);
            let l = s.len();
            for k in 0..mn {
                let src = if k <= mn / 2 { k } else { l - (mn - k) };
                sr[k] += f.data[src] * mn as f64 / l as f64;
                si[k] += fi[src] * mn as f64 / l as f64;
            }
        }
        sr.iter_mut().for_each(|v| *v /= segs.len() as f64);
        si.iter_mut().for_each(|v| *v /= segs.len() as f64);
        let spec = Matrix::complex(vec![mn, 1], sr, si);
        let r = it.call_function("ifft", vec![Value::Num(spec)], 1)?.remove(0).to_matrix()?;
        acc = r.data.clone();
        acc.resize(m, 0.0);
    } else {
        // Upsample by the resample factor with Fourier interpolation, then interpolate each rotation.
        let (xs, ts): (Vec<f64>, Vec<f64>) = if rf > 1.0 {
            let up = it.call_function("interpft", vec![real_col(x.data.clone()), scalar_value(n as f64 * rf)], 1)?.remove(0).to_matrix()?;
            (up.data.clone(), (0..up.numel()).map(|k| times[0] + k as f64 / (fs * rf)).collect())
        } else {
            (x.data.clone(), times.clone())
        };
        let xcol = Matrix::from_dims(vec![xs.len(), 1], xs);
        let mrot = m / nrot_u;
        acc = vec![0.0; m];
        for &k in &windows {
            for r in 0..nrot_u {
                let (p0, p1) = (pt((k as f64 * nrot + r as f64) * ppr), pt((k as f64 * nrot + r as f64 + 1.0) * ppr));
                let grid: Vec<f64> = (0..mrot).map(|j| p0 + (j as f64 / mrot as f64) * (p1 - p0)).collect();
                let y = it
                    .call_function("interp1", vec![real_col(ts.clone()), Value::Num(xcol.clone()), real_col(grid), Value::string(&method)], 1)?
                    .remove(0)
                    .to_matrix()?;
                for j in 0..mrot {
                    acc[r * mrot + j] += y.data[j];
                }
            }
        }
        acc.iter_mut().for_each(|v| *v /= windows.len() as f64);
    }
    let rev = nrot;
    let t: Vec<f64> = (0..m).map(|j| j as f64 / m as f64 * period).collect();
    let p: Vec<f64> = (0..m).map(|j| rev * j as f64 / m as f64).collect();
    let mut ta = Matrix::from_dims(vec![m, 1], acc);
    if x.kind == Kind::Single {
        ta.kind = Kind::Single;
    }
    out(vec![Value::Num(ta), real_col(t), real_col(p), scalar_value(rpm)].into_iter().take(nargout.max(1)).collect())
}
