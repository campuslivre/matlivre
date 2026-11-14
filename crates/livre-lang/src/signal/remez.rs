//! Equiripple FIR design: `firpm` (Parks-McClellan exchange on a frequency grid) and `firpmord`.

use crate::builtins::{m_arg, Ret};
use crate::cplx::C;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["firpm", "firpmord"];

const PI: f64 = std::f64::consts::PI;

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if name == "firpm" { firpm(a, nargout) } else { firpmord(a, nargout) }
}

#[derive(Clone, Copy, PartialEq)]
enum Ftype {
    Band,
    Hilbert,
    Diff,
}

struct Grid {
    f: Vec<f64>,   // in units of Nyquist (0..1)
    des: Vec<f64>, // user amplitude
    wt: Vec<f64>,  // user weight (including 1/f for differentiators)
}

fn firpm(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let n = m_arg(a, 0, "firpm")?;
    if n.numel() != 1 || n.data[0] < 3.0 {
        return rt("Filter order must be 3 or more.");
    }
    let mut order = n.data[0].floor() as usize;
    let f = m_arg(a, 1, "firpm")?.data;
    let amp = m_arg(a, 2, "firpm")?.data;
    if f.len() % 2 != 0 {
        return rt("The number of frequency points must be even.");
    }
    if f.len() != amp.len() {
        return rt("Frequency and amplitude vectors must be the same length.");
    }
    if f.windows(2).any(|p| p[1] < p[0]) {
        return rt("Frequencies must be non-decreasing.");
    }
    if f.iter().any(|&x| !(0.0..=1.0).contains(&x)) {
        return rt("Frequencies must lie between 0 and 1.");
    }
    let nb = f.len() / 2;
    let mut weights = vec![1.0; nb];
    let mut ftype = Ftype::Band;
    let mut lgrid = 16usize;
    for v in &a[3..] {
        match v {
            Value::Cell(c) => {
                if let Some(Value::Num(m)) = c.data.first() {
                    lgrid = m.data.first().copied().unwrap_or(16.0).max(1.0) as usize;
                }
            }
            Value::Num(m) if m.kind != Kind::Char => {
                if m.numel() != nb {
                    return rt("There should be one weight per band.");
                }
                weights = m.data.clone();
            }
            other => {
                let s = other.text().unwrap_or_default().to_lowercase();
                ftype = match s.as_str() {
                    "hilbert" | "h" => Ftype::Hilbert,
                    "differentiator" | "d" => Ftype::Diff,
                    _ => Ftype::Band,
                };
            }
        }
    }
    if ftype == Ftype::Band && order % 2 == 1 && f.last() == Some(&1.0) && amp.last().is_some_and(|&v| v != 0.0) {
        order += 1;
    }
    if weights.iter().any(|&w| w <= 0.0) {
        return rt("All weights must be greater than zero.");
    }
    if (1..nb).any(|b| f[2 * b - 1] == f[2 * b]) {
        return rt("Adjacent bands not allowed.");
    }
    let (b, dev, grid, iext, h) = design(order, &f, &amp, &weights, ftype, lgrid);
    let mut outv = vec![real_row(b)];
    if nargout > 1 {
        outv.push(scalar_value(dev.abs()));
    }
    if nargout > 2 {
        let col = |v: Vec<f64>| real_col(v);
        let fext: Vec<f64> = iext.iter().map(|&i| grid.f[i]).collect();
        let err: Vec<f64> = (0..grid.f.len()).map(|j| grid.wt[j] * (grid.des[j] - h.1[j])).collect();
        let hre: Vec<f64> = h.0.iter().map(|c| c.0).collect();
        let him: Vec<f64> = h.0.iter().map(|c| c.1).collect();
        let hv = if him.iter().all(|&v| v == 0.0) {
            Value::Num(Matrix::from_dims(vec![hre.len(), 1], hre))
        } else {
            Value::Num(Matrix::complex(vec![hre.len(), 1], hre, him))
        };
        let st = StructArray {
            dims: vec![1, 1],
            fields: ["fgrid", "des", "wt", "H", "error", "iextr", "fextr"].iter().map(|s| s.to_string()).collect(),
            elems: vec![vec![
                col(grid.f.clone()),
                col(grid.des.clone()),
                col(grid.wt.clone()),
                hv,
                col(err),
                col(iext.iter().map(|&i| (i + 1) as f64).collect()),
                col(fext),
            ]],
        };
        outv.push(Value::Struct(st));
    }
    out(outv)
}

/// Barycentric interpolation of the values `y` at nodes `xn` evaluated at `x`.
fn bary(xn: &[f64], w: &[f64], y: &[f64], x: f64) -> f64 {
    let (mut num, mut den) = (0.0, 0.0);
    for k in 0..xn.len() {
        let d = x - xn[k];
        if d.abs() < 1e-14 {
            return y[k];
        }
        let t = w[k] / d;
        num += t * y[k];
        den += t;
    }
    num / den
}

fn weights_of(xn: &[f64]) -> Vec<f64> {
    (0..xn.len())
        .map(|k| {
            let mut p = 1.0;
            for (i, &xi) in xn.iter().enumerate() {
                if i != k {
                    p *= xn[k] - xi;
                }
            }
            1.0 / p
        })
        .collect()
}

type Design = (Vec<f64>, f64, Grid, Vec<usize>, (Vec<(f64, f64)>, Vec<f64>));

fn design(order: usize, f: &[f64], amp: &[f64], wts: &[f64], ftype: Ftype, lgrid: usize) -> Design {
    let numtaps = order + 1;
    let nodd = numtaps % 2;
    let neg = ftype != Ftype::Band;
    let mut nfcns = numtaps / 2;
    if nodd == 1 && !neg {
        nfcns += 1;
    }
    let delf = 1.0 / (lgrid as f64 * nfcns as f64);
    let nb = f.len() / 2;
    let mut edges = f.to_vec();
    if neg && edges[0] < delf {
        edges[0] = delf;
    }
    // Grid, in units of the Nyquist frequency.
    let mut gf = vec![];
    let mut gdes = vec![];
    let mut gwt = vec![];
    for b in 0..nb {
        let (lo, hi) = (edges[2 * b], edges[2 * b + 1]);
        let (a1, a2) = (amp[2 * b], amp[2 * b + 1]);
        let (f0, f1) = (f[2 * b], f[2 * b + 1]);
        let desired = |x: f64| if f1 > f0 { a1 + (a2 - a1) * (x - f0) / (f1 - f0) } else { a1 };
        let weight = |x: f64| -> f64 {
            if ftype == Ftype::Diff && (a1 != 0.0 || a2 != 0.0) { wts[b] / (x / 2.0).max(1e-300) } else { wts[b] }
        };
        let mut x = lo;
        let mut pts = vec![];
        loop {
            pts.push(x);
            x += delf;
            if x > hi {
                break;
            }
        }
        let last = pts.len() - 1;
        pts[last] = hi;
        for &p in &pts {
            gf.push(p);
            gdes.push(desired(p));
            gwt.push(weight(p));
        }
    }
    // Remove the points where the transformed weight vanishes.
    let type2_or_3 = neg == (nodd == 1);
    if type2_or_3 && gf.last().is_some_and(|&x| x > 1.0 - delf) {
        gf.pop();
        gdes.pop();
        gwt.pop();
    }
    let trans = |x: f64| -> f64 {
        let fc = x / 2.0;
        match (neg, nodd) {
            (false, 1) => 1.0,
            (false, _) => (PI * fc).cos(),
            (true, 1) => (2.0 * PI * fc).sin(),
            (true, _) => (PI * fc).sin(),
        }
    };
    let ng = gf.len();
    let d: Vec<f64> = (0..ng).map(|j| gdes[j] / trans(gf[j])).collect();
    let w: Vec<f64> = (0..ng).map(|j| gwt[j] * trans(gf[j])).collect();
    let xg: Vec<f64> = gf.iter().map(|&x| (PI * x).cos()).collect();
    let r = nfcns;
    let mut iext: Vec<usize> = (0..=r).map(|j| ((j as f64) * (ng - 1) as f64 / r as f64).floor() as usize).collect();
    iext[r] = ng - 1;
    let mut dev = 0.0;
    let mut node_y: Vec<f64> = vec![];
    let mut node_x: Vec<f64> = vec![];
    let mut node_w: Vec<f64> = vec![];
    for _ in 0..300 {
        let xe: Vec<f64> = iext.iter().map(|&i| xg[i]).collect();
        let ad = weights_of(&xe);
        let (mut num, mut den) = (0.0, 0.0);
        for k in 0..=r {
            let sgn = if k % 2 == 0 { 1.0 } else { -1.0 };
            num += ad[k] * d[iext[k]];
            den += ad[k] * sgn / w[iext[k]];
        }
        dev = num / den;
        node_x = xe[..r].to_vec();
        node_y = (0..r).map(|k| d[iext[k]] - if k % 2 == 0 { dev } else { -dev } / w[iext[k]]).collect();
        node_w = weights_of(&node_x);
        let err: Vec<f64> = (0..ng).map(|j| w[j] * (d[j] - bary(&node_x, &node_w, &node_y, xg[j]))).collect();
        // Peaks of each sign run.
        let mut peaks: Vec<usize> = vec![];
        let mut j = 0;
        while j < ng {
            let s = err[j] >= 0.0;
            let mut best = j;
            let mut k = j;
            while k < ng && (err[k] >= 0.0) == s {
                if err[k].abs() > err[best].abs() {
                    best = k;
                }
                k += 1;
            }
            peaks.push(best);
            j = k;
        }
        while peaks.len() > r + 1 {
            let excess = peaks.len() - (r + 1);
            let first = err[peaks[0]].abs();
            let lastv = err[peaks[peaks.len() - 1]].abs();
            if excess == 1 {
                if first < lastv { peaks.remove(0); } else { peaks.pop(); }
            } else {
                let mut best_pair = (f64::INFINITY, 0usize);
                for i in 0..peaks.len() - 1 {
                    let c = err[peaks[i]].abs().max(err[peaks[i + 1]].abs());
                    if c < best_pair.0 {
                        best_pair = (c, i);
                    }
                }
                if first.min(lastv) < best_pair.0 {
                    if first < lastv { peaks.remove(0); } else { peaks.pop(); }
                } else {
                    peaks.remove(best_pair.1 + 1);
                    peaks.remove(best_pair.1);
                }
            }
        }
        if peaks.len() < r + 1 {
            // Rounding left too few sign changes: take the largest local maxima of |E| instead.
            let mut loc: Vec<usize> = (0..ng)
                .filter(|&j| (j == 0 || err[j].abs() >= err[j - 1].abs()) && (j + 1 == ng || err[j].abs() >= err[j + 1].abs()))
                .collect();
            for &i in &iext {
                if !loc.contains(&i) {
                    loc.push(i);
                }
            }
            if loc.len() < r + 1 {
                break;
            }
            loc.sort_by(|&a, &b| err[b].abs().partial_cmp(&err[a].abs()).unwrap());
            loc.truncate(r + 1);
            loc.sort();
            peaks = loc;
        }
        let emax = peaks.iter().map(|&i| err[i].abs()).fold(0.0_f64, f64::max);
        let changed = peaks != iext;
        iext = peaks;
        if !changed || (emax - dev.abs()) <= 1e-13 * emax {
            // Recompute dev and nodes for the final set.
            let xe: Vec<f64> = iext.iter().map(|&i| xg[i]).collect();
            let ad = weights_of(&xe);
            let (mut num, mut den) = (0.0, 0.0);
            for k in 0..=r {
                let sgn = if k % 2 == 0 { 1.0 } else { -1.0 };
                num += ad[k] * d[iext[k]];
                den += ad[k] * sgn / w[iext[k]];
            }
            dev = num / den;
            node_x = xe[..r].to_vec();
            node_y = (0..r).map(|k| d[iext[k]] - if k % 2 == 0 { dev } else { -dev } / w[iext[k]]).collect();
            node_w = weights_of(&node_x);
            break;
        }
    }
    // Amplitude at frequency x (units of the Nyquist frequency).
    let amp_at = |x: f64| -> f64 { bary(&node_x, &node_w, &node_y, (PI * x).cos()) * trans(x) };
    let nn = numtaps;
    let c = (nn as f64 - 1.0) / 2.0;
    let mut h = vec![0.0; nn];
    for (k, hk) in h.iter_mut().enumerate() {
        let kk = k as f64;
        let mut s = 0.0;
        if !neg {
            s += amp_at(0.0);
            for m in 1..=(nn / 2) {
                let fm = m as f64 / nn as f64; // cycles
                let a_m = amp_at(2.0 * fm);
                let wgt = if nn % 2 == 0 && m == nn / 2 { 1.0 } else { 2.0 };
                s += wgt * a_m * (2.0 * PI * fm * (kk - c)).cos();
            }
        } else {
            for m in 1..=(nn / 2) {
                let fm = m as f64 / nn as f64;
                let a_m = amp_at(2.0 * fm);
                let wgt = if nn % 2 == 0 && m == nn / 2 { 1.0 } else { 2.0 };
                let sg = if ftype == Ftype::Hilbert { -1.0 } else { 1.0 };
                s += sg * wgt * a_m * (2.0 * PI * fm * (c - kk)).sin();
            }
        }
        *hk = s / nn as f64;
    }
    // Frequency response on the grid.
    let hres: Vec<(f64, f64)> = gf
        .iter()
        .map(|&x| {
            let (mut re, mut im) = (0.0, 0.0);
            for (k, &hk) in h.iter().enumerate() {
                let ang = -PI * x * k as f64;
                re += hk * ang.cos();
                im += hk * ang.sin();
            }
            (re, im)
        })
        .collect();
    let amps: Vec<f64> = gf.iter().map(|&x| amp_at(x)).collect();
    let grid = Grid { f: gf, des: gdes, wt: gwt };
    (h, dev, grid, iext, (hres, amps))
}

fn firpmord(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if nargout > 4 {
        return rt("Too many output arguments.");
    }
    let f = m_arg(a, 0, "firpmord")?.data;
    let am = m_arg(a, 1, "firpmord")?.data;
    let dev = m_arg(a, 2, "firpmord")?.data;
    let mut fs = 2.0;
    let mut cell = false;
    for v in &a[3..] {
        match v {
            Value::Num(m) if m.kind == Kind::Char => cell = m.to_string_lossy().to_lowercase() == "cell",
            Value::Num(m) if m.numel() >= 1 => fs = m.data[0],
            _ => {}
        }
    }
    if dev.len() != am.len() {
        return rt("'A' and 'DEV' must be vectors of the same length.");
    }
    if f.len() != 2 * am.len() - 2 {
        return rt("Length of 'F' must be 2*length(A)-2.");
    }
    if fs <= 0.0 {
        return rt("Band-edge frequencies must be positive.");
    }
    if f.iter().any(|&x| x >= fs / 2.0) {
        return rt("Band-edge frequencies must be less than the Nyquist frequency.");
    }
    let rel: Vec<f64> = dev.iter().zip(&am).map(|(&d, &g)| if g != 0.0 { d / g } else { d }).collect();
    let maxd = rel.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let w: Vec<f64> = rel.iter().map(|&d| maxd / d).collect();
    let (a1, a2, a3, a4, a5, a6) = (5.309e-3, 7.114e-2, -4.761e-1, -2.66e-3, -5.941e-1, -4.278e-1);
    let lg = |d: f64| -> C { C::new(d.abs().log10(), if d < 0.0 { PI / std::f64::consts::LN_10 } else { 0.0 }) };
    let k = |x: f64| C::new(x, 0.0);
    let mut nmax: Option<(f64, f64)> = None;
    for i in 0..am.len() - 1 {
        let (mut d1, mut d2) = (rel[i], rel[i + 1]);
        if am[i] == 0.0 && am[i + 1] != 0.0 {
            std::mem::swap(&mut d1, &mut d2);
        }
        let (l1, l2) = (lg(d1), lg(d2));
        let sq = l1.mul(l1);
        let dinf = k(a1).mul(sq).add(k(a2).mul(l1)).add(k(a3)).mul(l2).add(k(a4).mul(sq)).add(k(a5).mul(l1)).add(k(a6));
        let ff = k(11.01217).add(k(0.51244).mul(l1.sub(l2)));
        let df = (f[2 * i + 1] - f[2 * i]).abs() / fs;
        let nn = dinf.div(k(df)).sub(ff.mul(k(df))).add(k(1.0));
        let mut re = nn.re.ceil() - 1.0;
        if am[i] != 0.0 && am[i + 1] != 0.0 {
            re += 1.0;
        }
        let cand = (re, nn.im.ceil());
        if nmax.map_or(true, |m| cand.0 > m.0) {
            nmax = Some(cand);
        }
    }
    let (nre, nim) = nmax.unwrap_or((0.0, 0.0));
    let nvalue = if nim != 0.0 { Value::Num(Matrix::complex(vec![1, 1], vec![nre], vec![nim])) } else { scalar_value(nre) };
    let mut fo = vec![0.0];
    fo.extend(f.iter().map(|&x| x / (fs / 2.0)));
    fo.push(1.0);
    let mut ao = vec![];
    for &g in &am {
        ao.push(g);
        ao.push(g);
    }
    let vals = vec![nvalue, real_col(fo), real_col(ao), real_col(w)];
    if cell && nargout <= 1 {
        return out(vec![Value::Cell(CellArray::row(vals))]);
    }
    let mut v = vals;
    v.truncate(nargout.max(1));
    out(v)
}
