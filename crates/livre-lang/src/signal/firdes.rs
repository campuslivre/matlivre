//! FIR design: `kaiserord`, `fir2`, `firls`.

use crate::builtins::{m_arg, Ret};
use crate::error::rt;
use crate::interp::{Interpreter, OutputKind};
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["kaiserord", "fir2", "firls"];

const PI: f64 = std::f64::consts::PI;

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "kaiserord" => kaiserord(a, nargout),
        "fir2" => fir2(it, a, nargout),
        _ => firls(it, a, nargout),
    }
}

fn hamming(n: usize) -> Vec<f64> {
    (0..n).map(|k| if n == 1 { 1.0 } else { 0.54 - 0.46 * (2.0 * PI * k as f64 / (n - 1) as f64).cos() }).collect()
}

// ---- kaiserord -----------------------------------------------------------------------------

fn kaiserord(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 5 {
        return rt("Too many input arguments.");
    }
    if nargout > 4 {
        return rt("Too many output arguments.");
    }
    let f = m_arg(a, 0, "kaiserord")?.data;
    let am = m_arg(a, 1, "kaiserord")?.data;
    let dev = m_arg(a, 2, "kaiserord")?.data;
    let fs = if a.len() > 3 { m_arg(a, 3, "kaiserord")?.data.first().copied().unwrap_or(2.0) } else { 2.0 };
    if dev.len() != am.len() {
        return rt("Requires A and DEV to be the same length.");
    }
    if f.len() != 2 * am.len() - 2 {
        return rt("Length of F must be 2*length(A)-2.");
    }
    if f.iter().any(|&x| x <= 0.0 || x >= fs / 2.0) {
        return rt("Band edge frequencies must be between 0 and half the sampling frequency Fs.");
    }
    if f.windows(2).any(|p| p[1] <= p[0]) {
        return rt("Bandedges must be strictly increasing.");
    }
    if dev.iter().any(|&d| d <= 0.0) {
        return rt("Argument must be integer-valued.");
    }
    if am.windows(2).any(|p| p[0] != 0.0 && p[1] != 0.0) {
        return rt("Stopbands must be zero.");
    }
    let rel = dev.iter().zip(&am).map(|(&d, &g)| if g != 0.0 { d / g.abs() } else { d }).fold(f64::INFINITY, f64::min);
    let att = -20.0 * rel.log10();
    let beta = if att > 50.0 {
        0.1102 * (att - 8.7)
    } else if att >= 21.0 {
        0.5842 * (att - 21.0).powf(0.4) + 0.07886 * (att - 21.0)
    } else {
        0.0
    };
    let widths: Vec<f64> = (0..am.len() - 1).map(|i| f[2 * i + 1] - f[2 * i]).collect::<Vec<_>>();
    let _ = widths;
    let trans: Vec<f64> = (0..am.len() - 1).map(|i| f[2 * i + 1] - f[2 * i]).collect();
    // The transitions lie between consecutive band pairs: f(2i), f(2i+1) (0-based).
    let tw = trans.iter().copied().fold(f64::INFINITY, f64::min);
    let mut n = ((att - 7.95) / (2.285 * 2.0 * PI * tw / fs)).ceil();
    let wn: Vec<f64> = (0..am.len() - 1).map(|i| (f[2 * i] + f[2 * i + 1]) / 2.0 / (fs / 2.0)).collect();
    let ft = if am[0] == 0.0 {
        if am.len() == 2 { "high" } else { "DC-0" }
    } else if am.len() == 2 {
        "low"
    } else if am.len() == 3 {
        "stop"
    } else {
        "DC-1"
    };
    if am[am.len() - 1] != 0.0 && ft != "low" && n % 2.0 != 0.0 {
        n += 1.0;
    }
    let wnv = if wn.len() == 1 { scalar_value(wn[0]) } else { real_col(wn) };
    let mut outv = vec![scalar_value(n), wnv, scalar_value(beta), Value::chars(ft)];
    outv.truncate(nargout.max(1));
    out(outv)
}

// ---- fir2 ----------------------------------------------------------------------------------

fn order_arg(a: &[Value]) -> R<usize> {
    let m = m_arg(a, 0, "fir2")?;
    if m.numel() != 1 || m.data[0] < 1.0 || m.data[0].fract() != 0.0 || m.im.is_some() {
        return rt("Filter order must be a real, positive integer.");
    }
    Ok(m.data[0] as usize)
}

fn fir2(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 6 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let mut n = order_arg(a)?;
    let types = "double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64";
    let get = |i: usize, label: &str, long: &str| -> R<Matrix> {
        match &a[i] {
            Value::Num(m) if m.kind != Kind::Char && m.kind != Kind::Logical => Ok(m.clone()),
            other => rt(format!("Expected input {long} {label} to be one of these types:  {types}  Instead its type was {}.", other.class_name())),
        }
    };
    let fm = get(1, "F", "frequency points vector")?;
    let mm = get(2, "A", "desired magnitude response vector")?;
    for (m, what) in [(&mm, "A"), (&fm, "F")] {
        if m.dims.len() > 2 || (m.dims[0] > 1 && m.dims[1] > 1) {
            return rt(if what == "A" {
                "Expected input desired magnitude response vector A to be a vector."
            } else {
                "Expected input frequency points vector F to be a vector."
            });
        }
    }
    let f = fm.data.clone();
    let m: Vec<f64> = mm.data.clone();
    let mi: Vec<f64> = (0..mm.numel()).map(|k| mm.im_at(k)).collect();
    if f.len() != m.len() {
        return rt("You must specify the same number of frequencies and amplitudes.");
    }
    if f.windows(2).any(|p| p[1] < p[0]) {
        return rt("Frequencies must be non-decreasing.");
    }
    if f.first() != Some(&0.0) || f.last() != Some(&1.0) {
        return rt("The first frequency must be 0 and the last 1.");
    }
    if f.len() > 2 && (f[0] == f[1] || f[f.len() - 2] == f[f.len() - 1]) {
        return rt("Too abrupt an amplitude change near end of frequency interval.");
    }
    // Optional npt, lap and window.
    let mut npt = 512.0_f64;
    let mut lap = -1.0_f64;
    let mut window: Option<Vec<f64>> = None;
    let rest: Vec<usize> = (3..a.len()).collect();
    let scalar_at = |i: usize| -> Option<f64> {
        match &a[i] {
            Value::Num(m) if m.numel() == 1 => Some(m.data[0]),
            _ => None,
        }
    };
    let mut seen_npt = false;
    for &i in &rest {
        match scalar_at(i) {
            Some(v) if !seen_npt => {
                npt = v;
                seen_npt = true;
            }
            Some(v) => lap = v,
            None => window = Some(m_arg(a, i, "fir2")?.data),
        }
    }
    let npt_p = if npt > 0.0 { 2f64.powf(npt.log2().ceil()) } else { f64::NAN };
    if npt_p.is_nan() {
        return rt("Size vector must be a row vector with real elements.");
    }
    let min_npt = 2f64.powf(((n + 1) as f64).log2().ceil()) / 2.0;
    if npt_p < min_npt {
        return rt(format!("The number of grid points npt must be greater than or equal to {} for filter order {}.", min_npt, n));
    }
    let npt = npt_p as usize;
    // Odd order needs zero gain at Nyquist.
    if n % 2 == 1 && m.last().is_some_and(|&v| v != 0.0) {
        it.emit(OutputKind::Warning("Odd order symmetric FIR filters must have a gain of zero at the Nyquist frequency.  The order is being increased by one.".into()));
        n += 1;
    }
    let win = match window {
        Some(w) if w.len() == n + 1 => w,
        Some(_) => return rt("The specified window must be the same as the filter length."),
        None => hamming(n + 1),
    };
    // Grid values: every grid point belongs to one segment and is interpolated between that
    // segment's first and last grid points; repeated frequencies get a transition region.
    let lap_i = if lap < 0.0 { (npt as f64 / 25.0).floor() } else { lap };
    let p = (lap_i / 2.0).floor() as i64;
    let q = (lap_i / 2.0).ceil() as i64;
    let k = f.len();
    // Segment i holds the grid points nb[i] .. nb[i+1]-1, interpolated from its first to its last
    // point; a repeated frequency gets a transition region around its grid position.
    let mut nb: Vec<i64> = f.iter().map(|&x| (x * (npt + 1) as f64 + 1e-9).floor() as i64).collect();
    nb[0] = 0;
    for i in 0..k - 1 {
        if f[i] == f[i + 1] && lap_i > 0.0 {
            let c = (f[i] * (npt + 1) as f64 + 1e-9).floor() as i64;
            if lap_i <= 1.0 {
                nb[i] = c + 1;
                nb[i + 1] = c + 2;
            } else {
                nb[i] = c - p;
                nb[i + 1] = c + q + 1;
            }
        }
    }
    let mut mg = vec![0.0; npt + 1];
    let mut mgi = vec![0.0; npt + 1];
    for i in 0..k - 1 {
        let lo = nb[i].max(0);
        let hi = if i == k - 2 { npt as i64 } else { (nb[i + 1] - 1).min(npt as i64) };
        if hi < lo {
            continue;
        }
        for j in lo..=hi {
            let t = if hi == lo { 0.0 } else { (j - lo) as f64 / (hi - lo) as f64 };
            mg[j as usize] = m[i] + (m[i + 1] - m[i]) * t;
            mgi[j as usize] = mi[i] + (mi[i + 1] - mi[i]) * t;
        }
    }
    // Linear phase and inverse transform of the symmetric spectrum of length 2 npt.
    let total = 2 * npt;
    let mut h = vec![0.0; n + 1];
    for (t, ht) in h.iter_mut().enumerate() {
        let mut s = 0.0;
        for (j, &mag) in mg.iter().enumerate() {
            let w = if j == 0 || j == npt { 1.0 } else { 2.0 };
            let ang = PI * j as f64 / npt as f64 * (t as f64 - n as f64 / 2.0);
            s += w * (mag * ang.cos() - mgi[j] * ang.sin());
        }
        *ht = s / total as f64;
    }
    for (v, w) in h.iter_mut().zip(&win) {
        *v *= w;
    }
    let mut outv = vec![real_row(h)];
    if nargout > 1 {
        outv.push(scalar_value(1.0));
    }
    out(outv)
}

// ---- firls ---------------------------------------------------------------------------------

/// Gauss-Legendre nodes and weights on [-1, 1].
fn gauss_legendre(n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut x = vec![0.0; n];
    let mut w = vec![0.0; n];
    for i in 0..(n + 1) / 2 {
        let mut z = (PI * (i as f64 + 0.75) / (n as f64 + 0.5)).cos();
        let mut dp = 1.0;
        for _ in 0..100 {
            let (mut p1, mut p2) = (1.0, 0.0);
            for j in 0..n {
                let p3 = p2;
                p2 = p1;
                p1 = ((2 * j + 1) as f64 * z * p2 - j as f64 * p3) / (j + 1) as f64;
            }
            dp = n as f64 * (z * p1 - p2) / (z * z - 1.0);
            let dz = p1 / dp;
            z -= dz;
            if dz.abs() < 1e-16 {
                break;
            }
        }
        x[i] = -z;
        x[n - 1 - i] = z;
        w[i] = 2.0 / ((1.0 - z * z) * dp * dp);
        w[n - 1 - i] = w[i];
    }
    (x, w)
}

fn solve(mut m: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for c in 0..n {
        let piv = (c..n).max_by(|&x, &y| m[x][c].abs().partial_cmp(&m[y][c].abs()).unwrap()).unwrap();
        m.swap(c, piv);
        b.swap(c, piv);
        for r in c + 1..n {
            let f = m[r][c] / m[c][c];
            for k in c..n {
                m[r][k] -= f * m[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for k in r + 1..n {
            s -= m[r][k] * x[k];
        }
        x[r] = s / m[r][r];
    }
    x
}

fn firls(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 5 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let nm = m_arg(a, 0, "firls")?;
    if nm.numel() != 1 || nm.data[0] <= 0.0 {
        return rt("Expected input number 1, n, to be positive.");
    }
    if nm.data[0].fract() != 0.0 {
        return rt("Filter order must be a real, positive integer.");
    }
    let mut n = nm.data[0] as usize;
    let types = "double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64";
    for (i, l) in [(1usize, "f"), (2, "a")] {
        if !matches!(&a[i], Value::Num(m) if m.kind != Kind::Char && m.kind != Kind::Logical) {
            return rt(format!("Expected input number {}, {l}, to be one of these types:  {types}  Instead its type was {}.", i + 1, a[i].class_name()));
        }
    }
    let f = m_arg(a, 1, "firls")?.data;
    let amp = m_arg(a, 2, "firls")?.data;
    if f.len() % 2 != 0 {
        return rt("'F' must have even length.");
    }
    if f.len() != amp.len() {
        return rt("'F' and 'A' must have equal lengths.");
    }
    if f.windows(2).any(|p| p[1] < p[0]) {
        return rt("Frequencies in F must be non decreasing.");
    }
    if f.iter().any(|&x| !(0.0..=1.0).contains(&x)) {
        return rt("Frequencies in F must be in range [0,1].");
    }
    let nb = f.len() / 2;
    let mut weights = vec![1.0; nb];
    let mut kind = 0; // 0 plain, 1 hilbert, 2 differentiator
    for i in 3..a.len() {
        match &a[i] {
            Value::Num(m) if m.kind != Kind::Char => {
                if m.numel() != nb {
                    return rt("There must be 1 weight per band.");
                }
                weights = m.data.iter().map(|w| w.abs()).collect();
            }
            v => {
                let s = v.text().unwrap_or_default().to_lowercase();
                kind = match s.as_str() {
                    "hilbert" | "h" => 1,
                    "differentiator" | "d" => 2,
                    _ => return rt("Requires symmetry to be 'Hilbert' or 'differentiator'."),
                };
            }
        }
    }
    if a.len() == 5 && matches!(&a[3], Value::Num(m) if m.kind != Kind::Char) && matches!(&a[4], Value::Num(m) if m.kind != Kind::Char) {
        return rt("Requires symmetry to be 'Hilbert' or 'differentiator'.");
    }
    // Odd order with a nonzero amplitude at Nyquist: raise the order.
    let sym = kind == 0;
    if sym && n % 2 == 1 && f.last() == Some(&1.0) && amp.last().is_some_and(|&v| v != 0.0) {
        it.emit(OutputKind::Warning("Odd order symmetric FIR filters must have a gain of zero at the Nyquist frequency.  The order is being increased by one.".into()));
        n += 1;
    }
    // Basis: cosines (types I, II) or sines (III, IV) in w = pi f.
    let even = n % 2 == 0;
    let count = if sym { if even { n / 2 + 1 } else { (n + 1) / 2 } } else if even { n / 2 } else { (n + 1) / 2 };
    let basis = |k: usize, w: f64| -> f64 {
        if sym {
            if even { (k as f64 * w).cos() } else { ((k as f64 + 0.5) * w).cos() }
        } else if even {
            ((k as f64 + 1.0) * w).sin()
        } else {
            ((k as f64 + 0.5) * w).sin()
        }
    };
    let nodes = 60 + 3 * n;
    let (gx, gw) = gauss_legendre(nodes);
    let mut q = vec![vec![0.0; count]; count];
    let mut rhs = vec![0.0; count];
    for b in 0..nb {
        let (f1, f2) = (f[2 * b], f[2 * b + 1]);
        if f2 <= f1 {
            continue;
        }
        let (a1, a2) = (amp[2 * b], amp[2 * b + 1]);
        let (w1, w2) = (PI * f1, PI * f2);
        for (xi, wi) in gx.iter().zip(&gw) {
            let w = (w1 + w2) / 2.0 + (w2 - w1) / 2.0 * xi;
            let d = a1 + (a2 - a1) * (w - w1) / (w2 - w1);
            let mut wt = weights[b] * (w2 - w1) / 2.0 * wi;
            if kind == 2 && (a1 != 0.0 || a2 != 0.0) {
                wt /= w * w;
            }
            let bs: Vec<f64> = (0..count).map(|k| basis(k, w)).collect();
            for i in 0..count {
                rhs[i] += wt * d * bs[i];
                for j in 0..count {
                    q[i][j] += wt * bs[i] * bs[j];
                }
            }
        }
    }
    let c = solve(q, rhs);
    let mut h = vec![0.0; n + 1];
    if sym {
        if even {
            let m = n / 2;
            h[m] = c[0];
            for k in 1..=m {
                h[m - k] = c[k] / 2.0;
                h[m + k] = c[k] / 2.0;
            }
        } else {
            let m = (n + 1) / 2;
            for k in 0..m {
                h[m - 1 - k] = c[k] / 2.0;
                h[m + k] = c[k] / 2.0;
            }
        }
    } else if even {
        let m = n / 2;
        for k in 1..=m {
            h[m - k] = -c[k - 1] / 2.0;
            h[m + k] = c[k - 1] / 2.0;
        }
    } else {
        let m = (n + 1) / 2;
        for k in 0..m {
            h[m - 1 - k] = -c[k] / 2.0;
            h[m + k] = c[k] / 2.0;
        }
    }
    let mut outv = vec![real_row(h)];
    if nargout > 1 {
        outv.push(scalar_value(1.0));
    }
    out(outv)
}
