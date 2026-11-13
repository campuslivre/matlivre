//! `fircls` and `fircls1`: constrained least squares linear-phase FIR design by a multiple-exchange
//! (active-set) method. The amplitude is A(w) = sum_k c_k cos((k + nu) w), nu = 0 for even orders
//! and 1/2 for odd orders; the objective is the integral of (A - D)^2 over [0, pi].

use crate::builtins::Ret;
use crate::error::rt;
use crate::signal::poly::*;
use crate::value::*;
use std::f64::consts::PI;

pub const NAMES: &[&str] = &["fircls", "fircls1"];

/// An interval of frequencies [lo, hi] (radians) with a desired level and bounds.
#[derive(Clone)]
struct Band {
    lo: f64,
    hi: f64,
    amp: f64,
    min: f64,
    max: f64,
}

struct Basis {
    nu: f64,
    m: usize,
}

impl Basis {
    fn phi(&self, w: f64) -> Vec<f64> {
        (0..self.m).map(|k| ((k as f64 + self.nu) * w).cos()).collect()
    }
    fn dphi(&self, w: f64) -> Vec<f64> {
        (0..self.m).map(|k| -(k as f64 + self.nu) * ((k as f64 + self.nu) * w).sin()).collect()
    }
    fn a(&self, c: &[f64], w: f64) -> f64 {
        c.iter().enumerate().map(|(k, ck)| ck * ((k as f64 + self.nu) * w).cos()).sum()
    }
    fn da(&self, c: &[f64], w: f64) -> f64 {
        c.iter().enumerate().map(|(k, ck)| -ck * (k as f64 + self.nu) * ((k as f64 + self.nu) * w).sin()).sum()
    }
    /// Diagonal of the Gram matrix of the basis on [0, pi].
    fn gram(&self) -> Vec<f64> {
        (0..self.m).map(|k| if k == 0 && self.nu == 0.0 { PI } else { PI / 2.0 }).collect()
    }
    /// Linear term p_k = integral of D(w) phi_k(w) for piecewise-constant D.
    fn lin(&self, bands: &[Band]) -> Vec<f64> {
        (0..self.m)
            .map(|k| {
                let f = k as f64 + self.nu;
                bands
                    .iter()
                    .map(|b| if f == 0.0 { b.amp * (b.hi - b.lo) } else { b.amp * ((f * b.hi).sin() - (f * b.lo).sin()) / f })
                    .sum()
            })
            .collect()
    }
}

fn solve_linear(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let mut piv = c;
        for r in c + 1..n {
            if a[r][c].abs() > a[piv][c].abs() {
                piv = r;
            }
        }
        if a[piv][c].abs() < 1e-13 {
            return None;
        }
        a.swap(c, piv);
        b.swap(c, piv);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            if f != 0.0 {
                for k in c..n {
                    a[r][k] -= f * a[c][k];
                }
                b[r] -= f * b[c];
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for k in r + 1..n {
            s -= a[r][k] * x[k];
        }
        x[r] = s / a[r][r];
    }
    Some(x)
}

#[derive(Clone)]
struct Active {
    w: f64,
    upper: bool,
    bound: f64,
}

/// Bounds that apply at frequency `w` (the tightest if bands share an edge), as (lower, upper).
type BoundFn<'a> = dyn Fn(f64) -> Option<(f64, f64)> + 'a;

fn extrema(basis: &Basis, c: &[f64], grid: &[f64]) -> Vec<(f64, bool)> {
    let n = grid.len() - 1;
    let ders: Vec<f64> = grid.iter().map(|&w| basis.da(c, w)).collect();
    let mut out: Vec<(f64, bool)> = vec![];
    // endpoints
    if basis.nu == 0.0 {
        let h = grid[1];
        out.push((0.0, basis.a(c, 0.0) > basis.a(c, h)));
        out.push((PI, basis.a(c, PI) > basis.a(c, PI - h)));
    } else {
        let h = grid[1];
        out.push((0.0, basis.a(c, 0.0) > basis.a(c, h)));
    }
    for i in 1..=n {
        if ders[i - 1] * ders[i] < 0.0 {
            let (mut lo, mut hi) = (grid[i - 1], grid[i]);
            let up_first = ders[i - 1] > 0.0;
            for _ in 0..70 {
                let mid = 0.5 * (lo + hi);
                if (basis.da(c, mid) > 0.0) == up_first {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            out.push((0.5 * (lo + hi), up_first));
        }
    }
    out
}

fn solve_cls(basis: &Basis, p: &[f64], bounds: &BoundFn, _edges: &[f64], _extrema_only: bool) -> Option<Vec<f64>> {
    let m = basis.m;
    let g = basis.gram();
    let mut active: Vec<Active> = vec![];
    let ngrid = 4096;
    let grid: Vec<f64> = (0..=ngrid).map(|i| PI * i as f64 / ngrid as f64).collect();
    let mut c = vec![0.0; m];
    let mut stalled = 0;
    for _iter in 0..3000 {
        let na = active.len();
        let n = m + na;
        let mut a = vec![vec![0.0; n]; n];
        let mut b = vec![0.0; n];
        for k in 0..m {
            a[k][k] = 2.0 * g[k];
            b[k] = 2.0 * p[k];
        }
        for (i, ac) in active.iter().enumerate() {
            let ph = basis.phi(ac.w);
            for k in 0..m {
                a[k][m + i] = ph[k];
                a[m + i][k] = ph[k];
            }
            b[m + i] = ac.bound;
        }
        let sol = match solve_linear(a, b) {
            Some(s) => s,
            None => {
                active.pop();
                if active.is_empty() {
                    return None;
                }
                continue;
            }
        };
        c = sol[..m].to_vec();
        let lam = &sol[m..];
        let mut worst: Option<(usize, f64)> = None;
        for (i, ac) in active.iter().enumerate() {
            let bad = if ac.upper { -lam[i] } else { lam[i] };
            if bad > 1e-12 && worst.map_or(true, |w| bad > w.1) {
                worst = Some((i, bad));
            }
        }
        if let Some((i, _)) = worst {
            active.remove(i);
            continue;
        }
        let ex = extrema(basis, &c, &grid);
        // move every active constraint to the nearest extremum of the same kind
        let mut moved = false;
        for ac in active.iter_mut() {
            let mut best: Option<(f64, f64)> = None;
            for &(w, is_max) in &ex {
                if is_max == ac.upper {
                    let d = (w - ac.w).abs();
                    if best.map_or(true, |b| d < b.0) {
                        best = Some((d, w));
                    }
                }
            }
            if let Some((d, w)) = best {
                if d > 1e-11 {
                    ac.w = w;
                    moved = true;
                }
            }
        }
        if moved {
            stalled += 1;
            if stalled < 200 {
                continue;
            }
        }
        // new violations at extrema
        let mut added = false;
        for &(w, is_max) in &ex {
            let Some((lo, hi)) = bounds(w) else { continue };
            let v = basis.a(&c, w);
            let tol = 1e-12;
            if is_max && v > hi + tol && !active.iter().any(|a| a.upper && (a.w - w).abs() < 1e-8) {
                active.push(Active { w, upper: true, bound: hi });
                added = true;
            } else if !is_max && v < lo - tol && !active.iter().any(|a| !a.upper && (a.w - w).abs() < 1e-8) {
                active.push(Active { w, upper: false, bound: lo });
                added = true;
            }
        }
        if !added {
            return Some(c);
        }
        stalled = 0;
    }
    Some(c)
}

fn taps(c: &[f64], nu: f64) -> Vec<f64> {
    let m = c.len();
    if nu == 0.0 {
        let n = 2 * (m - 1);
        let mut h = vec![0.0; n + 1];
        h[n / 2] = c[0];
        for k in 1..m {
            h[n / 2 + k] = c[k] / 2.0;
            h[n / 2 - k] = c[k] / 2.0;
        }
        h
    } else {
        let n = 2 * m - 1;
        let mut h = vec![0.0; n + 1];
        for k in 0..m {
            h[m + k] = c[k] / 2.0;
            h[m - 1 - k] = c[k] / 2.0;
        }
        h
    }
}

pub fn call(name: &str, a: &[Value], nargout: usize) -> Ret {
    if name == "fircls" { fircls(a, nargout) } else { fircls1(a, nargout) }
}

fn numeric_scalar_check(v: &Value, label: &str, fname: &str) -> R<()> {
    match v {
        Value::Num(m) if m.kind != Kind::Char => Ok(()),
        other => rt(format!("Input {label} in {fname} must be numeric, instead it was {}.", other.class_name())),
    }
}

fn fircls(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 5 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 6 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    numeric_scalar_check(&a[0], "N", "fircls")?;
    if let Some(v) = a.get(5) {
        let t = v.text().unwrap_or_default().to_lowercase();
        if !["trace", "plots", "both"].contains(&t.as_str()) {
            return rt("Unrecognized trace option. Try 'trace', 'plots', or 'both'.");
        }
    }
    let nm = crate::builtins::m_arg(a, 0, "fircls")?;
    let f = crate::builtins::m_arg(a, 1, "fircls")?.data.clone();
    let amp = crate::builtins::m_arg(a, 2, "fircls")?.data.clone();
    let up = crate::builtins::m_arg(a, 3, "fircls")?.data.clone();
    let lo = crate::builtins::m_arg(a, 4, "fircls")?.data.clone();
    let n = nm.data.first().copied().unwrap_or(0.0);
    if n.fract() != 0.0 {
        return rt("Filter order must be a real, positive integer.");
    }
    if n < 3.0 {
        return rt("Filter order must be 3 or more.");
    }
    if f.first().copied() != Some(0.0) {
        return rt("F must begin with 0");
    }
    if f.last().copied() != Some(1.0) {
        return rt("F must end with 1");
    }
    if f.windows(2).any(|w| w[1] < w[0]) {
        return rt("The frequencies in F must be in ascending order.");
    }
    if amp.len() != f.len() - 1 || up.len() != amp.len() || lo.len() != amp.len() {
        return rt("Each of A, UP, and LO must have the same length.");
    }
    if (0..amp.len()).any(|i| up[i] <= lo[i]) {
        return rt("Each element of UP must be greater than the corresponding element of LO.");
    }
    if lo.iter().any(|v| *v == 0.0) && lo.last() == Some(&0.0) && lo.iter().any(|v| *v == 0.0) {
        // zero lower bounds select a spectral-factorisation design in MATLAB
        return rt("fircls: lower bounds of zero (minimum-phase design) are not implemented.");
    }
    let mut n = n as usize;
    let last_amp_nonzero = amp.last().map_or(false, |v| *v != 0.0) || lo.last().map_or(false, |v| *v > 0.0);
    if n % 2 == 1 && last_amp_nonzero {
        n += 1;
    }
    let nu = if n % 2 == 0 { 0.0 } else { 0.5 };
    let mcount = if n % 2 == 0 { n / 2 + 1 } else { (n + 1) / 2 };
    let basis = Basis { nu, m: mcount };
    let bands: Vec<Band> = (0..amp.len()).map(|i| Band { lo: f[i] * PI, hi: f[i + 1] * PI, amp: amp[i], min: lo[i], max: up[i] }).collect();
    let p = basis.lin(&bands);
    let edges: Vec<f64> = f.iter().map(|v| v * PI).collect();
    let bounds = |w: f64| -> Option<(f64, f64)> {
        let mut l = f64::NEG_INFINITY;
        let mut u = f64::INFINITY;
        let mut any = false;
        for b in &bands {
            if w >= b.lo - 1e-12 && w <= b.hi + 1e-12 && b.hi > b.lo {
                l = l.max(b.min);
                u = u.min(b.max);
                any = true;
            }
        }
        if any { Some((l, u)) } else { None }
    };
    let Some(c) = solve_cls(&basis, &p, &bounds, &edges, true) else {
        return rt("fircls: the design did not converge.");
    };
    out(vec![real_row(taps(&c, nu)), scalar_value(1.0)].into_iter().take(nargout.max(1)).collect())
}

fn fircls1(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    numeric_scalar_check(&a[0], "N", "fircls1")?;
    // trailing options
    let mut rest: Vec<Value> = vec![];
    let mut high = false;
    for v in a {
        if let Value::Num(m) = v {
            if m.kind == Kind::Char {
                let t = v.text().unwrap_or_default().to_lowercase();
                match t.as_str() {
                    "high" => high = true,
                    "trace" | "plots" | "both" => {}
                    _ => return rt("Unrecognized character vector input."),
                }
                continue;
            }
        }
        rest.push(v.clone());
    }
    if rest.len() < 4 {
        return rt("You did not supply enough input parameters.");
    }
    if rest.len() == 6 {
        return rt("Unrecognized character vector input.");
    }
    let s = |i: usize| -> R<Vec<f64>> { Ok(crate::builtins::m_arg(&rest, i, "fircls1")?.data.clone()) };
    let (nv, wov, dpv, dsv) = (s(0)?, s(1)?, s(2)?, s(3)?);
    if nv.len() != 1 || wov.len() != 1 || dpv.len() != 1 || dsv.len() != 1 {
        return rt("Each of N, WO, DP, and DS must be a scalar.");
    }
    let (n0, wo, dp, ds) = (nv[0], wov[0], dpv[0], dsv[0]);
    if n0.fract() != 0.0 {
        return rt("Filter order must be a real, positive integer.");
    }
    if n0 < 3.0 {
        return rt("Filter order must be 3 or more.");
    }
    if !(wo > 0.0 && wo < 1.0) {
        return rt("W0 must lie between 0 and 1.");
    }
    if !(dp > 0.0 && ds > 0.0) {
        return rt("Both DP and DS must be positive.");
    }
    let mut n = n0 as usize;
    if high && n % 2 == 1 {
        n += 1;
    }
    let extra: Vec<f64> = rest[4..].iter().map(|v| v.to_matrix().map(|m| m.data[0]).unwrap_or(0.0)).collect();
    let nu = if n % 2 == 0 { 0.0 } else { 0.5 };
    let mcount = if n % 2 == 0 { n / 2 + 1 } else { (n + 1) / 2 };
    let basis = Basis { nu, m: mcount };
    let w0 = wo * PI;
    // optional constraint edge (wt) and L2 weights (wp, ws, k)
    if extra.len() == 3 {
        let (wp, ws) = (extra[0], extra[1]);
        let ok = if high { ws < wo && wo < wp } else { wp < wo && wo < ws };
        if !ok {
            return rt(if high { "For highpass filters, WS < W0 < WP is needed." } else { "For lowpass filters, WP < W0 < WS is needed." });
        }
        return rt("fircls1: the weighted (WP, WS, K) design is not implemented.");
    }
    if extra.len() == 1 {
        return rt("fircls1: the extra constraint edge (WT) is not implemented.");
    }
    let (pass_lo, pass_hi, stop_bound) = (1.0 - dp, 1.0 + dp, ds);
    let bands = if high {
        vec![Band { lo: 0.0, hi: w0, amp: 0.0, min: -ds, max: ds }, Band { lo: w0, hi: PI, amp: 1.0, min: pass_lo, max: pass_hi }]
    } else {
        vec![Band { lo: 0.0, hi: w0, amp: 1.0, min: pass_lo, max: pass_hi }, Band { lo: w0, hi: PI, amp: 0.0, min: -ds, max: ds }]
    };
    let _ = stop_bound;
    let p = basis.lin(&bands);
    let bounds = |w: f64| -> Option<(f64, f64)> {
        let in_first = w <= w0;
        let is_pass = if high { !in_first } else { in_first };
        if is_pass { Some((pass_lo, pass_hi)) } else { Some((-ds, ds)) }
    };
    let Some(c) = solve_cls(&basis, &p, &bounds, &[], true) else {
        return rt("fircls1: the design did not converge.");
    };
    out(vec![real_row(taps(&c, nu)), scalar_value(1.0)].into_iter().take(nargout.max(1)).collect())
}
