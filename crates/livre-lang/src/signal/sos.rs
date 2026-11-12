//! Second-order-section factorisation: `zp2sos`, `tf2sos`.

use crate::builtins::{m_arg, Ret};
use crate::cplx::C;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["zp2sos", "tf2sos"];

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let (mut z, mut p, k, flags) = if name == "tf2sos" {
        if a.len() < 2 {
            return rt("Not enough input arguments.");
        }
        let b = cvec_arg(a, 0, name)?;
        let d = cvec_arg(a, 1, name)?;
        let (z, p, k) = zpk_of(&b, &d)?;
        (z, p, k, &a[2..])
    } else {
        if a.len() < 2 {
            return rt("Not enough input arguments.");
        }
        let z = cvec_arg(a, 0, name)?;
        let p = cvec_arg(a, 1, name)?;
        let k = if a.len() > 2 {
            let k = m_arg(a, 2, name)?;
            if k.numel() != 1 {
                return rt("Expected input number 3, k, to be a scalar.");
            }
            C::new(k.data[0], k.im_at(0))
        } else {
            C::new(1.0, 0.0)
        };
        (z, p, k, if a.len() > 3 { &a[3..] } else { &a[0..0] })
    };
    if flags.len() > 2 {
        return rt("Too many input arguments.");
    }
    let mut up = true;
    let mut scale = "none".to_string();
    if let Some(f) = flags.first() {
        match f.text().map(|s| s.to_lowercase()) {
            Ok(s) if s == "up" => up = true,
            Ok(s) if s == "down" => up = false,
            _ => return rt("DIR_FLAG must be either 'UP' or 'DOWN'."),
        }
    }
    if let Some(f) = flags.get(1) {
        let bad = || rt::<()>("SCALE must be one of 'NONE', inf or 2.").unwrap_err();
        scale = match f {
            Value::Num(m) if m.kind != Kind::Char && m.numel() == 1 => match m.data[0] {
                x if x == f64::INFINITY => "inf".to_string(),
                x if x == 2.0 => "two".to_string(),
                _ => return Err(bad()),
            },
            v => match v.text().map(|s| s.to_lowercase()) {
                Ok(s) if s == "none" => "none".to_string(),
                Ok(s) if s == "inf" => "inf".to_string(),
                Ok(s) if s == "two" => "two".to_string(),
                _ => return Err(bad()),
            },
        };
    }
    // Poles at the origin make up for a numerator longer than the denominator.
    if name == "tf2sos" {
        while p.len() < z.len() {
            p.push(C::new(0.0, 0.0));
        }
    } else if z.len() > p.len() {
        return rt("The number of zeros cannot be greater than the number of poles.");
    }
    let _ = &mut z;
    let (mut sos, g) = zp2sos(&z, &p, k, up, &scale);
    if nargout < 2 {
        if let Some(first) = sos.first_mut() {
            for v in first.iter_mut().take(3) {
                *v *= g;
            }
        }
    }
    let g = if nargout < 2 { 1.0 } else { g };
    let rows = sos.len();
    let mut data = vec![0.0; rows * 6];
    for (r, s) in sos.iter().enumerate() {
        for (c, v) in s.iter().enumerate() {
            data[c * rows + r] = *v;
        }
    }
    let m = Value::Num(Matrix::new(rows, 6, data));
    if nargout >= 2 {
        out(vec![m, scalar_value(g)])
    } else {
        out(vec![m])
    }
}

fn zpk_of(num: &[C], den: &[C]) -> R<(Vec<C>, Vec<C>, C)> {
    let strip = |v: &[C]| -> Vec<C> { v.iter().copied().skip_while(|c| c.abs() == 0.0).collect() };
    let b = strip(num);
    let d = strip(den);
    if d.is_empty() {
        return rt("Denominator cannot be zero or empty.");
    }
    if b.is_empty() {
        return Ok((vec![], roots_matlab(&d)?, C::new(0.0, 0.0)));
    }
    let k = b[0].div(d[0]);
    // Roots at the origin come from trailing zeros; keep them in the zero list when the
    // numerator is padded to the denominator's length.
    let mut z = roots_matlab(&b)?;
    let p = roots_matlab(&d)?;
    // Numerator shorter than denominator: the missing leading coefficients are zeros at the
    // origin (powers of z^-1).
    while z.len() < p.len() {
        z.push(C::new(0.0, 0.0));
    }
    Ok((z, p, k))
}

/// A pole or zero group for one section: one real value, two reals, or a conjugate pair.
#[derive(Clone, Debug)]
enum Group {
    Real(Vec<f64>),
    Pair(C),
}

fn split(v: &[C]) -> (Vec<C>, Vec<f64>) {
    let tol = |c: &C| 1e-8 * (1.0 + c.abs());
    let mut pairs = vec![];
    let mut reals = vec![];
    let mut used = vec![false; v.len()];
    for i in 0..v.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        if v[i].im == 0.0 || v[i].im.abs() <= tol(&v[i]) * 0.0 {
            reals.push(v[i].re);
            continue;
        }
        let want = v[i].conj();
        if let Some(j) = (0..v.len()).find(|&j| !used[j] && v[j].sub(want).abs() <= tol(&v[i])) {
            used[j] = true;
            pairs.push(if v[i].im > 0.0 { v[i] } else { v[j] });
        } else {
            reals.push(v[i].re);
        }
    }
    (pairs, reals)
}

fn dist_unit(c: C) -> f64 {
    (1.0 - c.abs()).abs()
}

/// Sections `[b0 b1 b2 1 a1 a2]` and the gain left over. Zeros missing for the poles lie at
/// infinity, which shows as leading zeros in the numerator.
fn zp2sos(z: &[C], p: &[C], k: C, up: bool, scale: &str) -> (Vec<[f64; 6]>, f64) {
    let (ppairs, mut preals) = split(p);
    let (mut zpairs, mut zreals) = split(z);
    if p.is_empty() {
        return (vec![[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]], k.re);
    }

    // Pole groups, closest to the unit circle first; a lone real pole goes last.
    let mut groups: Vec<Group> = ppairs.iter().map(|&c| Group::Pair(c)).collect();
    preals.sort_by(|x, y| (1.0 - x.abs()).abs().partial_cmp(&(1.0 - y.abs()).abs()).unwrap());
    let mut lone = None;
    if preals.len() % 2 == 1 {
        // The real pole farthest from the unit circle stays single.
        lone = preals.pop();
    }
    while preals.len() >= 2 {
        let x = preals.remove(0);
        let y = preals.remove(0);
        groups.push(Group::Real(vec![x, y]));
    }
    let key = |g: &Group| match g {
        Group::Pair(c) => dist_unit(*c),
        Group::Real(v) => v.iter().map(|x| (1.0 - x.abs()).abs()).fold(f64::INFINITY, f64::min),
    };
    groups.sort_by(|x, y| key(x).partial_cmp(&key(y)).unwrap());
    if let Some(x) = lone {
        groups.push(Group::Real(vec![x]));
    }

    let mut sections: Vec<[f64; 6]> = vec![];
    for g in &groups {
        let (a1, a2, centre, order): (f64, f64, C, usize) = match g {
            Group::Pair(c) => (-2.0 * c.re, c.abs() * c.abs(), *c, 2),
            Group::Real(v) if v.len() == 2 => (-(v[0] + v[1]), v[0] * v[1], C::new((v[0] + v[1]) / 2.0, 0.0), 2),
            Group::Real(v) => (-v[0], 0.0, C::new(v[0], 0.0), 1),
        };
        // Zeros for this section: the finite ones nearest to its poles.
        let nearest_real = |zr: &Vec<f64>, from: C| zr.iter().enumerate().map(|(i, &x)| (i, C::new(x, 0.0).sub(from).abs())).min_by(|x, y| x.1.partial_cmp(&y.1).unwrap());
        let nearest_pair = |zp: &Vec<C>| zp.iter().enumerate().map(|(i, c)| (i, c.sub(centre).abs().min(c.conj().sub(centre).abs()))).min_by(|x, y| x.1.partial_cmp(&y.1).unwrap());
        let dp = if order == 2 { nearest_pair(&zpairs) } else { None };
        let dr = nearest_real(&zreals, centre);
        let (b0, b1, b2);
        match (dp, dr) {
            (Some((i, d1)), r) if r.map_or(true, |(_, d2)| d1 <= d2) => {
                let c = zpairs.remove(i);
                (b0, b1, b2) = (1.0, -2.0 * c.re, c.abs() * c.abs());
            }
            (Some((i, _)), Some(_)) if zreals.len() == 1 => {
                // A lone real zero cannot fill a pair section while a complex pair is left.
                let c = zpairs.remove(i);
                (b0, b1, b2) = (1.0, -2.0 * c.re, c.abs() * c.abs());
            }
            (_, Some((i, _))) => {
                let x = zreals.remove(i);
                if order == 2 {
                    match nearest_real(&zreals, C::new(x, 0.0)) {
                        Some((j, _)) => {
                            let y = zreals.remove(j);
                            (b0, b1, b2) = (1.0, -(x + y), x * y);
                        }
                        None => (b0, b1, b2) = (0.0, 1.0, -x),
                    }
                } else {
                    (b0, b1, b2) = (1.0, -x, 0.0);
                }
            }
            _ => (b0, b1, b2) = if order == 2 { (0.0, 0.0, 1.0) } else { (0.0, 1.0, 0.0) },
        }
        sections.push([b0, b1, b2, 1.0, a1, a2]);
    }
    if up {
        sections.reverse();
    }
    let mut g = k.re;
    if scale != "none" {
        g = scale_sections(&mut sections, k.re, scale == "inf");
    }
    (sections, g)
}

fn poly2(c: &[f64], w: f64) -> C {
    let e = C::new(w.cos(), -w.sin());
    let e2 = e.mul(e);
    C::new(c[0], 0.0).add(e.mul(C::new(c[1], 0.0))).add(e2.mul(C::new(c[2], 0.0)))
}

fn norm_of(f: &dyn Fn(f64) -> f64, inf: bool) -> f64 {
    if inf {
        // MATLAB takes the peak on 1024 points over [0, pi).
        (0..1024).map(|j| f(std::f64::consts::PI * j as f64 / 1024.0)).fold(0.0, f64::max)
    } else {
        let m = 1usize << 16;
        let sum: f64 = (0..m).map(|j| f(2.0 * std::f64::consts::PI * j as f64 / m as f64).powi(2)).sum();
        (sum / m as f64).sqrt()
    }
}

/// Scale the numerators so that the node feeding each section's successor has unit norm
/// (L-infinity for `inf`, L2 for `two`); the last section keeps the rest of the gain. Returns
/// the input gain, the norm of the first section's state, which `[sos,g]` reports as `g`.
fn scale_sections(sections: &mut [[f64; 6]], gain: f64, inf: bool) -> f64 {
    let n = sections.len();
    let units: Vec<[f64; 6]> = sections.to_vec();
    let mut sc = vec![1.0; n];
    for i in 0..n - 1 {
        let f = |w: f64| -> f64 {
            let mut h = C::new(1.0, 0.0);
            for j in 0..i {
                h = h.mul(poly2(&units[j][0..3], w)).div(poly2(&units[j][3..6], w)).mul(C::new(sc[j], 0.0));
            }
            h = h.mul(poly2(&units[i][0..3], w));
            for j in i..=i + 1 {
                h = h.div(poly2(&units[j][3..6], w));
            }
            h.abs()
        };
        sc[i] = 1.0 / norm_of(&f, inf);
    }
    let used: f64 = sc[..n - 1].iter().product();
    sc[n - 1] = gain / used;
    let s0 = 1.0 / norm_of(&|w: f64| 1.0 / poly2(&units[0][3..6], w).abs(), inf);
    for (r, (s, c)) in sections.iter_mut().zip(&sc).enumerate() {
        let c = if r == 0 { *c / s0 } else { *c };
        for v in s.iter_mut().take(3) {
            *v *= c;
        }
    }
    s0
}
