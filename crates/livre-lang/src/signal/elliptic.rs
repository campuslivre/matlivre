//! Elliptic and Bessel analog lowpass prototypes: `ellipap`, `besselap`.

use crate::builtins::Ret;
use crate::cplx::C;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::analog::Zpk;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["ellipap", "besselap"];

const PI: f64 = std::f64::consts::PI;

fn agm(mut a: f64, mut b: f64) -> f64 {
    for _ in 0..60 {
        let (x, y) = ((a + b) / 2.0, (a * b).sqrt());
        if (x - y).abs() <= 1e-16 * x {
            return x;
        }
        a = x;
        b = y;
    }
    a
}

/// Complete elliptic integral of the first kind for modulus `k`.
pub(crate) fn ellip_k(k: f64) -> f64 {
    PI / (2.0 * agm(1.0, (1.0 - k * k).sqrt()))
}

pub(crate) fn ellip_kp(k: f64) -> f64 {
    PI / (2.0 * agm(1.0, k))
}

fn nome(k: f64) -> f64 {
    (-PI * ellip_kp(k) / ellip_k(k)).exp()
}

fn csin(z: C) -> C {
    C::new(z.re.sin() * z.im.cosh(), z.re.cos() * z.im.sinh())
}

fn ccos(z: C) -> C {
    C::new(z.re.cos() * z.im.cosh(), -z.re.sin() * z.im.sinh())
}

#[derive(Clone, Copy)]
enum Theta {
    T1,
    T2,
    T3,
    T4,
}

fn theta(q: f64, w: C, kind: Theta) -> C {
    let mut s = match kind {
        Theta::T3 | Theta::T4 => C::new(1.0, 0.0),
        _ => C::new(0.0, 0.0),
    };
    for n in 0..4000 {
        let nf = n as f64;
        let (term, mag) = match kind {
            Theta::T1 | Theta::T2 => {
                let qq = q.powf((nf + 0.5) * (nf + 0.5));
                let arg = C::new(w.re * (2.0 * nf + 1.0), w.im * (2.0 * nf + 1.0));
                let t = if matches!(kind, Theta::T1) { csin(arg) } else { ccos(arg) };
                let sign = if matches!(kind, Theta::T1) && n % 2 == 1 { -2.0 } else { 2.0 };
                (C::new(t.re * sign * qq, t.im * sign * qq), qq)
            }
            _ => {
                if n == 0 {
                    continue;
                }
                let qq = q.powf(nf * nf);
                let arg = C::new(w.re * 2.0 * nf, w.im * 2.0 * nf);
                let t = ccos(arg);
                let sign = if matches!(kind, Theta::T4) && n % 2 == 1 { -2.0 } else { 2.0 };
                (C::new(t.re * sign * qq, t.im * sign * qq), qq)
            }
        };
        s = s.add(term);
        if mag < 1e-20 {
            break;
        }
    }
    s
}

/// Jacobi sn(z, k) through the theta functions of nome `q`.
fn sn(z: C, k: f64, kk: f64, q: f64) -> C {
    let _ = k;
    let w = C::new(PI * z.re / (2.0 * kk), PI * z.im / (2.0 * kk));
    let lead = theta(q, C::new(0.0, 0.0), Theta::T3).div(theta(q, C::new(0.0, 0.0), Theta::T2));
    lead.mul(theta(q, w, Theta::T1)).div(theta(q, w, Theta::T4))
}

/// Carlson's R_F.
fn rf(mut x: f64, mut y: f64, mut z: f64) -> f64 {
    for _ in 0..80 {
        let l = (x * y).sqrt() + (y * z).sqrt() + (z * x).sqrt();
        x = (x + l) / 4.0;
        y = (y + l) / 4.0;
        z = (z + l) / 4.0;
    }
    1.0 / ((x + y + z) / 3.0).sqrt()
}

/// Incomplete integral F(phi, k') for the complementary modulus of `k1`.
fn ellip_f_comp(phi: f64, k1: f64) -> f64 {
    let (s, c) = (phi.sin(), phi.cos());
    s * rf(c * c, c * c + k1 * k1 * s * s, 1.0)
}

pub(crate) fn ellipap(n: usize, rp: f64, rs: f64) -> Zpk {
    let ep = (10f64.powf(rp / 10.0) - 1.0).sqrt();
    let es = (10f64.powf(rs / 10.0) - 1.0).sqrt();
    let k1 = ep / es;
    // Degree equation: the nome of the selectivity is that of the discrimination to the 1/n.
    let q = nome(k1).powf(1.0 / n as f64);
    let k = {
        let t2 = theta(q, C::new(0.0, 0.0), Theta::T2).re;
        let t3 = theta(q, C::new(0.0, 0.0), Theta::T3).re;
        (t2 / t3) * (t2 / t3)
    };
    let kk = ellip_k(k);
    let kkp = ellip_kp(k);
    let qk = nome(k);
    let cd = |x: C| -> C { sn(C::new(x.re + kk, x.im), k, kk, qk) };
    let shift = ellip_f_comp((1.0 / ep).atan(), k1) / ellip_kp(k1) * kkp;
    let pairs = n / 2;
    let mut z = vec![];
    let mut p = vec![];
    // MATLAB lists the pairs from the highest u down.
    for i in (1..=pairs).rev() {
        let u = (2 * i - 1) as f64 / n as f64;
        let zeta = cd(C::new(u * kk, 0.0)).re;
        let zi = 1.0 / (k * zeta);
        z.push(C::new(0.0, -zi));
        z.push(C::new(0.0, zi));
        let pole = C::new(0.0, 1.0).mul(cd(C::new(u * kk, -shift)));
        p.push(C::new(pole.re, -pole.im.abs()));
        p.push(C::new(pole.re, pole.im.abs()));
    }
    if n % 2 == 1 {
        p.push(C::new(C::new(0.0, 1.0).mul(cd(C::new(kk, -shift))).re, 0.0));
    }
    let mut gain = C::new(1.0, 0.0);
    for r in &p {
        gain = gain.mul(C::new(-r.re, -r.im));
    }
    for r in &z {
        gain = gain.div(C::new(-r.re, -r.im));
    }
    let mut kgain = gain.re;
    if n % 2 == 0 {
        kgain /= (1.0 + ep * ep).sqrt();
    }
    Zpk { z, p, k: kgain }
}

/// Poles of the Bessel lowpass with the asymptotic magnitude normalisation, in the order of the
/// roots of the reverse Bessel polynomial.
pub(crate) fn besselap(n: usize) -> R<Zpk> {
    // theta_n(s) = sum_k (2n-k)! / (2^(n-k) k! (n-k)!) s^k, highest power first.
    let fact = |m: usize| -> f64 { (1..=m).map(|x| x as f64).product() };
    let coef: Vec<C> = (0..=n)
        .rev()
        .map(|k| C::new(fact(2 * n - k) / (2f64.powi((n - k) as i32) * fact(k) * fact(n - k)), 0.0))
        .collect();
    let roots = roots_matlab(&coef)?;
    let a0 = coef[n].re;
    let scale = a0.powf(1.0 / n as f64);
    let p: Vec<C> = roots.iter().map(|r| C::new(r.re / scale, r.im / scale)).collect();
    let mut p: Vec<C> = p.into_iter().map(|c| if c.im.abs() < 1e-14 { C::new(c.re, 0.0) } else { c }).collect();
    // The order MATLAB returns: a real pole first, then the pairs by increasing imaginary part,
    // each as (-, +); orders 2 and 4 come out differently.
    let mut pairs: Vec<C> = p.iter().copied().filter(|c| c.im > 0.0).collect();
    let real: Vec<C> = p.iter().copied().filter(|c| c.im == 0.0).collect();
    pairs.sort_by(|a, b| a.im.partial_cmp(&b.im).unwrap());
    if n == 4 {
        pairs.reverse();
    }
    p = real;
    for c in pairs {
        if n == 2 {
            p.push(c);
            p.push(c.conj());
        } else {
            p.push(c.conj());
            p.push(c);
        }
    }
    Ok(Zpk { z: vec![], p, k: 1.0 })
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    let want = if name == "ellipap" { 3 } else { 1 };
    if a.len() < want {
        return rt("Not enough input arguments.");
    }
    if a.len() > want {
        return rt("Too many input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let m = crate::builtins::m_arg(a, 0, name)?;
    if m.kind == Kind::Char {
        return rt("Expected N to be one of these types:  double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64");
    }
    if m.numel() != 1 {
        return rt("Expected N to be a scalar.");
    }
    if m.data[0] <= 0.0 {
        return rt("Expected N to be positive.");
    }
    if m.data[0].fract() != 0.0 {
        return rt("Expected N to be integer-valued.");
    }
    let n = m.data[0] as usize;
    if name == "besselap" {
        let f = besselap(n)?;
        return out(vec![Value::Num(Matrix::zeros(0, 0)), col_value(&f.p), scalar_value(f.k)]);
    }
    let scalar = |i: usize, label: &str| -> R<f64> {
        let m = crate::builtins::m_arg(a, i, name)?;
        if m.numel() != 1 {
            return rt(format!("Expected {label} to be a scalar."));
        }
        if m.data[0] < 0.0 {
            return rt(format!("Expected {label} to be nonnegative."));
        }
        Ok(m.data[0])
    };
    let rp = scalar(1, "Rp")?;
    let rs = scalar(2, "Rs")?;
    if rp == 0.0 {
        return rt("Passband ripple cannot be zero. Use CHEBY2 or BUTTER if no passband ripple is desired.");
    }
    if rs <= rp {
        return rt("Stopband attenuation must be greater than passband ripple.");
    }
    let f = ellipap(n, rp, rs);
    out(vec![col_value(&f.z), col_value(&f.p), scalar_value(f.k)])
}
