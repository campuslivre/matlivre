//! `pmusic`, `peig`, `rootmusic` and `rooteig`: eigenspace (MUSIC and eigenvector) frequency
//! estimation from the singular value decomposition of a windowed data matrix, or from a given
//! correlation matrix.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;
use std::f64::consts::PI;

pub const NAMES: &[&str] = &["pmusic", "peig", "rootmusic", "rooteig"];

type C = (f64, f64);

fn cmul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

struct Analysis {
    n: usize,
    lam: Vec<f64>,
    vecs: Vec<Vec<C>>, // columns, sorted by decreasing eigenvalue
    sig: usize,
    complex: bool,
    row_scale: f64,
}

#[derive(Default)]
struct Parsed {
    nfft: Option<usize>,
    freqs: Option<Vec<f64>>,
    fs: Option<f64>,
    nwin: Option<Value>,
    noverlap: Option<usize>,
    range: Option<String>,
    corr: bool,
}

fn matrix_arg(v: &Value, pos: usize, label: &str, root: bool) -> R<Matrix> {
    match v {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => Ok(m.clone()),
        _ => rt(format!(
            "Expected input number {pos}, {label}, to be one of these types:  {}",
            if root { "single, double" } else { "double, single" }
        )),
    }
}

fn parse_tail(fname: &str, a: &[Value], root: bool) -> R<Parsed> {
    let mut p = Parsed::default();
    let mut numeric_pos = 0;
    for v in &a[2..] {
        match v {
            Value::Num(m) if m.kind == Kind::Char => {
                let s = v.text().unwrap_or_default().to_lowercase();
                match s.as_str() {
                    "corr" => p.corr = true,
                    "ev" => {}
                    "half" | "onesided" | "whole" | "twosided" | "centered" => p.range = Some(s),
                    _ => return rt("Expected input to match one of these values:  'half', 'onesided', 'whole', 'twosided', 'adapt', 'unity', 'eigen', 'corr', 'ev', 'centered', 'power', 'psd', 'ms', 'reassigned', 'confidencelevel', 'mean', 'maxhold', 'minhold', 'mimo'  The input, '".to_string() + &v.text().unwrap_or_default() + "', did not match any of the valid values."),
                }
            }
            Value::Num(m) => {
                if root {
                    // rootmusic(x,p,fs)
                    if numeric_pos == 0 {
                        if m.numel() > 1 {
                            return rt("The sampling frequency must be a scalar.");
                        }
                        if m.numel() == 0 {
                            p.fs = Some(1.0);
                        }
                        if m.numel() == 1 {
                            if !(m.data[0] > 0.0) {
                                return rt("Expected fs to be positive.");
                            }
                            p.fs = Some(m.data[0]);
                        }
                    }
                    numeric_pos += 1;
                    continue;
                }
                match numeric_pos {
                    0 => {
                        if m.numel() > 1 {
                            p.freqs = Some(m.data.clone());
                        } else if m.numel() == 1 {
                            let v = m.data[0];
                            if v.fract() != 0.0 {
                                return rt("Argument must be integer-valued.");
                            }
                            if v < 1.0 {
                                return rt("Array indices must be positive integers or logical values.");
                            }
                            p.nfft = Some(v as usize);
                        }
                    }
                    1 => {
                        if m.numel() == 0 {
                            p.fs = Some(1.0);
                        }
                        if m.numel() == 1 {
                            p.fs = Some(m.data[0]);
                        }
                    }
                    2 => p.nwin = Some(v.clone()),
                    3 => {
                        if m.numel() == 1 {
                            p.noverlap = Some(m.data[0].max(0.0) as usize);
                        }
                    }
                    _ => {}
                }
                numeric_pos += 1;
            }
            _ => {}
        }
    }
    let _ = fname;
    Ok(p)
}

fn analyze(it: &mut Interpreter, x: &Matrix, pv: &[f64], par: &Parsed, weighted: bool) -> R<Analysis> {
    let complex = x.im.is_some();
    let p1 = pv[0];
    let (vecs, lam): (Vec<Vec<C>>, Vec<f64>);
    let n;
    let mut row_scale = 1.0;
    if par.corr {
        if x.dims.len() != 2 || x.dims[0] != x.dims[1] {
            return rt("To use the 'corr' option, specify the input 'x' as a square matrix.");
        }
        n = x.dims[0];
        let (v, ev) = hermitian_eig(it, x)?;
        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|&a, &b| ev[b].partial_cmp(&ev[a]).unwrap_or(std::cmp::Ordering::Equal));
        vecs = idx.iter().map(|&j| v[j].clone()).collect();
        lam = idx.iter().map(|&j| ev[j]).collect();
    } else {
        let (rows0, cols0) = (x.dims[0], x.dims[1]);
        let xim = x.im.clone().unwrap_or_else(|| vec![0.0; x.numel()]);
        let is_vec = rows0 == 1 || cols0 == 1;
        let (mat_re, mat_im, rows, cols, norm): (Vec<f64>, Vec<f64>, usize, usize, f64);
        if is_vec {
            let len = x.numel();
            let (nwin, wvec): (usize, Option<Vec<f64>>) = match &par.nwin {
                Some(Value::Num(m)) if m.numel() > 1 => (m.numel(), Some(m.data.clone())),
                Some(Value::Num(m)) if m.numel() == 1 => (m.data[0] as usize, None),
                _ => ((2.0 * p1) as usize, None),
            };
            if nwin >= len {
                return if par.nwin.is_some() {
                    rt("The segment length, NW, must be smaller than the signal length.")
                } else {
                    rt(format!("Expected M to be a scalar with value < {len}."))
                };
            }
            let nov = par.noverlap.unwrap_or(nwin - 1);
            if nov >= nwin {
                return rt("Frame overlap P must be less than the buffer size N.");
            }
            let hop = nwin - nov;
            let k = (len - nov + hop - 1) / hop;
            cols = nwin;
            rows = k;
            let mut re = vec![0.0; rows * cols];
            let mut im = vec![0.0; rows * cols];
            for r in 0..rows {
                for c in 0..cols {
                    let src = r * hop + c;
                    if src < len {
                        let w = wvec.as_ref().map_or(1.0, |w| w[c]);
                        // MATLAB's data matrix holds each segment in reverse order
                        let cc = cols - 1 - c;
                        re[r + rows * cc] = x.data[src] * w;
                        im[r + rows * cc] = xim[src] * w;
                    }
                }
            }
            (mat_re, mat_im) = (re, im);
            norm = if par.nwin.is_some() { (k as f64 - 1.0).max(1.0) } else { k as f64 };
            if par.nwin.is_some() {
                row_scale = norm / ((len - nwin) as f64);
            }
        } else {
            rows = rows0;
            cols = cols0;
            let wvec = match &par.nwin {
                Some(Value::Num(m)) if m.numel() > 1 => Some(m.data.clone()),
                _ => None,
            };
            let mut re = x.data.clone();
            let mut im = xim.clone();
            if let Some(w) = wvec {
                for r in 0..rows {
                    for c in 0..cols {
                        re[r + rows * c] *= w[c];
                        im[r + rows * c] *= w[c];
                    }
                }
            }
            (mat_re, mat_im) = (re, im);
            norm = rows as f64;
            row_scale = rows as f64;
        }
        n = cols;
        let has_im = complex || mat_im.iter().any(|v| *v != 0.0);
        if rows < cols {
            if weighted {
                return rt("The segment length and overlap specified result in a data matrix with more columns than rows.");
            }
            // correlation matrix X^H X / norm, then its eigen-decomposition
            let mut cr = vec![0.0; n * n];
            let mut ci = vec![0.0; n * n];
            for a in 0..n {
                for b in 0..n {
                    let (mut sr, mut si) = (0.0, 0.0);
                    for r in 0..rows {
                        let t = cmul((mat_re[r + rows * a], -mat_im[r + rows * a]), (mat_re[r + rows * b], mat_im[r + rows * b]));
                        sr += t.0;
                        si += t.1;
                    }
                    cr[a + n * b] = sr / norm;
                    ci[a + n * b] = si / norm;
                }
            }
            let cm = if has_im { Matrix::complex(vec![n, n], cr, ci) } else { Matrix::from_dims(vec![n, n], cr) };
            let (v, ev) = hermitian_eig(it, &cm)?;
            let mut idx: Vec<usize> = (0..n).collect();
            idx.sort_by(|&a, &b| ev[b].partial_cmp(&ev[a]).unwrap_or(std::cmp::Ordering::Equal));
            vecs = idx.iter().map(|&j| v[j].clone()).collect();
            lam = idx.iter().map(|&j| ev[j].max(0.0)).collect();
        } else {
            let xm = if has_im { Matrix::complex(vec![rows, cols], mat_re, mat_im) } else { Matrix::from_dims(vec![rows, cols], mat_re) };
            let res = it.call_function("svd", vec![Value::Num(xm), scalar_value(0.0)], 3)?;
            let sv = res[1].to_matrix()?;
            let vm = res[2].to_matrix()?;
            let vim = vm.im.clone().unwrap_or_else(|| vec![0.0; vm.numel()]);
            let s: Vec<f64> = (0..n).map(|i| sv.data[i + sv.dims[0] * i]).collect();
            lam = s.iter().map(|v| v * v / norm).collect();
            vecs = (0..n).map(|j| (0..n).map(|i| (vm.data[i + n * j], vim[i + n * j])).collect()).collect();
        }
    }
    // signal subspace dimension
    let sig = if pv.len() >= 2 {
        let thr = pv[1] * lam.last().copied().unwrap_or(0.0);
        let count = lam.iter().filter(|&&l| l >= thr).count();
        (p1.min(count as f64)) as usize
    } else {
        p1 as usize
    };
    let sig = sig.min(n);
    Ok(Analysis { n, lam, vecs, sig, complex, row_scale })
}

fn hermitian_eig(it: &mut Interpreter, x: &Matrix) -> R<(Vec<Vec<C>>, Vec<f64>)> {
    let n = x.dims[0];
    let r = it.call_function("eig", vec![Value::Num(x.clone())], 2)?;
    let v = r[0].to_matrix()?;
    let d = r[1].to_matrix()?;
    let vim = v.im.clone().unwrap_or_else(|| vec![0.0; v.numel()]);
    let ev: Vec<f64> = (0..n).map(|i| d.data[i + n * i]).collect();
    let cols = (0..n).map(|j| (0..n).map(|i| (v.data[i + n * j], vim[i + n * j])).collect()).collect();
    Ok((cols, ev))
}

fn check_p(a: &[Value]) -> R<Vec<f64>> {
    let pm = match &a[1] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt("Expected input number 2, P, to be numeric."),
    };
    if pm.numel() == 0 || pm.numel() > 2 {
        return rt("Expected input number 2, P, to be a vector of 1 or 2 elements.");
    }
    Ok(pm.data.clone())
}

fn freq_grid(complex: bool, range: &Option<String>, nfft: usize) -> Vec<i64> {
    let r = range.clone().unwrap_or_else(|| if complex { "whole".into() } else { "half".into() });
    let n = nfft as i64;
    match r.as_str() {
        "whole" | "twosided" => (0..n).collect(),
        "centered" => {
            let start = if n % 2 == 0 { -(n / 2 - 1) } else { -((n - 1) / 2) };
            (0..n).map(|k| start + k).collect()
        }
        _ => {
            let cnt = if n % 2 == 0 { n / 2 + 1 } else { (n + 1) / 2 };
            (0..cnt).collect()
        }
    }
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    let root = name.starts_with("root");
    let max_out = if root { 2 } else { 4 };
    if nargout > max_out {
        return rt("Too many output arguments.");
    }
    let x = matrix_arg(&a[0], 1, "X", root)?;
    if x.numel() == 0 {
        return rt("Expected input number 1, X, to be nonempty.");
    }
    let single = x.kind == Kind::Single;
    let pv = check_p(a)?;
    let par = parse_tail(name, a, root)?;
    if !root {
        // p must be an integer
        if pv[0].is_finite() && pv[0].fract() != 0.0 {
            return rt("The number of complex sinusoids specified in P must be an integer.");
        }
    } else if pv[0].is_finite() && pv[0].fract() != 0.0 {
        return rt("Argument must be integer-valued.");
    }
    if pv[0] < 0.0 || pv[0] == 0.0 {
        return rt("Expected M to be nonnegative.");
    }
    if !pv[0].is_finite() && (!par.corr || root) {
        return rt(if root { "Argument must be integer-valued." } else { "Expected M to be integer-valued." });
    }
    let an = analyze(it, &x, &pv, &par, name.ends_with("eig"))?;
    if root {
        return root_estimate(it, name, &an, &par, nargout, single);
    }
    // pseudospectrum
    let nfft = par.nfft.unwrap_or(256);
    let weights: Vec<f64> = (0..an.n).map(|k| if name == "peig" { 1.0 / (an.lam[k] * an.row_scale) } else { 1.0 }).collect();
    let noise: Vec<usize> = (an.sig..an.n).collect();
    let (omegas, ks): (Vec<f64>, Vec<i64>) = if let Some(f) = &par.freqs {
        // frequencies given in Hz when fs is supplied, else normalised (rad/sample)
        let fsv = par.fs.unwrap_or(1.0);
        let om: Vec<f64> = if par.fs.is_some() { f.iter().map(|v| 2.0 * PI * v / fsv).collect() } else { f.clone() };
        (om, vec![])
    } else {
        let ks = freq_grid(an.complex, &par.range, nfft);
        (ks.iter().map(|&k| 2.0 * PI * k as f64 / nfft as f64).collect(), ks)
    };
    let _ = ks;
    let noise_small = noise.len() <= an.sig;
    let mut s = Vec::with_capacity(omegas.len());
    for &w in &omegas {
        let term = |k: usize| -> f64 {
            let mut re = 0.0;
            let mut im = 0.0;
            for (i, v) in an.vecs[k].iter().enumerate() {
                let (c, sn) = ((w * i as f64).cos(), (w * i as f64).sin());
                // conj(v) * e^{-jwi}
                let cv = (v.0, -v.1);
                let t = cmul(cv, (c, sn));
                re += t.0;
                im += t.1;
            }
            (re * re + im * im) * weights[k]
        };
        let denom = if name == "pmusic" && !noise_small {
            an.n as f64 - (0..an.sig).map(|k| term(k)).sum::<f64>()
        } else {
            noise.iter().map(|&k| term(k)).sum::<f64>()
        };
        s.push(1.0 / denom);
    }
    let freq_out: Vec<f64> = if let Some(f) = &par.freqs {
        f.clone()
    } else {
        let fsv = par.fs;
        omegas.iter().map(|w| match fsv { Some(fs) => w * fs / (2.0 * PI), None => *w }).collect()
    };
    let cast = |m: Matrix| -> Value {
        let mut m = m;
        if single {
            m.data.iter_mut().for_each(|e| *e = *e as f32 as f64);
            m.kind = Kind::Single;
        }
        Value::Num(m)
    };
    let nn = s.len();
    let mut outs = vec![cast(Matrix::from_dims(vec![nn, 1], s)), cast(Matrix::from_dims(vec![nn, 1], freq_out))];
    if nargout > 2 {
        let m = noise.len();
        let mut re = vec![0.0; an.n * m];
        let mut im = vec![0.0; an.n * m];
        for (j, &k) in noise.iter().enumerate() {
            for i in 0..an.n {
                re[i + an.n * j] = an.vecs[k][i].0;
                im[i + an.n * j] = an.vecs[k][i].1;
            }
        }
        let vm = if an.complex { Matrix::complex(vec![an.n, m], re, im) } else { Matrix::from_dims(vec![an.n, m], re) };
        outs.push(cast(vm));
    }
    if nargout > 3 {
        outs.push(cast(Matrix::from_dims(vec![an.n, 1], an.lam.clone())));
    }
    out(outs)
}

fn root_estimate(it: &mut Interpreter, name: &str, an: &Analysis, par: &Parsed, nargout: usize, single: bool) -> Ret {
    let n = an.n;
    let eig_weighted = name == "rooteig";
    let m = an.sig;
    if !an.complex && m % 2 == 1 {
        return rt("Real signals require an even number p of complex sinusoids.");
    }
    // polynomial coefficients: sums of the diagonals of C = sum_k v_k v_k^H (/ lambda_k)
    let noise: Vec<usize> = (an.sig..n).collect();
    let mut cm = vec![(0.0, 0.0); n * n];
    for &k in &noise {
        let wgt = if eig_weighted { 1.0 / an.lam[k] } else { 1.0 };
        for i in 0..n {
            for j in 0..n {
                let t = cmul(an.vecs[k][i], (an.vecs[k][j].0, -an.vecs[k][j].1));
                cm[i + n * j].0 += wgt * t.0;
                cm[i + n * j].1 += wgt * t.1;
            }
        }
    }
    let mut coef = vec![(0.0, 0.0); 2 * n - 1];
    for l in -(n as i64 - 1)..(n as i64) {
        let mut acc = (0.0, 0.0);
        for i in 0..n as i64 {
            let j = i + l;
            if j >= 0 && j < n as i64 {
                let v = cm[(j as usize) + n * (i as usize)];
                acc.0 += v.0;
                acc.1 += v.1;
            }
        }
        coef[(l + n as i64 - 1) as usize] = acc;
    }
    // roots of the polynomial with these coefficients (highest power first)
    let re: Vec<f64> = coef.iter().map(|c| c.0).collect();
    let im: Vec<f64> = coef.iter().map(|c| c.1).collect();
    let cvec = if im.iter().any(|v| v.abs() > 0.0) { Matrix::complex(vec![2 * n - 1, 1], re, im) } else { Matrix::from_dims(vec![2 * n - 1, 1], re) };
    let rr = it.call_function("roots", vec![Value::Num(cvec)], 1)?.remove(0).to_matrix()?;
    let rim = rr.im.clone().unwrap_or_else(|| vec![0.0; rr.numel()]);
    let mut roots: Vec<C> = (0..rr.numel()).map(|i| (rr.data[i], rim[i])).filter(|z| z.0.hypot(z.1) < 1.0).collect();
    roots.sort_by(|a, b| b.0.hypot(b.1).partial_cmp(&a.0.hypot(a.1)).unwrap_or(std::cmp::Ordering::Equal));
    roots.truncate(m);
    let w: Vec<f64> = roots.iter().map(|z| z.1.atan2(z.0)).collect();
    // powers from the signal eigenvalues
    let sigma2 = if noise.is_empty() { 0.0 } else { noise.iter().map(|&k| an.lam[k]).sum::<f64>() / noise.len() as f64 };
    let positive: Vec<usize> = if an.complex { (0..w.len()).collect() } else { (0..w.len()).filter(|&i| w[i] > 0.0).collect() };
    let rows: Vec<usize> = (0..positive.len()).collect();
    // G[k][i] = |v_k^H e(w_i)|^2 with e = exp(-j w n)
    let mut g = vec![vec![0.0; positive.len()]; rows.len()];
    for (ri, &k) in rows.iter().enumerate() {
        for (ci, &wi) in positive.iter().enumerate() {
            let mut re = 0.0;
            let mut im = 0.0;
            for i in 0..n {
                let ph = w[wi] * i as f64;
                let t = cmul((an.vecs[k][i].0, -an.vecs[k][i].1), (ph.cos(), ph.sin()));
                re += t.0;
                im += t.1;
            }
            g[ri][ci] = re * re + im * im;
        }
    }
    let rhs: Vec<f64> = rows.iter().map(|&k| an.lam[k] - sigma2).collect();
    let mut pw = solve_square(&g, &rhs);
    pw.iter_mut().for_each(|v| *v *= an.row_scale);
    let wout: Vec<f64> = match par.fs {
        Some(fs) => w.iter().map(|v| v * fs / (2.0 * PI)).collect(),
        None => w.clone(),
    };
    let cast = |m: Matrix| -> Value {
        let mut m = m;
        if single {
            m.data.iter_mut().for_each(|e| *e = *e as f32 as f64);
            m.kind = Kind::Single;
        }
        Value::Num(m)
    };
    let mut outs = vec![cast(Matrix::from_dims(vec![wout.len(), 1], wout))];
    if nargout > 1 {
        outs.push(cast(Matrix::from_dims(vec![pw.len(), 1], pw)));
    }
    out(outs)
}

fn solve_square(g: &[Vec<f64>], rhs: &[f64]) -> Vec<f64> {
    let n = rhs.len();
    if n == 0 {
        return vec![];
    }
    let mut a: Vec<Vec<f64>> = g.to_vec();
    let mut b = rhs.to_vec();
    for c in 0..n {
        let mut piv = c;
        for r in c + 1..n {
            if a[r][c].abs() > a[piv][c].abs() {
                piv = r;
            }
        }
        a.swap(c, piv);
        b.swap(c, piv);
        if a[c][c].abs() < 1e-300 {
            continue;
        }
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            for k in c..n {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for k in r + 1..n {
            s -= a[r][k] * x[k];
        }
        x[r] = if a[r][r].abs() < 1e-300 { 0.0 } else { s / a[r][r] };
    }
    x
}
