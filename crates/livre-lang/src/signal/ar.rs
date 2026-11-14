//! Autoregressive estimators and lattice tools: `aryule`, `arburg`, `arcov`, `armcov`,
//! `rlevinson`, `schurrc`, `latcfilt`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["aryule", "arburg", "arcov", "armcov", "rlevinson", "schurrc", "latcfilt"];

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "rlevinson" => rlevinson(it, a, nargout),
        "schurrc" => schurrc(it, a, nargout),
        "latcfilt" => latcfilt(a, nargout),
        _ => ar_estimator(it, name, a, nargout),
    }
}

fn columns(m: &Matrix) -> Vec<Vec<f64>> {
    if m.dims[0] == 1 {
        return vec![m.data.clone()];
    }
    let r = m.dims[0].max(1);
    (0..m.numel() / r).map(|c| m.data[c * r..(c + 1) * r].to_vec()).collect()
}

/// Least squares through Householder QR with column pivoting; for a rank-deficient matrix the
/// basic solution (at most `rank` nonzero entries) is returned, as MATLAB's backslash does.
pub(crate) fn lstsq(a: &[Vec<f64>], y: &[f64]) -> Vec<f64> {
    let rows = y.len();
    let cols = a.len();
    let mut m: Vec<Vec<f64>> = a.to_vec();
    let mut b = y.to_vec();
    let mut perm: Vec<usize> = (0..cols).collect();
    let mut rank = 0;
    let mut r11 = 0.0_f64;
    for k in 0..cols.min(rows) {
        // Pivot: the remaining column with the largest norm below row k.
        let (mut best, mut bj) = (-1.0, k);
        for j in k..cols {
            let nrm: f64 = (k..rows).map(|i| m[j][i] * m[j][i]).sum();
            if nrm > best {
                best = nrm;
                bj = j;
            }
        }
        m.swap(k, bj);
        perm.swap(k, bj);
        let norm = best.max(0.0).sqrt();
        if k == 0 {
            r11 = norm;
        }
        if norm <= r11 * (rows.max(cols) as f64) * f64::EPSILON || norm == 0.0 {
            break;
        }
        rank = k + 1;
        let alpha = if m[k][k] > 0.0 { -norm } else { norm };
        let mut v: Vec<f64> = (k..rows).map(|i| m[k][i]).collect();
        v[0] -= alpha;
        let vn: f64 = v.iter().map(|t| t * t).sum();
        if vn == 0.0 {
            continue;
        }
        for j in k..cols {
            let dot: f64 = (k..rows).map(|i| v[i - k] * m[j][i]).sum();
            for i in k..rows {
                m[j][i] -= 2.0 * dot / vn * v[i - k];
            }
        }
        let dot: f64 = (k..rows).map(|i| v[i - k] * b[i]).sum();
        for i in k..rows {
            b[i] -= 2.0 * dot / vn * v[i - k];
        }
    }
    let mut xp = vec![0.0; cols];
    for k in (0..rank).rev() {
        let mut s = b[k];
        for j in k + 1..rank {
            s -= m[j][k] * xp[j];
        }
        xp[k] = s / m[k][k];
    }
    let mut x = vec![0.0; cols];
    for (k, &pj) in perm.iter().enumerate() {
        x[pj] = xp[k];
    }
    x
}

fn step_up_poly(k: &[f64]) -> Vec<f64> {
    let mut a = vec![1.0];
    for (m, &km) in k.iter().enumerate() {
        let mut next = vec![0.0; m + 2];
        next[0] = 1.0;
        for i in 1..=m {
            next[i] = a[i] + km * a[m + 1 - i];
        }
        next[m + 1] = km;
        a = next;
    }
    a
}

fn ar_estimator(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let max_out = if matches!(name, "arcov" | "armcov") { 2 } else { 3 };
    if nargout > max_out {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        Value::Num(_) => return rt("Expected X to be one of these types:  single, double"),
        other => return rt(format!("Expected X to be one of these types:  single, double  Instead its type was {}.", other.class_name())),
    };
    if x.numel() == 0 {
        return rt("Expected X to be nonempty.");
    }
    let om = crate::builtins::m_arg(a, 1, name)?;
    if om.numel() != 1 {
        return rt("Expected ORDER to be a scalar.");
    }
    let p = om.data[0];
    if p < 0.0 {
        return rt("Expected ORDER to be nonnegative.");
    }
    if p.fract() != 0.0 {
        return rt("Expected ORDER to be integer-valued.");
    }
    let p = p as usize;
    if x.im.is_some() {
        return rt("Complex input is not supported.");
    }
    let cols = columns(&x);
    let n = cols[0].len();
    match name {
        "arburg" if n < p + 1 => return rt(format!("The length of input vector X must at least {}.", p + 1)),
        "arcov" if (n as f64) <= 1.5 * p as f64 => {
            return rt("When X is a vector, it must have length greater than 3/2 times the model order.")
        }
        "armcov" if n <= 2 * p => return rt("When X is a vector, it must have length greater than 2 times the model order."),
        _ => {}
    }
    let (mut polys, mut errs, mut ks) = (vec![], vec![], vec![]);
    for c in &cols {
        let (poly, e, k) = match name {
            "aryule" => {
                let mut r = vec![0.0; p + 1];
                for (lag, rv) in r.iter_mut().enumerate() {
                    *rv = (0..n.saturating_sub(lag)).map(|i| c[i] * c[i + lag]).sum::<f64>() / n as f64;
                }
                let res = it.call_function("levinson", vec![real_col(r), scalar_value(p as f64)], 3)?;
                (res[0].to_matrix()?.data, res[1].to_matrix()?.data[0], res[2].to_matrix()?.data)
            }
            "arburg" => {
                let mut f = c.clone();
                let mut b = c.clone();
                let mut e = c.iter().map(|v| v * v).sum::<f64>() / n as f64;
                let mut k = vec![];
                for m in 1..=p {
                    let num: f64 = (m..n).map(|i| f[i] * b[i - 1]).sum();
                    let den: f64 = (m..n).map(|i| f[i] * f[i] + b[i - 1] * b[i - 1]).sum();
                    let km = -2.0 * num / den;
                    let (fo, bo) = (f.clone(), b.clone());
                    for i in m..n {
                        f[i] = fo[i] + km * bo[i - 1];
                        b[i] = bo[i - 1] + km * fo[i];
                    }
                    e *= 1.0 - km * km;
                    k.push(km);
                }
                (step_up_poly(&k), e, k)
            }
            _ => {
                let backward = name == "armcov";
                let mut rows = vec![];
                let mut y = vec![];
                for t in p..n {
                    rows.push((1..=p).map(|k| c[t - k]).collect::<Vec<_>>());
                    y.push(c[t]);
                }
                if backward {
                    for t in 0..n - p {
                        rows.push((1..=p).map(|k| c[t + k]).collect::<Vec<_>>());
                        y.push(c[t]);
                    }
                }
                let acols: Vec<Vec<f64>> = (0..p).map(|j| rows.iter().map(|r| r[j]).collect()).collect();
                let coef = if p == 0 { vec![] } else { lstsq(&acols, &y) };
                let mut res = 0.0;
                for (r, yy) in rows.iter().zip(&y) {
                    let pred: f64 = r.iter().zip(&coef).map(|(a, b)| a * b).sum();
                    res += (yy - pred) * (yy - pred);
                }
                let denom = if backward { 2 * (n - p) } else { n - p };
                let mut poly = vec![1.0];
                poly.extend(coef.iter().map(|v| -v));
                (poly, res / denom as f64, vec![])
            }
        };
        polys.push(poly);
        errs.push(e);
        ks.push(k);
    }
    let single = x.kind == Kind::Single;
    let pl = p + 1;
    let amat = if cols.len() == 1 {
        Matrix::from_dims(vec![1, pl], polys[0].clone())
    } else {
        let mut d = vec![0.0; pl * cols.len()];
        for (c, poly) in polys.iter().enumerate() {
            for (j, &v) in poly.iter().enumerate() {
                d[j * cols.len() + c] = v;
            }
        }
        Matrix::from_dims(vec![cols.len(), pl], d)
    };
    let mut outv = vec![Value::Num(amat)];
    if nargout > 1 {
        outv.push(if cols.len() == 1 { scalar_value(errs[0]) } else { real_col(errs.clone()) });
    }
    if nargout > 2 {
        if cols.len() == 1 {
            outv.push(real_col(ks[0].clone()));
        } else {
            let mut d = vec![0.0; p * cols.len()];
            for (c, k) in ks.iter().enumerate() {
                for (j, &v) in k.iter().enumerate() {
                    d[c * p + j] = v;
                }
            }
            outv.push(Value::Num(Matrix::from_dims(vec![p, cols.len()], d)));
        }
    }
    if single {
        outv = outv.into_iter().map(crate::signal::to_single).collect();
    }
    out(outv)
}

fn rlevinson(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if nargout > 4 {
        return rt("Too many output arguments.");
    }
    if let Value::Num(m) = &a[0] {
        if !matches!(m.kind, Kind::Double | Kind::Single) {
            return rt(format!("Input A(Prediction Polynomial) in rlevinson must be double/single, instead it was {}.", m.kind.name()));
        }
    }
    let r = it.call_function("poly2ac", vec![a[0].clone(), a[1].clone()], 1)?;
    let mut outv = vec![r[0].clone()];
    if nargout > 1 {
        let m = a[0].to_matrix()?;
        let av = m.data.clone();
        let p = av.len() - 1;
        // Step down to find the polynomials of every order.
        let a0 = av[0];
        let mut cur: Vec<f64> = av.iter().map(|v| v / a0).collect();
        let mut polys = vec![vec![]; p + 1];
        let mut k = vec![0.0; p];
        for ord in (1..=p).rev() {
            polys[ord] = cur.clone();
            let km = cur[ord];
            k[ord - 1] = km;
            let den = 1.0 - km * km;
            let mut next = vec![1.0; ord];
            for i in 1..ord {
                next[i] = (cur[i] - km * cur[ord - i]) / den;
            }
            cur = next;
        }
        polys[0] = vec![1.0];
        let mut u = vec![0.0; (p + 1) * (p + 1)];
        for (ord, poly) in polys.iter().enumerate() {
            for (i, &v) in poly.iter().rev().enumerate() {
                u[ord * (p + 1) + i] = v;
            }
        }
        outv.push(Value::Num(Matrix::from_dims(vec![p + 1, p + 1], u)));
        if nargout > 2 {
            outv.push(real_col(k.clone()));
        }
        if nargout > 3 {
            let e = a[1].to_matrix()?.data[0];
            let mut es = vec![0.0; p];
            es[p - 1] = e;
            for ord in (1..p).rev() {
                es[ord - 1] = es[ord] / (1.0 - k[ord] * k[ord]);
            }
            outv.push(real_row(es));
        }
    }
    out(outv)
}

fn schurrc(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 1 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let m = match &a[0] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt("Output argument \"K\" (and possibly others) not assigned a value in the execution with \"schurrc\" function."),
    };
    if m.numel() < 2 {
        return rt("Output argument \"K\" (and possibly others) not assigned a value in the execution with \"schurrc\" function.");
    }
    let r = Value::Num(Matrix::from_dims(vec![m.numel(), 1], m.data.clone()));
    let res = it.call_function("ac2rc", vec![r], 2)?;
    let mut outv = vec![res[0].clone()];
    if nargout > 1 {
        let k = res[0].to_matrix()?.data;
        let e = m.data[0] * k.iter().map(|v| 1.0 - v * v).product::<f64>();
        outv.push(scalar_value(e));
    }
    Ok(Some(outv))
}

fn latcfilt(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let km = match &a[0] {
        Value::Num(m) if m.kind == Kind::Double && m.numel() > 0 && m.dims.len() == 2 => m.clone(),
        _ => return rt("Lattice filter coefficients (K) must be a double precision 2-D array."),
    };
    let (v, x) = if a.len() >= 3 {
        match &a[1] {
            Value::Num(m) if m.kind == Kind::Double && m.numel() > 0 && m.dims.len() == 2 => (Some(m.clone()), a[2].clone()),
            _ => return rt("Ladder filter coefficients (V) must be a double precision 2-D array."),
        }
    } else {
        (None, a[1].clone())
    };
    let xm = match &x {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt("Input signal must be numeric."),
    };
    if xm.numel() == 0 || (v.is_none() && xm.kind != Kind::Double) {
        return rt("Ladder filter coefficients (V) must be a double precision 2-D array.");
    }
    if a.len() > 3 {
        return rt("Must specify 'ic' when using initial conditions");
    }
    // Rows of K are separate filters when K is a matrix.
    let krows: Vec<Vec<(f64, f64)>> = if km.dims[0] == 1 || km.dims[1] == 1 {
        vec![(0..km.numel()).map(|i| (km.data[i], km.im_at(i))).collect()]
    } else {
        (0..km.dims[1]).map(|c| (0..km.dims[0]).map(|r| (km.data[c * km.dims[0] + r], km.im_at(c * km.dims[0] + r))).collect()).collect()
    };
    let mm = krows[0].len();
    if let Some(vm) = &v {
        if vm.numel() > mm + 1 {
            return rt("For IIR lattice-ladders, length(V) must be <= 1+length(K).");
        }
    }
    let xcols = columns(&xm);
    let xrow = xm.dims[0] == 1;
    let n = xcols[0].len();
    let nf = krows.len().max(xcols.len());
    let mut fre = vec![];
    let mut fim = vec![];
    let mut gre = vec![];
    let mut gim = vec![];
    for j in 0..nf {
        let kk = &krows[if krows.len() == 1 { 0 } else { j }];
        let xx = &xcols[if xcols.len() == 1 { 0 } else { j }];
        // State: g_{m-1}(n-1) for each stage.
        let mut gprev = vec![(0.0, 0.0); mm + 1];
        let mut f_out = vec![(0.0, 0.0); n];
        let mut g_out = vec![(0.0, 0.0); n];
        for t in 0..n {
            let mut gnew = vec![(0.0, 0.0); mm + 1];
            let (fo, go);
            if let Some(vm) = &v {
                // All-pole lattice with a ladder on the backward signals.
                let mut f = (xx[t], 0.0);
                let mut fs = vec![(0.0, 0.0); mm + 1];
                fs[mm] = f;
                for s in (1..=mm).rev() {
                    let (kr, ki) = kk[s - 1];
                    let gp = gprev[s - 1];
                    f = (f.0 - (kr * gp.0 - ki * gp.1), f.1 - (kr * gp.1 + ki * gp.0));
                    fs[s - 1] = f;
                }
                gnew[0] = fs[0];
                for s in 1..=mm {
                    let (kr, ki) = kk[s - 1];
                    let gp = gprev[s - 1];
                    let f0 = fs[s - 1];
                    gnew[s] = (kr * f0.0 + ki * f0.1 + gp.0, kr * f0.1 - ki * f0.0 + gp.1);
                }
                let mut y = (0.0, 0.0);
                for (s, g) in gnew.iter().enumerate().take(vm.numel()) {
                    let (vr, vi) = (vm.data[s], vm.im_at(s));
                    y = (y.0 + vr * g.0 - vi * g.1, y.1 + vr * g.1 + vi * g.0);
                }
                fo = y;
                go = gnew[mm];
            } else {
                let mut f = (xx[t], 0.0);
                gnew[0] = f;
                for s in 1..=mm {
                    let (kr, ki) = kk[s - 1];
                    let gp = gprev[s - 1];
                    let fn_ = (f.0 + kr * gp.0 - ki * gp.1, f.1 + kr * gp.1 + ki * gp.0);
                    gnew[s] = (kr * f.0 + ki * f.1 + gp.0, kr * f.1 - ki * f.0 + gp.1);
                    f = fn_;
                }
                fo = f;
                go = gnew[mm];
            }
            f_out[t] = fo;
            g_out[t] = go;
            gprev = gnew;
        }
        fre.extend(f_out.iter().map(|c| c.0));
        fim.extend(f_out.iter().map(|c| c.1));
        gre.extend(g_out.iter().map(|c| c.0));
        gim.extend(g_out.iter().map(|c| c.1));
    }
    let dims = if xrow && nf == 1 { vec![1, n] } else { vec![n, nf] };
    let mk = |re: Vec<f64>, im: Vec<f64>| {
        if im.iter().all(|&v| v == 0.0) { Value::Num(Matrix::from_dims(dims.clone(), re)) } else { Value::Num(Matrix::complex(dims.clone(), re, im)) }
    };
    let mut outv = vec![mk(fre, fim)];
    if nargout > 1 {
        outv.push(mk(gre, gim));
    }
    out(outv)
}
