//! `maxflat`: maximally flat (generalized Butterworth) digital filters.
//!
//! The squared magnitude is written in x = sin^2(w/2) as N(x)/D(x). Flatness at w = 0, the number of
//! zeros at z = -1 and the half-power frequency give one linear system per choice of the number
//! of zeros; the choice for which the result is a valid magnitude (non-negative numerator, positive
//! denominator, decreasing) is the design. The magnitude is then factored into minimum-phase
//! polynomials.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["maxflat"];

fn fmt_g6(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let e = v.abs().log10().floor() as i32;
    let dec = (5 - e).max(0) as usize;
    let s = format!("{:.*}", dec, v);
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
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
        if a[piv][c].abs() < 1e-14 {
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

fn perm(k: usize, j: usize) -> f64 {
    ((k - j + 1)..=k).map(|v| v as f64).product()
}

fn eval(p: &[f64], x: f64) -> f64 {
    p.iter().rev().fold(0.0, |a, c| a * x + c)
}

/// N and D (ascending powers of x, constant term 1) for `s` zeros at z = -1.
fn solve_iir(nb: usize, na: usize, xc: f64, s: usize) -> Option<(Vec<f64>, Vec<f64>)> {
    let m = nb + na - s;
    let n = nb + na;
    let mut a: Vec<Vec<f64>> = vec![];
    let mut b: Vec<f64> = vec![];
    for j in 0..s {
        let mut row = vec![0.0; n];
        for k in 1..=nb {
            if k >= j {
                row[k - 1] = perm(k, j);
            }
        }
        a.push(row);
        b.push(if j == 0 { -1.0 } else { 0.0 });
    }
    for k in 1..m {
        let mut row = vec![0.0; n];
        if k <= nb {
            row[k - 1] += 1.0;
        }
        if k <= na {
            row[nb + k - 1] -= 1.0;
        }
        a.push(row);
        b.push(0.0);
    }
    let mut row = vec![0.0; n];
    for k in 1..=nb {
        row[k - 1] += xc.powi(k as i32);
    }
    for k in 1..=na {
        row[nb + k - 1] -= xc.powi(k as i32) / 2.0;
    }
    a.push(row);
    b.push(-0.5);
    if a.len() != n {
        return None;
    }
    let sol = solve_linear(a, b)?;
    let mut nn = vec![1.0];
    nn.extend_from_slice(&sol[..nb]);
    let mut dd = vec![1.0];
    dd.extend_from_slice(&sol[nb..]);
    Some((nn, dd))
}

fn grid() -> impl Iterator<Item = f64> {
    (0..=4000).map(|i| i as f64 / 4000.0)
}

/// Some(true) = valid magnitude, Some(false) = invalid, None = numerically degenerate.
fn valid_iir(nn: &[f64], dd: &[f64]) -> Option<bool> {
    if nn.iter().chain(dd.iter()).any(|v| !v.is_finite() || v.abs() > 1e10) {
        return None;
    }
    let mut prev = f64::INFINITY;
    for x in grid() {
        let (nv, dv) = (eval(nn, x), eval(dd, x));
        if nv < -1e-9 || dv <= 1e-9 {
            return Some(false);
        }
        let g = nv / dv;
        if g > prev + 1e-9 {
            return Some(false);
        }
        prev = g;
    }
    Some(true)
}

fn roots_of(it: &mut Interpreter, asc: &[f64]) -> R<Vec<(f64, f64)>> {
    // strip negligible leading (highest power) coefficients
    let mx = asc.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let mut d = asc.len();
    while d > 1 && asc[d - 1].abs() <= 1e-9 * mx {
        d -= 1;
    }
    if d <= 1 {
        return Ok(vec![]);
    }
    let desc: Vec<f64> = asc[..d].iter().rev().cloned().collect();
    let r = it.call_function("roots", vec![real_col(desc)], 1)?.remove(0).to_matrix()?;
    let im = r.im.clone().unwrap_or_else(|| vec![0.0; r.numel()]);
    Ok((0..r.numel()).map(|i| (r.data[i], im[i])).collect())
}

fn cmul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

/// Roots z (inside the unit circle) of the z-domain factors for roots x of a polynomial in x.
fn z_from_x(x: (f64, f64)) -> (f64, f64) {
    let w = (2.0 - 4.0 * x.0, -4.0 * x.1);
    // z = (w - sqrt(w^2 - 4)) / 2, picking the root with |z| <= 1
    let disc = (cmul(w, w).0 - 4.0, cmul(w, w).1);
    let mag = disc.0.hypot(disc.1);
    let sr = ((mag + disc.0) / 2.0).max(0.0).sqrt();
    let si = ((mag - disc.0) / 2.0).max(0.0).sqrt() * if disc.1 < 0.0 { -1.0 } else { 1.0 };
    let z1 = ((w.0 - sr) / 2.0, (w.1 - si) / 2.0);
    let z2 = ((w.0 + sr) / 2.0, (w.1 + si) / 2.0);
    if z1.0.hypot(z1.1) <= z2.0.hypot(z2.1) { z1 } else { z2 }
}

/// Monic polynomial in z^-1 with the given roots, padded to `len` coefficients.
fn poly_from_z(zs: &[(f64, f64)], len: usize) -> Vec<f64> {
    let mut p: Vec<(f64, f64)> = vec![(1.0, 0.0)];
    for z in zs {
        let mut q = vec![(0.0, 0.0); p.len() + 1];
        for (i, c) in p.iter().enumerate() {
            q[i].0 += c.0;
            q[i].1 += c.1;
            let t = cmul(*c, *z);
            q[i + 1].0 -= t.0;
            q[i + 1].1 -= t.1;
        }
        p = q;
    }
    let mut out: Vec<f64> = p.iter().map(|c| c.0).collect();
    out.resize(len, 0.0);
    out
}

fn conv(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut o = vec![0.0; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            o[i + j] += x * y;
        }
    }
    o
}

fn binom_poly(s: usize) -> Vec<f64> {
    let mut p = vec![1.0];
    for _ in 0..s {
        p = conv(&p, &[1.0, 1.0]);
    }
    p
}

/// Divide `num` by (1 - x)^s (ascending coefficients).
fn div_one_minus_x(num: &[f64], s: usize) -> Vec<f64> {
    let mut p = num.to_vec();
    for _ in 0..s {
        // p(x) = (1 - x) q(x): q_0 = p_0, q_k = p_k + q_{k-1}
        let mut q = vec![0.0; p.len().saturating_sub(1)];
        let mut acc = 0.0;
        for k in 0..q.len() {
            acc += p[k];
            q[k] = acc;
        }
        p = q;
    }
    p
}

fn lo_range(nb: usize) -> f64 {
    let x = 1.0 - 2f64.powf(-1.0 / nb as f64);
    2.0 / std::f64::consts::PI * x.sqrt().asin()
}

fn xc_of(wn: f64) -> f64 {
    (std::f64::consts::PI * wn / 2.0).sin().powi(2)
}

pub fn call(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 4 {
        return rt("Too many input arguments.");
    }
    if nargout > 6 {
        return rt("Too many output arguments.");
    }
    let num = |v: &Value, pos: usize| -> R<Matrix> {
        match v {
            Value::Num(m) if m.kind != Kind::Char => Ok(m.clone()),
            other => rt(format!("Input {pos} in maxflat must be numeric, instead it was {}.", other.class_name())),
        }
    };
    let nbm = num(&a[0], 1)?;
    let sym = matches!(&a[1], Value::Num(m) if m.kind == Kind::Char);
    let nam = if sym { None } else { Some(num(&a[1], 2)?) };
    let wnm = num(&a[2], 3)?;
    if let Some(f) = a.get(3) {
        let ok = matches!(f, Value::Num(m) if m.kind == Kind::Char) && ["trace", "plots", "both"].contains(&f.text().unwrap_or_default().as_str());
        if !ok {
            return rt("Plot flag must be 'trace', 'plots', or 'both'.");
        }
    }
    if nbm.numel() != 1 {
        return rt("Size inputs must be scalar.");
    }
    let nbv = nbm.data[0];
    if nbv.fract() != 0.0 {
        return rt("Size inputs must be integers.");
    }
    if nbv < 0.0 {
        return rt("Index exceeds the number of array elements. Index must not exceed 0.");
    }
    let nb = nbv as usize;
    if sym {
        if nb % 2 == 1 {
            return rt("For symmetric FIR filters, only even orders are allowed.");
        }
        if nb >= 140 {
            return rt("For symmetric FIR filters, the order must be less than 140.");
        }
        if wnm.numel() != 1 {
            return rt("Index in position 1 exceeds array bounds. Index must not exceed 1.");
        }
        return sym_design(it, nb, wnm.data[0], nargout);
    }
    let nam = nam.unwrap();
    if nam.numel() != 1 {
        return rt("Index in position 1 is invalid. Array indices must be positive integers or logical values.");
    }
    if nam.data[0].fract() != 0.0 {
        return rt("Argument must be integer-valued.");
    }
    if nam.data[0] < 0.0 {
        return rt("Index exceeds the number of array elements. Index must not exceed 0.");
    }
    let na = nam.data[0] as usize;
    if wnm.numel() != 1 {
        return rt(format!("Index in position 1 exceeds array bounds. Index must not exceed {}.", wnm.numel()));
    }
    let wn = wnm.data[0];
    iir_design(it, nb, na, wn, nargout)
}

fn finish(_it: &mut Interpreter, b: Vec<f64>, a: Vec<f64>, b1: Vec<f64>, b2: Vec<f64>, nargout: usize) -> Ret {
    let mut outs = vec![real_row(b), real_row(a), real_row(b1), real_row(b2)];
    if nargout > 4 {
        return rt("maxflat: the second-order-section outputs are not implemented.");
    }
    outs.truncate(nargout.max(1));
    out(outs)
}

fn iir_design(it: &mut Interpreter, nb: usize, na: usize, wn: f64, nargout: usize) -> Ret {
    if nb == 0 && na == 0 {
        return finish(it, vec![1.0], vec![1.0], vec![1.0], vec![1.0], nargout);
    }
    if na == 0 {
        if nb == 1 {
            if !(wn > 0.0 && wn <= 0.5) {
                return rt("For NB=1 and NA=0, Wn must be between 0 and 0.5.");
            }
            return finish(it, vec![0.5, 0.5], vec![1.0], vec![1.0, 1.0], vec![0.5], nargout);
        }
        let lo = lo_range(nb);
        if !(wn >= lo && wn <= 1.0 - lo) {
            return rt(format!("For NB={nb} and NA=0, Wn must be between {} and {}.", fmt_g6(lo), fmt_g6(1.0 - lo)));
        }
    } else {
        if wn < 0.0 || wn > 1.0 {
            return rt(format!("For NB={nb} and NA={na}, Wn must be between 0 and 1."));
        }
        if wn == 0.0 {
            return rt("Input to ROOTS must not contain NaN or Inf.");
        }
        if wn == 1.0 {
            return rt("Index exceeds the number of array elements. Index must not exceed 0.");
        }
    }
    let xc = xc_of(wn);
    let s_min = if na == 0 { 1 } else { 0 };
    let s_max = nb.min(nb + na - 1);
    let mut cands: Vec<(usize, Vec<f64>, Vec<f64>)> = vec![];
    let mut degenerate = false;
    for s in s_min..=s_max {
        match solve_iir(nb, na, xc, s) {
            None => degenerate = true,
            Some((nn, dd)) => match valid_iir(&nn, &dd) {
                Some(true) => cands.push((s, nn, dd)),
                Some(false) => {}
                None => degenerate = true,
            },
        }
    }
    let chosen = if nb == na && nb > 0 {
        solve_iir(nb, na, xc, nb).map(|(nn, dd)| (nb, nn, dd))
    } else if na == 0 && cands.len() > 1 {
        return rt("Unable to design filter, try using a smaller filter order.");
    } else {
        cands.into_iter().max_by_key(|c| c.0)
    };
    let Some((s, nn, dd)) = chosen else {
        return if degenerate {
            rt("A pole-zero pair cancels at z=1. Use a lower order.")
        } else {
            rt("Unable to design filter, try using a smaller filter order.")
        };
    };
    // numerator: N(x) = (1-x)^s P(x)
    let p = div_one_minus_x(&nn, s);
    let xr = roots_of(it, &p)?;
    let mut zb: Vec<(f64, f64)> = xr.iter().map(|&x| z_from_x(x)).collect();
    let b2_len = nb - s + 1;
    let xd = roots_of(it, &dd)?;
    let zd: Vec<(f64, f64)> = xd.iter().map(|&x| z_from_x(x)).collect();
    let a_poly = poly_from_z(&zd, na + 1);
    let b2_monic = poly_from_z(&zb, b2_len);
    zb.clear();
    let b1 = binom_poly(s);
    let a_dc: f64 = a_poly.iter().sum();
    let b_dc: f64 = conv(&b1, &b2_monic).iter().sum();
    let k = a_dc / b_dc;
    let b2: Vec<f64> = b2_monic.iter().map(|v| v * k).collect();
    let b = conv(&b1, &b2);
    finish(it, b, a_poly, b1, b2, nargout)
}

fn sym_design(it: &mut Interpreter, nb: usize, wn: f64, nargout: usize) -> Ret {
    let n = nb / 2;
    if n == 0 {
        return finish(it, vec![1.0], vec![1.0], vec![1.0], vec![1.0], nargout);
    }
    let lo = if n == 1 { 0.0 } else { lo_range(nb) };
    let hi = if n == 1 {
        lo_range(2)
    } else {
        let x = (1.0 - 0.5f64.sqrt()).powf(1.0 / n as f64);
        2.0 / std::f64::consts::PI * x.sqrt().asin()
    };
    if !(wn >= lo && wn <= hi) || !wn.is_finite() {
        return rt(format!("For NB={nb} and NA=0, Wn must be between {} and {}.", fmt_g6(if n == 1 { 0.0 } else { lo }), fmt_g6(hi)));
    }
    let mut kfound = 1usize;
    let amp: Vec<f64> = if n == 1 {
        vec![1.0, -1.0]
    } else {
        let xc = xc_of(wn);
        let mut found: Vec<(usize, Vec<f64>)> = vec![];
        for k in 1..n {
            let m = n - k;
            let mut a: Vec<Vec<f64>> = vec![];
            let mut b: Vec<f64> = vec![];
            for j in 0..k {
                a.push((1..=n).map(|i| if i >= j { perm(i, j) } else { 0.0 }).collect());
                b.push(if j == 0 { -1.0 } else { 0.0 });
            }
            for i in 1..m {
                let mut row = vec![0.0; n];
                row[i - 1] = 1.0;
                a.push(row);
                b.push(0.0);
            }
            a.push((1..=n).map(|i| xc.powi(i as i32)).collect());
            b.push(1.0 / 2f64.sqrt() - 1.0);
            if a.len() != n {
                continue;
            }
            if let Some(sol) = solve_linear(a, b) {
                let mut p = vec![1.0];
                p.extend(sol);
                let mut prev = f64::INFINITY;
                let mut ok = p.iter().all(|v| v.is_finite());
                if ok {
                    for x in grid() {
                        let v = eval(&p, x);
                        if v < -1e-9 || v > prev + 1e-9 {
                            ok = false;
                            break;
                        }
                        prev = v;
                    }
                }
                if ok {
                    found.push((k, p));
                }
            }
        }
        match found.into_iter().max_by_key(|f| f.0) {
            Some((k, p)) => {
                kfound = k;
                p
            }
            None => return rt("Unable to design filter, try using a smaller filter order."),
        }
    };
    // A(x) = sum amp_k x^k with x = (1 - cos w)/2 = (2 - z - 1/z)/4, as a symmetric polynomial in z
    let base = [-0.25, 0.5, -0.25];
    let mut taps = vec![0.0; 2 * n + 1];
    let mut pk = vec![1.0];
    for (k, c) in amp.iter().enumerate() {
        if k > 0 {
            pk = conv(&pk, &base);
        }
        let off = n - k;
        for (i, v) in pk.iter().enumerate() {
            taps[off + i] += c * v;
        }
    }
    let b1 = binom_poly(2 * kfound);
    // b2 = taps / b1
    let mut b2 = vec![0.0; taps.len() - b1.len() + 1];
    let mut rem = taps.clone();
    for i in 0..b2.len() {
        b2[i] = rem[i] / b1[0];
        for (j, c) in b1.iter().enumerate() {
            rem[i + j] -= b2[i] * c;
        }
    }
    finish(it, taps, vec![1.0], b1, b2, nargout)
}
