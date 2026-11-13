//! `cfirpm`: complex and nonlinear-phase equiripple FIR design. MATLAB's result is the exact
//! Chebyshev (minimax) optimum on its frequency grid, which is found here with a log-barrier
//! method on the second-order-cone form of the problem.

use crate::builtins::Ret;
use crate::error::rt;
use crate::signal::poly::*;
use crate::value::*;
use std::f64::consts::PI;

pub const NAMES: &[&str] = &["cfirpm"];

type C = (f64, f64);

fn cmul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

#[derive(Clone, Copy, PartialEq)]
enum Sym {
    None,
    Even,
    Odd,
    Real,
}

fn sym_name(s: Sym) -> &'static str {
    match s {
        Sym::None => "none",
        Sym::Even => "even",
        Sym::Odd => "odd",
        Sym::Real => "real",
    }
}

fn nextpow2(x: f64) -> u32 {
    if x <= 1.0 {
        0
    } else {
        x.log2().ceil() as u32
    }
}

/// Solves the symmetric positive definite system `a x = b` (Cholesky, then Gaussian elimination
/// when the matrix is not definite).
fn solve_spd(a: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    let n = b.len();
    let mut m: Vec<Vec<f64>> = a.to_vec();
    let mut r = b.to_vec();
    for c in 0..n {
        let mut piv = c;
        for k in c + 1..n {
            if m[k][c].abs() > m[piv][c].abs() {
                piv = k;
            }
        }
        m.swap(c, piv);
        r.swap(c, piv);
        let d = m[c][c];
        if d.abs() < 1e-300 {
            continue;
        }
        for k in c + 1..n {
            let f = m[k][c] / d;
            if f != 0.0 {
                for j in c..n {
                    let v = m[c][j];
                    m[k][j] -= f * v;
                }
                r[k] -= f * r[c];
            }
        }
    }
    let mut x = vec![0.0; n];
    for k in (0..n).rev() {
        let mut s = r[k];
        for j in k + 1..n {
            s -= m[k][j] * x[j];
        }
        x[k] = if m[k][k].abs() < 1e-300 { 0.0 } else { s / m[k][k] };
    }
    x
}

/// Least squares `min |A x - d|` over real `x` with complex rows (real and imaginary parts
/// stacked), by modified Gram-Schmidt QR with reorthogonalisation.
fn lstsq(ar: &[Vec<f64>], ai: &[Vec<f64>], dr: &[f64], di: &[f64]) -> Vec<f64> {
    let n = ar.len();
    let m = ar[0].len();
    let rows = 2 * n;
    let mut q: Vec<Vec<f64>> = (0..m).map(|j| (0..rows).map(|k| if k < n { ar[k][j] } else { ai[k - n][j] }).collect()).collect();
    let mut r = vec![vec![0.0; m]; m];
    for j in 0..m {
        for _pass in 0..2 {
            for i in 0..j {
                let dot: f64 = (0..rows).map(|k| q[i][k] * q[j][k]).sum();
                r[i][j] += dot;
                for k in 0..rows {
                    let v = q[i][k];
                    q[j][k] -= dot * v;
                }
            }
        }
        let nrm: f64 = q[j].iter().map(|v| v * v).sum::<f64>().sqrt();
        r[j][j] = nrm;
        if nrm > 1e-300 {
            q[j].iter_mut().for_each(|v| *v /= nrm);
        }
    }
    let rhs: Vec<f64> = (0..rows).map(|k| if k < n { dr[k] } else { di[k - n] }).collect();
    let mut y: Vec<f64> = (0..m).map(|j| (0..rows).map(|k| q[j][k] * rhs[k]).sum()).collect();
    for j in (0..m).rev() {
        let mut s = y[j];
        for k in j + 1..m {
            s -= r[j][k] * y[k];
        }
        y[j] = if r[j][j].abs() < 1e-300 { 0.0 } else { s / r[j][j] };
    }
    y
}

/// Minimises the largest `|d_k - a_k x|` (complex rows, real unknowns): returns `x` and the
/// optimum.
fn minimax(ar: &[Vec<f64>], ai: &[Vec<f64>], dr: &[f64], di: &[f64]) -> (Vec<f64>, f64) {
    let n = ar.len();
    let m = ar[0].len();
    let arc: Vec<Vec<f64>> = (0..m).map(|j| (0..n).map(|k| ar[k][j]).collect()).collect();
    let aic: Vec<Vec<f64>> = (0..m).map(|j| (0..n).map(|k| ai[k][j]).collect()).collect();
    let mut x = lstsq(ar, ai, dr, di);
    let resid = |x: &[f64]| -> (Vec<f64>, Vec<f64>) {
        let er: Vec<f64> = (0..n).map(|k| dr[k] - (0..m).map(|j| ar[k][j] * x[j]).sum::<f64>()).collect();
        let ei: Vec<f64> = (0..n).map(|k| di[k] - (0..m).map(|j| ai[k][j] * x[j]).sum::<f64>()).collect();
        (er, ei)
    };
    let (er, ei) = resid(&x);
    let tmax = (0..n).map(|k| er[k].hypot(ei[k])).fold(0.0, f64::max);
    let scale = (0..n).map(|k| dr[k].hypot(di[k])).fold(0.0, f64::max).max(1e-300);
    if tmax <= 1e-14 * scale {
        return (x, tmax);
    }
    let mut t = tmax * 1.5 + 1e-6 * scale;
    let mut mu = (n as f64 / t).max(1.0 / scale);
    for _outer in 0..80 {
        let mut cnt = 0;
        let mut cnt = 0usize;
        for _it in 0..200 {
            cnt += 1;
            cnt += 1;
            let (er, ei) = resid(&x);
            let mut gt = mu;
            let mut inv = vec![0.0; n];
            let mut inv2 = vec![0.0; n];
            for k in 0..n {
                let sk = t * t - (er[k] * er[k] + ei[k] * ei[k]);
                inv[k] = 1.0 / sk;
                inv2[k] = inv[k] * inv[k];
                gt -= 2.0 * t * inv[k];
            }
            let uc: Vec<Vec<f64>> = (0..m).map(|j| (0..n).map(|k| 2.0 * (arc[j][k] * er[k] + aic[j][k] * ei[k])).collect()).collect();
            let mut gx = vec![0.0; m];
            for j in 0..m {
                gx[j] = -(0..n).map(|k| uc[j][k] * inv[k]).sum::<f64>();
            }
            let mut h = vec![vec![0.0; m + 1]; m + 1];
            for j in 0..m {
                for l in 0..=j {
                    let mut acc = 0.0;
                    for k in 0..n {
                        acc += uc[j][k] * uc[l][k] * inv2[k] + 2.0 * inv[k] * (arc[j][k] * arc[l][k] + aic[j][k] * aic[l][k]);
                    }
                    h[j][l] = acc;
                }
                h[m][j] = (0..n).map(|k| uc[j][k] * 2.0 * t * inv2[k]).sum();
            }
            h[m][m] = (0..n).map(|k| 4.0 * t * t * inv2[k] - 2.0 * inv[k]).sum();
            for j in 0..=m {
                for l in 0..j {
                    h[l][j] = h[j][l];
                }
            }
            let mut g = gx.clone();
            g.push(gt);
            let neg: Vec<f64> = g.iter().map(|v| -v).collect();
            let step = solve_spd(&h, &neg);
            let dec: f64 = -g.iter().zip(&step).map(|(a, b)| a * b).sum::<f64>();
            let mut a = 1.0;
            let (mut xn, mut tn);
            loop {
                xn = (0..m).map(|j| x[j] + a * step[j]).collect::<Vec<f64>>();
                tn = t + a * step[m];
                let (r1, r2) = resid(&xn);
                if tn > 0.0 && (0..n).all(|k| tn * tn - (r1[k] * r1[k] + r2[k] * r2[k]) > 0.0) {
                    break;
                }
                a *= 0.5;
                if a < 1e-18 {
                    break;
                }
            }
            x = xn;
            t = tn;
            let eps = if n as f64 / mu > 1e-8 * t { 1e-6 } else { 1e-10 };
            let moved = step.iter().zip(x.iter().chain(std::iter::once(&t))).map(|(d, v)| (a * d).abs() / (1.0 + v.abs())).fold(0.0, f64::max);
            if dec < eps || a < 1e-18 || moved < 1e-15 {
                break;
            }
        }
        if std::env::var("CFDEBUG").is_ok() { eprintln!("outer mu={mu:e} t={t:.15} its={cnt}"); }
        mu *= 12.0;
        if n as f64 / mu < 1e-13 * t {
            break;
        }
    }
    let (er, ei) = resid(&x);
    let tfinal = (0..n).map(|k| er[k].hypot(ei[k])).fold(0.0, f64::max);
    (x, tfinal)
}

/// The frequency grid on the bands (in units of pi), as MATLAB lays it out: the band edges plus
/// the multiples of the grid step that lie at least half a step from both edges.
fn grid(bands: &[(f64, f64)], l: usize) -> Vec<f64> {
    let step = 2.0 / l as f64;
    let mut pts: Vec<f64> = vec![];
    for &(fa, fb) in bands {
        let k0 = (fa / step - 1e-9).ceil() as i64;
        let k1 = (fb / step + 1e-9).floor() as i64;
        let mut g: Vec<f64> = vec![fa];
        for k in k0..=k1 {
            let v = k as f64 * step;
            if v - fa >= 0.5 * step - 1e-12 && fb - v >= 0.5 * step - 1e-12 {
                g.push(v);
            }
        }
        if fb > fa {
            g.push(fb);
        }
        pts.extend(g);
    }
    pts
}

fn amp_at(f: &[f64], a: &[C], g: f64) -> C {
    let nb = f.len() / 2;
    for i in 0..nb {
        let (fa, fb) = (f[2 * i], f[2 * i + 1]);
        if g >= fa - 1e-12 && g <= fb + 1e-12 {
            let (a0, a1) = (a[2 * i], a[2 * i + 1]);
            if fb > fa {
                let t = (g - fa) / (fb - fa);
                return (a0.0 + (a1.0 - a0.0) * t, a0.1 + (a1.1 - a0.1) * t);
            }
            return a0;
        }
    }
    (0.0, 0.0)
}

fn band_of(f: &[f64], g: f64) -> usize {
    let nb = f.len() / 2;
    for i in 0..nb {
        if g >= f[2 * i] - 1e-12 && g <= f[2 * i + 1] + 1e-12 {
            return i;
        }
    }
    0
}

pub fn call(it: &mut crate::interp::Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    // N and F.
    let nm = match &a[0] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        Value::Num(_) => return rt("Input N in cfirpm must be numeric, instead it was char."),
        _ => return rt("Input N in cfirpm must be numeric, instead it was cell."),
    };
    let fm = match &a[1] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        Value::Num(_) => return rt("Input F in cfirpm must be numeric, instead it was char."),
        _ => return rt("Input F in cfirpm must be numeric, instead it was cell."),
    };
    // A response function (or {function, extra arguments}) in place of the amplitudes.
    let mut resp: Option<(FuncValue, Vec<Value>)> = None;
    match &a[2] {
        Value::Func(f) => resp = Some((f.clone(), vec![])),
        Value::Cell(c) if matches!(c.data.first(), Some(Value::Func(_))) => {
            if let Some(Value::Func(f)) = c.data.first() {
                resp = Some((f.clone(), c.data[1..].to_vec()));
            }
        }
        _ => {}
    }
    if let Some((FuncValue::Named(name), _)) = &resp {
        if ["lowpass", "highpass", "bandpass", "bandstop", "multiband", "differentiator", "hilbfilt", "allpass", "invsinc"].contains(&name.as_str()) {
            return rt(format!("cfirpm: the predefined response @{name} is not implemented."));
        }
    }
    let am = if resp.is_some() { Matrix::from_dims(vec![1, 0], vec![]) } else { match &a[2] {
        Value::Num(m) if m.kind == Kind::Char => {
            let name = a[2].text().unwrap_or_default();
            return rt(format!("Undefined function '{name}' for input arguments of type 'double'."));
        }
        Value::Num(m) => m.clone(),
        _ => return rt("cfirpm: function handle responses are not implemented."),
    } };
    // Remaining arguments.
    let mut sym: Option<Sym> = None;
    let mut weights: Option<Vec<f64>> = None;
    let mut lgrid = 25.0;
    for (pos, v) in a.iter().enumerate().skip(3) {
        match v {
            Value::Num(m) if m.kind == Kind::Char => {
                let s = v.text().unwrap_or_default();
                match s.to_lowercase().as_str() {
                    "none" => sym = Some(Sym::None),
                    "even" => sym = Some(Sym::Even),
                    "odd" => sym = Some(Sym::Odd),
                    "real" => sym = Some(Sym::Real),
                    "off" | "trace" | "plots" | "both" | "skip_stage2" => {}
                    _ => return rt(format!("Invalid argument \"{s}\" specified.")),
                }
            }
            Value::Num(m) if pos == 3 => weights = Some(m.data.clone()),
            Value::Num(_) => return rt("Expecting a character vector argument."),
            Value::Cell(c) => {
                if let Some(Value::Num(m)) = c.data.first() {
                    if m.numel() == 1 {
                        lgrid = m.data[0];
                    }
                }
            }
            _ => return rt("Expecting a character vector argument."),
        }
    }
    // N.
    if nm.numel() != 1 {
        return rt("Arrays have incompatible sizes for this operation.");
    }
    let nf = nm.data[0];
    if nf.fract() != 0.0 {
        return rt("Argument must be integer-valued.");
    }
    if nf == 0.0 {
        return rt("Arrays have incompatible sizes for this operation.");
    }
    if nf < 0.0 {
        return rt("Incorrect dimensions for matrix multiplication. Check that the number of columns in the first matrix matches the number of rows in the second matrix. To operate on each element of the matrix individually, use TIMES (.*) for elementwise multiplication.");
    }
    let n = nf as usize;
    // F.
    let f: Vec<f64> = fm.data.clone();
    if f.len() % 2 != 0 {
        return rt("F must contain an even number of band edge entries.");
    }
    if f.iter().any(|v| !v.is_finite()) {
        return rt("The sample points must be finite.");
    }
    if f.windows(2).any(|w| w[1] <= w[0]) {
        return rt("Band edges must be monotonically increasing.");
    }
    let has_negative = f.iter().any(|v| *v < 0.0);
    if resp.is_some() && sym.is_none() {
        it.emit(crate::interp::OutputKind::Warning("Frequency response function does not support the 'default' symmetry query.  Assuming symmetry='none'".into()));
    }
    let sym = sym.unwrap_or(if has_negative || resp.is_some() { Sym::None } else { Sym::Even });
    if sym == Sym::None {
        if f.iter().any(|v| v.abs() > 1.0) {
            return rt("Frequency band edges must be in the range [-1,+1] for designs with SYM='none'.");
        }
    } else if f.iter().any(|v| *v < 0.0 || *v > 1.0) {
        return rt(format!("Frequency band edges must be in the range [0,+1] for designs with SYM='{}'.", sym_name(sym)));
    }
    let nb = f.len() / 2;
    if resp.is_none() && am.numel() != f.len() {
        return rt("X and V must be of the same length.");
    }
    let amp: Vec<C> = if resp.is_some() { vec![] } else {
        let im = am.im.clone().unwrap_or_else(|| vec![0.0; am.numel()]);
        (0..am.numel()).map(|i| (am.data[i], im[i])).collect()
    };
    let wts: Vec<f64> = match &weights {
        Some(w) => {
            if w.len() != nb {
                return rt("X and V must be of the same length.");
            }
            w.clone()
        }
        None => vec![1.0; nb],
    };
    if resp.is_none() && sym == Sym::Odd && f[0] == 0.0 && (amp[0].0 != 0.0 || amp[0].1 != 0.0) {
        return rt("Odd symmetric (antisymmetric) filters must have a zero gain at DC.");
    }
    if resp.is_none() && amp.iter().any(|z| !z.0.is_finite() || !z.1.is_finite()) {
        let nan = Matrix::from_dims(vec![1, n + 1], vec![f64::NAN; n + 1]);
        return out(vec![Value::Num(nan), scalar_value(f64::NAN)].into_iter().take(nargout.max(1)).collect());
    }
    // The grid.
    let l = 1usize << nextpow2(0.8 * lgrid * (n as f64 + 1.0));
    let bands: Vec<(f64, f64)> = (0..nb).map(|i| (f[2 * i], f[2 * i + 1])).collect();
    let mut g = grid(&bands, l);
    if sym == Sym::None && f[0] == -1.0 && f[f.len() - 1] == 1.0 {
        let keep = g.len().saturating_sub(2);
        g.truncate(keep);
        g.push(1.0 - 1.0 / (lgrid * (n as f64 + 1.0)));
    }
    if sym == Sym::Real && g.last() == Some(&1.0) && g.len() > 1 {
        let n_ = g.len();
        g.remove(n_ - 2);
    }
    let w: Vec<f64> = g.iter().map(|v| PI * v).collect();
    let nn = g.len();
    let half = n as f64 / 2.0;
    // The basis: columns of complex responses.
    let mut cols: Vec<Vec<C>> = vec![];
    let phase = |wk: f64, off: f64| -> C { ((-wk * off).cos(), (-wk * off).sin()) };
    match sym {
        Sym::None => {
            for k in 0..=n {
                cols.push(w.iter().map(|&wk| phase(wk, k as f64)).collect());
            }
            for k in 0..=n {
                cols.push(w.iter().map(|&wk| cmul((0.0, 1.0), phase(wk, k as f64))).collect());
            }
        }
        Sym::Real => {
            for k in 0..=n {
                cols.push(w.iter().map(|&wk| phase(wk, k as f64)).collect());
            }
        }
        Sym::Even => {
            for k in 0..=n / 2 {
                let mult = if 2 * k == n { 1.0 } else { 2.0 };
                cols.push(w.iter().map(|&wk| { let c = mult * ((half - k as f64) * wk).cos(); let p = phase(wk, half); (c * p.0, c * p.1) }).collect());
            }
        }
        Sym::Odd => {
            for k in 0..(n + 1) / 2 {
                cols.push(w.iter().map(|&wk| { let s = 2.0 * ((half - k as f64) * wk).sin(); let p = phase(wk, half); (-s * p.1, s * p.0) }).collect());
            }
        }
    }
    let m = cols.len();
    // Desired response: weights times (amplitude with the linear phase; a factor j for odd designs).
    let mut dr = vec![0.0; nn];
    let mut di = vec![0.0; nn];
    let mut wt = vec![1.0; nn];
    let mut ar = vec![vec![0.0; m]; nn];
    let mut ai = vec![vec![0.0; m]; nn];
    let mut wts_vec: Vec<f64> = vec![];
    let mut des = vec![(0.0, 0.0); nn];
    let mut fdes: Vec<C> = vec![];
    if let Some((fv, extra)) = &resp {
        let mut args = vec![scalar_value(n as f64), real_row(f.clone()), real_col(g.clone()), real_row(wts.clone())];
        args.extend(extra.iter().cloned());
        let r = it.call_value(fv, args, 2)?;
        if r.len() < 2 {
            return rt("Too many output arguments.");
        }
        let dh = r.first().ok_or_else(|| crate::error::LangError::new("One or more output arguments not assigned during call to the response function.", 0))?.to_matrix()?;
        let dwn = r.get(1).map(|v| v.to_matrix().map(|m| m.numel()).unwrap_or(0)).unwrap_or(nn);
        if dh.numel() != nn || dwn != nn {
            let name = it.call_function("func2str", vec![Value::Func(fv.clone())], 1).ok().and_then(|v| v.into_iter().next()).and_then(|v| v.text().ok()).unwrap_or_default();
            return rt(format!("Incorrect size of results from response function \"{name}\".  Sizes must be the same size as the frequency grid GF."));
        }
        let im = dh.im.clone().unwrap_or_else(|| vec![0.0; nn]);
        fdes = (0..nn).map(|k| (dh.data[k], im[k])).collect();
        if let Some(dw) = r.get(1) {
            let dwm = dw.to_matrix()?;
            if dwm.numel() == nn {
                wts_vec = dwm.data.clone();
            }
        }
    }
    for k in 0..nn {
        let b = band_of(&f, g[k]);
        wt[k] = if wts_vec.is_empty() { wts[b] } else { wts_vec[k] };
        let mut d = if resp.is_some() { fdes[k] } else { cmul(amp_at(&f, &amp, g[k]), phase(w[k], half)) };
        des[k] = d;
        if sym == Sym::Odd && resp.is_none() {
            d = cmul((0.0, 1.0), d);
        }
        dr[k] = wt[k] * d.0;
        di[k] = wt[k] * d.1;
        for j in 0..m {
            ar[k][j] = wt[k] * cols[j][k].0;
            ai[k][j] = wt[k] * cols[j][k].1;
        }
    }
    let (x, t) = minimax(&ar, &ai, &dr, &di);
    // Impulse response.
    let mut hr = vec![0.0; n + 1];
    let mut hi = vec![0.0; n + 1];
    match sym {
        Sym::None => {
            for k in 0..=n {
                hr[k] = x[k];
                hi[k] = x[n + 1 + k];
            }
        }
        Sym::Real => hr.copy_from_slice(&x[..=n]),
        Sym::Even => {
            for k in 0..=n / 2 {
                hr[k] = x[k];
                hr[n - k] = x[k];
            }
        }
        Sym::Odd => {
            // MATLAB returns a zero filter for odd designs given by amplitudes.
            let _ = &x;
        }
    }
    // Signed error at the extremal points.
    let rot = |k: usize| -> C {
        let mut h = (0.0, 0.0);
        for j in 0..m {
            h.0 += x[j] * cols[j][k].0;
            h.1 += x[j] * cols[j][k].1;
        }
        let mut e = (dr[k] / wt[k] - h.0, di[k] / wt[k] - h.1);
        e = cmul(e, ((wk_cos(w[k], half)), (wk_sin(w[k], half))));
        if sym == Sym::Odd {
            e = cmul(e, (0.0, -1.0));
        }
        e
    };
    let active: Vec<usize> = (0..nn)
        .filter(|&k| {
            let e = rot(k);
            wt[k] * e.0.hypot(e.1) >= t * (1.0 - 1e-6)
        })
        .collect();
    let delta = active.first().map_or(t, |&k| if rot(k).0 < 0.0 { -t } else { t });
    let bm = if sym == Sym::None {
        Matrix::complex(vec![1, n + 1], hr, hi)
    } else {
        Matrix::from_dims(vec![1, n + 1], hr)
    };
    let mut outs = vec![Value::Num(bm), scalar_value(delta)];
    if nargout > 2 {
        let (fg, idx): (Vec<f64>, Vec<usize>) = if sym == Sym::Real {
            let mut fg: Vec<f64> = g.iter().rev().map(|v| -v).collect();
            fg.extend(g.iter());
            let mut idx: Vec<usize> = vec![];
            for &k in &active {
                idx.push(nn - 1 - k);
                if k != 0 && k != nn - 1 {
                    idx.push(nn + k);
                }
            }
            idx.sort();
            (fg, idx)
        } else {
            (g.clone(), active.clone())
        };
        let total = fg.len();
        let mirror = |v: Vec<C>| -> Vec<C> {
            if sym == Sym::Real {
                let mut o: Vec<C> = v.iter().rev().map(|c| (c.0, -c.1)).collect();
                o.extend(v);
                o
            } else {
                v
            }
        };
        let hresp: Vec<C> = (0..nn)
            .map(|k| {
                let mut h = (0.0, 0.0);
                for j in 0..m {
                    h.0 += x[j] * cols[j][k].0;
                    h.1 += x[j] * cols[j][k].1;
                }
                let r = cmul(h, (wk_cos(w[k], half), wk_sin(w[k], half)));
                if sym == Sym::Odd { cmul(r, (0.0, -1.0)) } else { r }
            })
            .collect();
        let amp_re: Vec<C> = (0..nn).map(|k| if resp.is_some() { cmul(des[k], (wk_cos(w[k], half), wk_sin(w[k], half))) } else { amp_at(&f, &amp, g[k]) }).collect();
        let err: Vec<C> = (0..nn).map(|k| (amp_re[k].0 - hresp[k].0, amp_re[k].1 - hresp[k].1)).collect();
        let desv = mirror(des.clone());
        let hv = mirror(hresp);
        let errv = mirror(err);
        let wtv: Vec<f64> = if sym == Sym::Real { wt.iter().rev().chain(wt.iter()).cloned().collect() } else { wt.clone() };
        let cplx = |v: Vec<C>| Value::Num(Matrix::complex(vec![total, 1], v.iter().map(|c| c.0).collect(), v.iter().map(|c| c.1).collect()));
        let fextr: Vec<f64> = idx.iter().map(|&k| fg[k]).collect();
        let names = vec!["fgrid", "des", "wt", "H", "error", "iextr", "fextr"];
        let vals = vec![
            real_col(fg),
            cplx(desv),
            real_col(wtv),
            cplx(hv),
            cplx(errv),
            real_col(idx.iter().map(|&k| (k + 1) as f64).collect()),
            real_col(fextr),
        ];
        outs.push(make_struct(&names, vals));
    }
    out(outs.into_iter().take(nargout.max(1)).collect())
}

fn wk_cos(w: f64, half: f64) -> f64 {
    (w * half).cos()
}
fn wk_sin(w: f64, half: f64) -> f64 {
    (w * half).sin()
}

fn make_struct(names: &[&str], vals: Vec<Value>) -> Value {
    Value::Struct(StructArray { dims: vec![1, 1], fields: names.iter().map(|n| n.to_string()).collect(), elems: vec![vals] })
}
