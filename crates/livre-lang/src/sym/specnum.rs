//! Double-precision kernels for the Symbolic Math Toolbox special functions: exponential,
//! trigonometric and logarithmic integrals, Fresnel integrals, polylogarithms, incomplete
//! gamma, Hurwitz zeta, hypergeometric and Meijer G functions, Carlson-form elliptic
//! integrals and the Jacobi elliptic functions.

use std::f64::consts::{FRAC_PI_2, PI};

use crate::cplx::C;

pub const EULER: f64 = 0.577_215_664_901_532_9;
pub const CATALAN: f64 = 0.915_965_594_177_219;
const EPS: f64 = f64::EPSILON;
const NAN: f64 = f64::NAN;
const INF: f64 = f64::INFINITY;

fn re(x: f64) -> C {
    C::new(x, 0.0)
}

pub fn gamma(x: f64) -> f64 {
    crate::builtins::gamma_fn(x)
}

/// 1/Gamma(x), zero at the poles.
pub fn rgamma(x: f64) -> f64 {
    if x <= 0.0 && x.fract() == 0.0 {
        0.0
    } else {
        1.0 / gamma(x)
    }
}

// ---- exponential, trigonometric and logarithmic integrals ---------------------------------------

/// E1(x) for x > 0.
pub fn e1(x: f64) -> f64 {
    if x == 0.0 {
        return INF;
    }
    if x <= 1.0 {
        let mut sum = 0.0;
        let mut t = 1.0;
        for k in 1..200 {
            t *= -x / k as f64;
            let term = t / k as f64;
            sum += term;
            if term.abs() < EPS * sum.abs().max(1e-300) {
                break;
            }
        }
        -EULER - x.ln() - sum
    } else {
        // Continued fraction (modified Lentz).
        let tiny = 1e-300;
        let mut b = x + 1.0;
        let mut c = 1.0 / tiny;
        let mut d = 1.0 / b;
        let mut h = d;
        for i in 1..10_000 {
            let an = -((i * i) as f64);
            b += 2.0;
            d = 1.0 / (an * d + b);
            c = b + an / c;
            let del = c * d;
            h *= del;
            if (del - 1.0).abs() < EPS {
                break;
            }
        }
        h * (-x).exp()
    }
}

/// Ei(x) for real x.
pub fn ei(x: f64) -> f64 {
    if x.is_nan() {
        return NAN;
    }
    if x == 0.0 {
        return -INF;
    }
    if x < 0.0 {
        return -e1(-x);
    }
    if x < 40.0 {
        let mut sum = 0.0;
        let mut t = 1.0;
        for k in 1..1000 {
            t *= x / k as f64;
            let term = t / k as f64;
            sum += term;
            if term < EPS * sum {
                break;
            }
        }
        EULER + x.ln() + sum
    } else {
        let mut sum = 1.0;
        let mut t = 1.0;
        for k in 1..100 {
            let next = t * k as f64 / x;
            if next > t || next < EPS * sum {
                break;
            }
            t = next;
            sum += t;
        }
        x.exp() / x * sum
    }
}

/// (Ci(x), Si(x)) for x > 0.
fn cisi(x: f64) -> (f64, f64) {
    if x > 2.0 {
        let fpmin = 1e-300;
        let mut b = C::new(1.0, x);
        let mut c = C::new(1.0 / fpmin, 0.0);
        let mut d = C::new(1.0, 0.0).div(b);
        let mut h = d;
        for i in 2..100_000 {
            let a = -(((i - 1) * (i - 1)) as f64);
            b = b.add(re(2.0));
            d = C::new(1.0, 0.0).div(re(a).mul(d).add(b));
            c = b.add(re(a).div(c));
            let del = c.mul(d);
            h = h.mul(del);
            if (del.re - 1.0).abs() + del.im.abs() < EPS {
                break;
            }
        }
        h = C::new(x.cos(), -x.sin()).mul(h);
        (-h.re, FRAC_PI_2 + h.im)
    } else {
        let (mut sum, mut sums, mut sumc) = (0.0f64, 0.0f64, 0.0f64);
        let mut sign = 1.0;
        let mut fact = 1.0;
        let mut odd = true;
        for k in 1..200 {
            fact *= x / k as f64;
            let term = fact / k as f64;
            sum += sign * term;
            let err = term / sum.abs();
            if odd {
                sign = -sign;
                sums = sum;
                sum = sumc;
            } else {
                sumc = sum;
                sum = sums;
            }
            if err < EPS {
                break;
            }
            odd = !odd;
        }
        (sumc + x.ln() + EULER, sums)
    }
}

pub fn sinint(x: f64) -> f64 {
    if x.is_nan() {
        return NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    if x.is_infinite() {
        return FRAC_PI_2.copysign(x);
    }
    let s = cisi(x.abs()).1;
    if x < 0.0 { -s } else { s }
}

pub fn cosint(x: f64) -> C {
    if x.is_nan() {
        return re(NAN);
    }
    if x == 0.0 {
        return re(-INF);
    }
    if x == INF {
        return re(0.0);
    }
    let c = if x.is_infinite() { 0.0 } else { cisi(x.abs()).0 };
    if x < 0.0 { C::new(c, PI) } else { re(c) }
}

/// (Chi(x), Shi(x)) for x > 0.
fn chishi(x: f64) -> (f64, f64) {
    if x <= 40.0 {
        let mut t = 1.0;
        let (mut c, mut s) = (0.0, 0.0);
        for k in 1..400 {
            t *= x / k as f64;
            let term = t / k as f64;
            if k % 2 == 1 {
                s += term;
            } else {
                c += term;
            }
            if term < EPS * 0.25 * (s + c.abs()) {
                break;
            }
        }
        (EULER + x.ln() + c, s)
    } else {
        let (a, b) = (ei(x), e1(x));
        ((a - b) / 2.0, (a + b) / 2.0)
    }
}

pub fn sinhint(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        return x;
    }
    if x.is_infinite() {
        return x;
    }
    let s = chishi(x.abs()).1;
    if x < 0.0 { -s } else { s }
}

pub fn coshint(x: f64) -> C {
    if x.is_nan() {
        return re(NAN);
    }
    if x == 0.0 {
        return re(-INF);
    }
    if x.is_infinite() {
        return if x > 0.0 { re(INF) } else { C::new(INF, PI) };
    }
    let c = chishi(x.abs()).0;
    if x < 0.0 { C::new(c, PI) } else { re(c) }
}

pub fn logint(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    if x == 1.0 {
        return -INF;
    }
    ei(x.ln())
}

/// (C(x), S(x)), the Fresnel integrals with the pi/2 normalization.
pub fn fresnel(x: f64) -> (f64, f64) {
    let ax = x.abs();
    if x.is_nan() {
        return (NAN, NAN);
    }
    if ax.is_infinite() {
        return (0.5f64.copysign(x), 0.5f64.copysign(x));
    }
    let (c, s) = if ax < 1e-150 {
        (ax, 0.0)
    } else if ax <= 1.5 {
        let (mut sum, mut sums) = (0.0f64, 0.0f64);
        let mut sumc = ax;
        let mut sign = 1.0;
        let fact = FRAC_PI_2 * ax * ax;
        let mut odd = true;
        let mut term = ax;
        let mut n = 3.0;
        for k in 1..200 {
            term *= fact / k as f64;
            sum += sign * term / n;
            let test = sum.abs() * EPS;
            if odd {
                sign = -sign;
                sums = sum;
                sum = sumc;
            } else {
                sumc = sum;
                sum = sums;
            }
            if term < test {
                break;
            }
            odd = !odd;
            n += 2.0;
        }
        (sumc, sums)
    } else {
        let pix2 = PI * ax * ax;
        let fpmin = 1e-300;
        let mut b = C::new(1.0, -pix2);
        let mut cc = C::new(1.0 / fpmin, 0.0);
        let mut d = C::new(1.0, 0.0).div(b);
        let mut h = d;
        let mut n = -1.0;
        for _ in 2..100_000 {
            n += 2.0;
            let a = -n * (n + 1.0);
            b = b.add(re(4.0));
            d = C::new(1.0, 0.0).div(re(a).mul(d).add(b));
            cc = b.add(re(a).div(cc));
            let del = cc.mul(d);
            h = h.mul(del);
            if (del.re - 1.0).abs() + del.im.abs() < EPS {
                break;
            }
        }
        h = h.mul(C::new(ax, -ax));
        let cs = C::new(0.5, 0.5).mul(C::new(1.0, 0.0).sub(C::new((0.5 * pix2).cos(), (0.5 * pix2).sin()).mul(h)));
        (cs.re, cs.im)
    };
    if x < 0.0 { (-c, -s) } else { (c, s) }
}

pub fn erfi(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        return x;
    }
    if x.abs() > 26.7 {
        return INF.copysign(x);
    }
    let x2 = x * x;
    let mut t = x;
    let mut sum = x;
    for n in 1..5000 {
        t *= x2 / n as f64;
        let term = t / (2 * n + 1) as f64;
        sum += term;
        if term.abs() < EPS * 0.5 * sum.abs() {
            break;
        }
    }
    2.0 / PI.sqrt() * sum
}

pub fn dawson(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        return x;
    }
    if x.is_infinite() {
        return 0.0;
    }
    if x.abs() < 7.0 {
        return PI.sqrt() / 2.0 * (-x * x).exp() * erfi(x);
    }
    let y = 1.0 / (2.0 * x * x);
    let mut t = 1.0;
    let mut sum = 1.0;
    for k in 1..60 {
        let next = t * (2 * k - 1) as f64 * y;
        if next > t || next < EPS * 0.25 {
            break;
        }
        t = next;
        sum += t;
    }
    sum / (2.0 * x)
}

// ---- polylogarithms -----------------------------------------------------------------------------

/// Bernoulli numbers B_0..B_n as doubles (B_1 = -1/2).
fn bernoulli(n: usize) -> Vec<f64> {
    let mut b = vec![0.0; n + 1];
    b[0] = 1.0;
    for m in 1..=n {
        let mut s = 0.0;
        let mut binom = 1.0; // C(m+1, k)
        for (k, bk) in b.iter().enumerate().take(m) {
            s += binom * bk;
            binom = binom * (m + 1 - k) as f64 / (k + 1) as f64;
        }
        b[m] = -s / (m + 1) as f64;
    }
    b
}

fn factorial(n: u32) -> f64 {
    (1..=n).fold(1.0, |a, k| a * k as f64)
}

/// Li_n(x) for integer n >= 2 and real x in [-1, 1].
fn li_real(n: i32, x: f64) -> f64 {
    if x == 1.0 {
        return super::extra::zeta_f64(n as f64);
    }
    if x.abs() <= 0.5 {
        let mut sum = 0.0;
        let mut p = 1.0;
        for k in 1..2000 {
            p *= x;
            let term = p / (k as f64).powi(n);
            sum += term;
            if term.abs() < EPS * 0.25 * sum.abs() {
                break;
            }
        }
        return sum;
    }
    if x < 0.0 {
        // Li_n(x) = 2^(1-n) Li_n(x^2) - Li_n(-x)
        return 2f64.powi(1 - n) * li_real(n, x * x) - li_real(n, -x);
    }
    // 0.5 < x < 1: expansion in mu = ln x.
    let mu = x.ln();
    let mut sum = 0.0;
    let mut p = 1.0; // mu^k / k!
    let harmonic: f64 = (1..n).map(|k| 1.0 / k as f64).sum();
    let zeta_int = |s: i32| -> f64 {
        if s >= 2 {
            super::extra::zeta_f64(s as f64)
        } else if s == 0 {
            -0.5
        } else if s % 2 == 0 {
            0.0
        } else {
            // zeta(1 - 2k) = -B_2k / (2k), B_2k = (-1)^(k+1) 2 (2k)! zeta(2k) / (2 pi)^(2k)
            let two_k = 1 - s;
            let k = two_k / 2;
            let b = 2.0 * factorial(two_k as u32) * super::extra::zeta_f64(two_k as f64) / (2.0 * PI).powi(two_k);
            let b = if k % 2 == 1 { b } else { -b };
            -b / two_k as f64
        }
    };
    for k in 0..200 {
        let kk = k as i32;
        let term = if kk == n - 1 { p * (harmonic - (-mu).ln()) } else { p * zeta_int(n - kk) };
        sum += term;
        if kk > n + 1 && p.abs() < EPS * 1e-3 * sum.abs() {
            break;
        }
        p *= mu / (k + 1) as f64;
    }
    sum
}

fn eulerian_li(k: u32, x: f64) -> f64 {
    // Li_{-k}(x) = x * sum_m A(k, m) x^m / (1 - x)^(k + 1)
    let mut a = vec![1.0f64];
    for n in 2..=k {
        let mut b = vec![0.0; n as usize];
        for m in 0..n as usize {
            let left = if m < a.len() { (m + 1) as f64 * a[m] } else { 0.0 };
            let right = if m >= 1 && m - 1 < a.len() { (n as usize - m) as f64 * a[m - 1] } else { 0.0 };
            b[m] = left + right;
        }
        a = b;
    }
    let p = a.iter().rev().fold(0.0, |acc, c| acc * x + c);
    x * p / (1.0 - x).powi(k as i32 + 1)
}

pub fn polylog(n: f64, x: f64) -> C {
    if n.is_nan() || x.is_nan() || n.fract() != 0.0 {
        return re(NAN);
    }
    let n = n as i32;
    if n <= 0 {
        if x == 1.0 {
            return re(if n == 0 { INF } else { NAN });
        }
        return re(if n == 0 { x / (1.0 - x) } else { eulerian_li((-n) as u32, x) });
    }
    if n == 1 {
        let y = 1.0 - x;
        return if y > 0.0 { re(-y.ln()) } else if y == 0.0 { re(INF) } else { C::new(-(-y).ln(), -PI) };
    }
    if x.abs() <= 1.0 {
        return re(li_real(n, x));
    }
    if x.is_infinite() {
        return re(-INF);
    }
    // Inversion: Li_n(z) = -(-1)^n Li_n(1/z) - (2 pi i)^n / n! B_n(1/2 + ln(-z) / (2 pi i)).
    let lnmz = if x < 0.0 { re((-x).ln()) } else { C::new(x.ln(), PI) };
    let two_pi_i = C::new(0.0, 2.0 * PI);
    let t = C::new(0.5, 0.0).add(lnmz.div(two_pi_i));
    let bn = bernoulli(n as usize);
    let mut bpoly = C::new(0.0, 0.0);
    let mut binom = 1.0;
    for (k, bk) in bn.iter().enumerate() {
        let pw = cpowi(t, n as usize - k);
        bpoly = bpoly.add(pw.mul(re(binom * bk)));
        binom = binom * (n as usize - k) as f64 / (k + 1) as f64;
    }
    let pre = cpowi(two_pi_i, n as usize).mul(re(1.0 / factorial(n as u32)));
    let sign = if n % 2 == 0 { -1.0 } else { 1.0 };
    let inv = li_real(n, 1.0 / x);
    let mut r = re(sign * inv).sub(pre.mul(bpoly));
    if x < 0.0 {
        r.im = 0.0;
    }
    r
}

fn cpowi(z: C, k: usize) -> C {
    (0..k).fold(C::new(1.0, 0.0), |a, _| a.mul(z))
}

pub fn dilog(x: f64) -> C {
    polylog(2.0, 1.0 - x)
}

// ---- gamma-type functions -----------------------------------------------------------------------

/// Upper incomplete gamma Γ(a, x) for real a and x >= 0.
pub fn igamma(a: f64, x: f64) -> f64 {
    if a.is_nan() || x.is_nan() || x < 0.0 {
        return NAN;
    }
    if x == 0.0 {
        return if a > 0.0 { gamma(a) } else { INF };
    }
    if x.is_infinite() {
        return 0.0;
    }
    if a == 0.0 {
        return e1(x);
    }
    if a > 0.0 {
        let (_, q) = crate::builtins_specfun::gammainc_pq(x, a);
        let g = gamma(a);
        if g.is_finite() {
            return q * g;
        }
        return (q.ln() + crate::builtins_specfun::gammaln(a)).exp();
    }
    // Downward recurrence Γ(a, x) = (Γ(a + 1, x) - x^a e^-x) / a.
    let k = (-a).ceil();
    let top = a + k;
    let mut g = igamma(top, x);
    let mut s = top;
    while s > a + 0.5 {
        s -= 1.0;
        g = (g - x.powf(s) * (-x).exp()) / s;
    }
    g
}

pub fn pochhammer(x: f64, n: f64) -> f64 {
    if x.is_nan() || n.is_nan() {
        return NAN;
    }
    if n == 0.0 {
        return 1.0;
    }
    if n.fract() == 0.0 && n > 0.0 && n < 1e6 {
        return (0..n as i64).fold(1.0, |acc, k| acc * (x + k as f64));
    }
    let r = gamma(x + n) / gamma(x);
    if r.is_finite() {
        r
    } else {
        let (lg1, s1) = lgamma_sign(x + n);
        let (lg2, s2) = lgamma_sign(x);
        s1 * s2 * (lg1 - lg2).exp()
    }
}

fn lgamma_sign(x: f64) -> (f64, f64) {
    if x > 0.0 {
        (crate::builtins_specfun::gammaln(x), 1.0)
    } else {
        let s = (PI * x).sin();
        let (l, _) = lgamma_sign(1.0 - x);
        ((PI / s.abs()).ln() - l, if s < 0.0 { -1.0 } else { 1.0 })
    }
}

/// Hurwitz zeta ζ(s, a) for real s and a > 0 (Euler-Maclaurin).
pub fn hurwitz_zeta(s: f64, a: f64) -> f64 {
    if s.is_nan() || a.is_nan() || s == 1.0 {
        return NAN;
    }
    if a <= 0.0 && (a.fract() == 0.0 || s.fract() != 0.0) {
        return if a.fract() == 0.0 && s > 0.0 { INF } else { NAN };
    }
    let n = 12usize;
    let mut sum = 0.0;
    for k in 0..n {
        sum += (k as f64 + a).powf(-s);
    }
    let na = n as f64 + a;
    sum += na.powf(1.0 - s) / (s - 1.0) + 0.5 * na.powf(-s);
    let b2 = [1.0 / 6.0, -1.0 / 30.0, 1.0 / 42.0, -1.0 / 30.0, 5.0 / 66.0, -691.0 / 2730.0, 7.0 / 6.0, -3617.0 / 510.0, 43867.0 / 798.0, -174611.0 / 330.0];
    let mut rising = s; // s (s+1) ... (s + 2j - 2)
    let mut fact = 2.0; // (2j)!
    let mut pw = na.powf(-s - 1.0);
    for (j, b) in b2.iter().enumerate() {
        let j = j + 1;
        let term = b / fact * rising * pw;
        sum += term;
        if term.abs() < EPS * 0.25 * sum.abs() {
            break;
        }
        let jj = j as f64;
        rising *= (s + 2.0 * jj - 1.0) * (s + 2.0 * jj);
        fact *= (2.0 * jj + 1.0) * (2.0 * jj + 2.0);
        pw /= na * na;
    }
    sum
}

pub fn lambertw0(x: f64) -> f64 {
    super::extra::lambertw_f64(x)
}

/// Wright omega for real x: the solution of w + log(w) = x.
pub fn wright_omega(x: f64) -> f64 {
    if x.is_nan() {
        return NAN;
    }
    if x == INF {
        return INF;
    }
    if x < -40.0 {
        let e = x.exp();
        return e * (1.0 - e);
    }
    let mut w = if x > 1.0 {
        x - x.ln()
    } else if x < -2.0 {
        x.exp()
    } else {
        lambertw0(x.exp())
    };
    for _ in 0..100 {
        let f = w + w.ln() - x;
        let dw = f / (1.0 + 1.0 / w);
        let nw = w - dw;
        let done = (nw - w).abs() <= 2.0 * EPS * nw.abs();
        w = if nw > 0.0 { nw } else { w / 2.0 };
        if done {
            break;
        }
    }
    w
}

// ---- hypergeometric functions -------------------------------------------------------------------

/// Power series of pFq; None when it does not converge in a reasonable number of terms.
fn pfq_series(a: &[f64], b: &[f64], z: f64, max_terms: usize) -> Option<f64> {
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut small = 0;
    for k in 0..max_terms {
        let kf = k as f64;
        let mut ratio = z / (kf + 1.0);
        for &ai in a {
            ratio *= ai + kf;
        }
        for &bi in b {
            if bi + kf == 0.0 {
                return Some(if term == 0.0 { sum } else { INF });
            }
            ratio /= bi + kf;
        }
        term *= ratio;
        if term == 0.0 {
            return Some(sum);
        }
        sum += term;
        if !sum.is_finite() {
            return Some(sum);
        }
        if term.abs() < EPS * 0.25 * sum.abs() {
            small += 1;
            if small > 2 {
                return Some(sum);
            }
        } else {
            small = 0;
        }
    }
    None
}

fn terminating(a: &[f64]) -> bool {
    a.iter().any(|&x| x <= 0.0 && x.fract() == 0.0)
}

pub fn hypergeom(a: &[f64], b: &[f64], z: f64) -> f64 {
    if z.is_nan() || a.iter().chain(b).any(|v| v.is_nan()) {
        return NAN;
    }
    if z == 0.0 {
        return 1.0;
    }
    let (p, q) = (a.len(), b.len());
    if terminating(a) {
        return pfq_series(a, b, z, 1_000_000).unwrap_or(NAN);
    }
    if p <= q {
        if p == 1 && q == 1 && z < 0.0 {
            // Kummer: M(a, b, z) = e^z M(b - a, b, -z).
            return z.exp() * hypergeom(&[b[0] - a[0]], b, -z);
        }
        return pfq_series(a, b, z, 1_000_000).unwrap_or(NAN);
    }
    if p > q + 1 {
        return NAN;
    }
    if p == 2 && q == 1 {
        return hyp2f1(a[0], a[1], b[0], z);
    }
    if z.abs() < 1.0 || (z.abs() == 1.0 && b.iter().sum::<f64>() - a.iter().sum::<f64>() > 0.0) {
        return pfq_series(a, b, z, 2_000_000).unwrap_or(NAN);
    }
    NAN
}

fn hyp2f1(a: f64, b: f64, c: f64, z: f64) -> f64 {
    if z == 1.0 {
        let s = c - a - b;
        return if s > 0.0 { gamma(c) * gamma(s) * rgamma(c - a) * rgamma(c - b) } else { INF };
    }
    if z > 1.0 {
        return NAN;
    }
    if z < -0.5 {
        // Pfaff: (1 - z)^-a 2F1(a, c - b; c; z / (z - 1)).
        let w = z / (z - 1.0);
        return (1.0 - z).powf(-a) * hyp2f1(a, c - b, c, w);
    }
    if z <= 0.9 {
        return pfq_series(&[a, b], &[c], z, 100_000).unwrap_or(NAN);
    }
    let s = c - a - b;
    if s.fract() != 0.0 {
        let w = 1.0 - z;
        let t1 = gamma(c) * gamma(s) * rgamma(c - a) * rgamma(c - b) * pfq_series(&[a, b], &[1.0 - s], w, 100_000).unwrap_or(NAN);
        let t2 = w.powf(s) * gamma(c) * gamma(-s) * rgamma(a) * rgamma(b) * pfq_series(&[c - a, c - b], &[1.0 + s], w, 100_000).unwrap_or(NAN);
        return t1 + t2;
    }
    pfq_series(&[a, b], &[c], z, 5_000_000).unwrap_or(NAN)
}

/// ∫_0^∞ f(t) dt by the exp-sinh rule, for integrands decaying at infinity.
fn exp_sinh(f: &dyn Fn(f64) -> f64) -> f64 {
    let node = |tau: f64| -> f64 {
        let t = (FRAC_PI_2 * tau.sinh()).exp();
        let w = FRAC_PI_2 * tau.cosh() * t;
        if t == 0.0 || !t.is_finite() || !w.is_finite() {
            return 0.0;
        }
        let v = f(t) * w;
        if v.is_finite() { v } else { 0.0 }
    };
    let mut h = 0.5;
    let sum_at = |h: f64, offset: bool| -> f64 {
        let mut s = 0.0;
        let start = if offset { 0.5 } else { 0.0 };
        for dir in [1.0, -1.0] {
            let mut k = start;
            if !offset && dir < 0.0 {
                k = 1.0;
            }
            loop {
                let tau = dir * k * h;
                if tau.abs() > 7.0 {
                    break;
                }
                s += node(tau);
                k += 1.0;
            }
        }
        s
    };
    let mut total = sum_at(h, false);
    let mut est = total * h;
    for _ in 0..8 {
        total += sum_at(h, true);
        h /= 2.0;
        let next = total * h;
        if (next - est).abs() <= 1e-15 * next.abs() {
            return next;
        }
        est = next;
    }
    est
}

/// Tricomi's confluent hypergeometric U(a, b, z) for real arguments, z >= 0.
pub fn kummer_u(a: f64, b: f64, z: f64) -> f64 {
    if a.is_nan() || b.is_nan() || z.is_nan() || z < 0.0 {
        return NAN;
    }
    if a == 0.0 {
        return 1.0;
    }
    if z == 0.0 {
        return if b < 1.0 { gamma(1.0 - b) * rgamma(a - b + 1.0) } else { INF };
    }
    if a > 0.0 {
        let lg = crate::builtins_specfun::gammaln(a);
        let f = move |t: f64| (-z * t + (a - 1.0) * t.ln() + (b - a - 1.0) * t.ln_1p() - lg).exp();
        return exp_sinh(&f);
    }
    // Downward recurrence in a: U(a-1) = (2a - b + z) U(a) - a (a - b + 1) U(a+1).
    let a0 = a + (-a).ceil();
    let mut u1 = kummer_u(a0 + 1.0, b, z);
    let mut u0 = kummer_u(a0, b, z);
    let mut s = a0;
    while s > a + 0.5 {
        let um = (2.0 * s - b + z) * u0 - s * (s - b + 1.0) * u1;
        u1 = u0;
        u0 = um;
        s -= 1.0;
    }
    u0
}

pub fn whittaker_m(k: f64, m: f64, z: f64) -> f64 {
    (-z / 2.0).exp() * z.powf(m + 0.5) * hypergeom(&[m - k + 0.5], &[1.0 + 2.0 * m], z)
}

pub fn whittaker_w(k: f64, m: f64, z: f64) -> f64 {
    (-z / 2.0).exp() * z.powf(m + 0.5) * kummer_u(m - k + 0.5, 1.0 + 2.0 * m, z)
}

/// Meijer G by Slater's theorem (sum over the poles of the first m gamma factors); valid
/// when p < q, or p == q and |z| < 1, and no two of b_1..b_m differ by an integer.
pub fn meijer_g(an: &[f64], ap: &[f64], bm: &[f64], bq: &[f64], z: f64) -> f64 {
    let a: Vec<f64> = an.iter().chain(ap).copied().collect();
    let b: Vec<f64> = bm.iter().chain(bq).copied().collect();
    let (n, m, p, q) = (an.len(), bm.len(), a.len(), b.len());
    if !(p < q || (p == q && z.abs() < 1.0)) || z.is_nan() {
        return NAN;
    }
    for i in 0..m {
        for j in 0..i {
            if (b[i] - b[j]).fract() == 0.0 {
                return NAN;
            }
        }
    }
    let sign = if (p + m + n) % 2 == 0 { 1.0 } else { -1.0 };
    let mut total = 0.0;
    for h in 0..m {
        let bh = b[h];
        let mut num = 1.0;
        for (j, bj) in b.iter().enumerate().take(m) {
            if j != h {
                num *= gamma(bj - bh);
            }
        }
        for aj in a.iter().take(n) {
            num *= gamma(1.0 + bh - aj);
        }
        let mut den = 1.0;
        for bj in b.iter().skip(m) {
            den *= rgamma(1.0 + bh - bj);
        }
        for aj in a.iter().skip(n) {
            den *= rgamma(aj - bh);
        }
        if den == 0.0 || num == 0.0 {
            continue;
        }
        let pa: Vec<f64> = a.iter().map(|aj| 1.0 + bh - aj).collect();
        let pb: Vec<f64> = b.iter().enumerate().filter(|&(j, _)| j != h).map(|(_, bj)| 1.0 + bh - bj).collect();
        total += num * den * z.powf(bh) * hypergeom(&pa, &pb, sign * z);
    }
    total
}

// ---- elliptic integrals (Carlson forms) ---------------------------------------------------------

const ERRTOL: f64 = 1e-3;

pub fn rf(x: f64, y: f64, z: f64) -> f64 {
    let (mut x, mut y, mut z) = (x, y, z);
    loop {
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let lam = sx * (sy + sz) + sy * sz;
        x = 0.25 * (x + lam);
        y = 0.25 * (y + lam);
        z = 0.25 * (z + lam);
        let ave = (x + y + z) / 3.0;
        let (dx, dy, dz) = ((ave - x) / ave, (ave - y) / ave, (ave - z) / ave);
        if dx.abs().max(dy.abs()).max(dz.abs()) < ERRTOL {
            let e2 = dx * dy - dz * dz;
            let e3 = dx * dy * dz;
            return (1.0 + (e2 / 24.0 - 0.1 - 3.0 / 44.0 * e3) * e2 + e3 / 14.0) / ave.sqrt();
        }
    }
}

pub fn rd(x: f64, y: f64, z: f64) -> f64 {
    let (mut x, mut y, mut z) = (x, y, z);
    let (mut sum, mut fac) = (0.0, 1.0);
    let (c1, c2, c3, c4) = (3.0 / 14.0, 1.0 / 6.0, 9.0 / 22.0, 3.0 / 26.0);
    let (c5, c6) = (0.25 * c3, 1.5 * c4);
    loop {
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let lam = sx * (sy + sz) + sy * sz;
        sum += fac / (sz * (z + lam));
        fac *= 0.25;
        x = 0.25 * (x + lam);
        y = 0.25 * (y + lam);
        z = 0.25 * (z + lam);
        let ave = 0.2 * (x + y + 3.0 * z);
        let (dx, dy, dz) = ((ave - x) / ave, (ave - y) / ave, (ave - z) / ave);
        if dx.abs().max(dy.abs()).max(dz.abs()) < ERRTOL {
            let ea = dx * dy;
            let eb = dz * dz;
            let ec = ea - eb;
            let ed = ea - 6.0 * eb;
            let ee = ed + ec + ec;
            return 3.0 * sum + fac * (1.0 + ed * (-c1 + c5 * ed - c6 * dz * ee) + dz * (c2 * ee + dz * (-c3 * ec + dz * c4 * ea))) / (ave * ave.sqrt());
        }
    }
}

pub fn rc(x: f64, y: f64) -> f64 {
    let (mut xt, mut yt, w) = if y > 0.0 { (x, y, 1.0) } else { (x - y, -y, x.sqrt() / (x - y).sqrt()) };
    loop {
        let lam = 2.0 * xt.sqrt() * yt.sqrt() + yt;
        xt = 0.25 * (xt + lam);
        yt = 0.25 * (yt + lam);
        let ave = (xt + yt + yt) / 3.0;
        let s = (yt - ave) / ave;
        if s.abs() < ERRTOL {
            return w * (1.0 + s * s * (0.3 + s * (1.0 / 7.0 + s * (0.375 + s * 9.0 / 22.0)))) / ave.sqrt();
        }
    }
}

pub fn rj(x: f64, y: f64, z: f64, p: f64) -> f64 {
    let (mut x, mut y, mut z, mut p) = (x, y, z, p);
    let (mut sum, mut fac) = (0.0, 1.0);
    let (c1, c2, c3, c4) = (3.0 / 14.0, 1.0 / 3.0, 3.0 / 22.0, 3.0 / 26.0);
    let (c5, c6, c7, c8) = (0.75 * c3, 1.5 * c4, 0.5 * c2, c3 + c3);
    loop {
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let lam = sx * (sy + sz) + sy * sz;
        let alpha = (p * (sx + sy + sz) + sx * sy * sz).powi(2);
        let beta = p * (p + lam).powi(2);
        sum += fac * rc(alpha, beta);
        fac *= 0.25;
        x = 0.25 * (x + lam);
        y = 0.25 * (y + lam);
        z = 0.25 * (z + lam);
        p = 0.25 * (p + lam);
        let ave = 0.2 * (x + y + z + p + p);
        let (dx, dy, dz, dp) = ((ave - x) / ave, (ave - y) / ave, (ave - z) / ave, (ave - p) / ave);
        if dx.abs().max(dy.abs()).max(dz.abs()).max(dp.abs()) < ERRTOL {
            let ea = dx * (dy + dz) + dy * dz;
            let eb = dx * dy * dz;
            let ec = dp * dp;
            let ed = ea - 3.0 * ec;
            let ee = eb + 2.0 * dp * (ea - ec);
            return 3.0 * sum
                + fac * (1.0 + ed * (-c1 + c5 * ed - c6 * ee) + eb * (c7 + dp * (-c8 + dp * c4)) + dp * ea * (c2 - dp * c3) - c2 * dp * ec) / (ave * ave.sqrt());
        }
    }
}

pub fn ellip_k(m: f64) -> C {
    if m.is_nan() {
        return re(NAN);
    }
    if m == 1.0 {
        return re(INF);
    }
    if m == -INF {
        return re(0.0);
    }
    if m < 1.0 {
        return re(rf(0.0, 1.0 - m, 1.0));
    }
    let s = m.sqrt();
    C::new(rf(0.0, 1.0 - 1.0 / m, 1.0) / s, -rf(0.0, 1.0 / m, 1.0) / s)
}

pub fn ellip_e(m: f64) -> C {
    if m.is_nan() {
        return re(NAN);
    }
    if m == 1.0 {
        return re(1.0);
    }
    if m == -INF {
        return re(INF);
    }
    if m < 1.0 {
        let y = 1.0 - m;
        return re(rf(0.0, y, 1.0) - m / 3.0 * rd(0.0, y, 1.0));
    }
    let k1 = ellip_k(1.0 / m).re;
    let e1 = ellip_e(1.0 / m).re;
    let k2 = ellip_k(1.0 - 1.0 / m).re;
    let e2 = ellip_e(1.0 - 1.0 / m).re;
    let s = m.sqrt();
    C::new(s * (e1 - (1.0 - 1.0 / m) * k1), s * (e2 - k2 / m))
}

/// Complete Π(n | m) for n < 1, m < 1.
pub fn ellip_pi(n: f64, m: f64) -> f64 {
    if n.is_nan() || m.is_nan() || m > 1.0 || n > 1.0 {
        return NAN;
    }
    if n == 1.0 || m == 1.0 {
        return INF;
    }
    let y = 1.0 - m;
    rf(0.0, y, 1.0) + n / 3.0 * rj(0.0, y, 1.0, 1.0 - n)
}

/// Splits phi into phi0 in [-pi/2, pi/2] and k with phi = phi0 + k pi.
fn reduce_phi(phi: f64) -> (f64, f64) {
    let k = (phi / PI).round();
    (phi - k * PI, k)
}

pub fn ellip_f(phi: f64, m: f64) -> f64 {
    if phi.is_nan() || m.is_nan() {
        return NAN;
    }
    let (p0, k) = reduce_phi(phi);
    let (s, c) = (p0.sin(), p0.cos());
    let y = 1.0 - m * s * s;
    if y < 0.0 {
        return NAN;
    }
    let base = s * rf(c * c, y, 1.0);
    if k == 0.0 { base } else { base + 2.0 * k * ellip_k(m).re }
}

pub fn ellip_e_inc(phi: f64, m: f64) -> f64 {
    if phi.is_nan() || m.is_nan() {
        return NAN;
    }
    let (p0, k) = reduce_phi(phi);
    let (s, c) = (p0.sin(), p0.cos());
    let y = 1.0 - m * s * s;
    if y < 0.0 {
        return NAN;
    }
    let base = s * rf(c * c, y, 1.0) - m / 3.0 * s * s * s * rd(c * c, y, 1.0);
    if k == 0.0 { base } else { base + 2.0 * k * ellip_e(m).re }
}

pub fn ellip_pi_inc(n: f64, phi: f64, m: f64) -> f64 {
    if n.is_nan() || phi.is_nan() || m.is_nan() {
        return NAN;
    }
    let (p0, k) = reduce_phi(phi);
    let (s, c) = (p0.sin(), p0.cos());
    let y = 1.0 - m * s * s;
    let pp = 1.0 - n * s * s;
    if y < 0.0 || pp <= 0.0 {
        return NAN;
    }
    let base = s * rf(c * c, y, 1.0) + n / 3.0 * s * s * s * rj(c * c, y, 1.0, pp);
    if k == 0.0 { base } else { base + 2.0 * k * ellip_pi(n, m) }
}

pub fn ellip_nome(m: f64) -> f64 {
    if m.is_nan() || !(0.0..=1.0).contains(&m) {
        return NAN;
    }
    if m == 0.0 {
        return 0.0;
    }
    if m == 1.0 {
        return 1.0;
    }
    (-PI * ellip_k(1.0 - m).re / ellip_k(m).re).exp()
}

/// (sn, cn, dn, am) of u for parameter m.
pub fn jacobi(u: f64, m: f64) -> (f64, f64, f64, f64) {
    if u.is_nan() || m.is_nan() {
        return (NAN, NAN, NAN, NAN);
    }
    if m == 0.0 {
        return (u.sin(), u.cos(), 1.0, u);
    }
    if m == 1.0 {
        let s = 1.0 / u.cosh();
        return (u.tanh(), s, s, 2.0 * u.exp().atan() - FRAC_PI_2);
    }
    if m > 1.0 {
        let k = m.sqrt();
        let (sn, cn, dn, _) = jacobi(u * k, 1.0 / m);
        return (sn / k, dn, cn, NAN);
    }
    if m < 0.0 {
        let mu = -m / (1.0 - m);
        let s = (1.0 - m).sqrt();
        let (sn, cn, dn, _) = jacobi(u * s, mu);
        return (sn / (s * dn), cn / dn, 1.0 / dn, NAN);
    }
    // Arithmetic-geometric mean (descending Landen transformation).
    let mut a = vec![1.0f64];
    let mut c = vec![m.sqrt()];
    let mut b = (1.0 - m).sqrt();
    while c.last().unwrap().abs() > EPS && a.len() < 60 {
        let an = *a.last().unwrap();
        c.push((an - b) / 2.0);
        let bn = (an * b).sqrt();
        a.push((an + b) / 2.0);
        b = bn;
    }
    let nn = a.len() - 1;
    let mut phi = 2f64.powi(nn as i32) * a[nn] * u;
    let mut prev = phi;
    for i in (1..=nn).rev() {
        prev = phi;
        phi = (phi + (c[i] / a[i] * phi.sin()).asin()) / 2.0;
    }
    let (s, cc) = (phi.sin(), phi.cos());
    let dn = if nn == 0 { 1.0 } else { cc / (prev - phi).cos() };
    (s, cc, dn, phi)
}

pub fn jacobi_zeta(u: f64, m: f64) -> f64 {
    if m == 0.0 {
        return 0.0;
    }
    let (_, _, _, am) = jacobi(u, m);
    ellip_e_inc(am, m) - ellip_e(m).re / ellip_k(m).re * u
}

// ---- elementary pulses ------------------------------------------------------------------------

pub fn heaviside(x: f64) -> f64 {
    if x.is_nan() {
        NAN
    } else if x > 0.0 {
        1.0
    } else if x < 0.0 {
        0.0
    } else {
        0.5
    }
}

pub fn rect_pulse(a: f64, b: f64, x: f64) -> f64 {
    if x.is_nan() || a.is_nan() || b.is_nan() {
        return NAN;
    }
    if a == b {
        return 0.0;
    }
    if x > a && x < b {
        1.0
    } else if x == a || x == b {
        0.5
    } else {
        0.0
    }
}

pub fn tri_pulse(a: f64, b: f64, c: f64, x: f64) -> f64 {
    if x.is_nan() || a.is_nan() || b.is_nan() || c.is_nan() {
        return NAN;
    }
    if x > a && x < b {
        (x - a) / (b - a)
    } else if x == b && a < c {
        1.0
    } else if x > b && x < c {
        (c - x) / (c - b)
    } else {
        0.0
    }
}

pub fn sign_im(z: C) -> f64 {
    if z.re.is_nan() || z.im.is_nan() {
        NAN
    } else if z.im > 0.0 || (z.im == 0.0 && z.re < 0.0) {
        1.0
    } else if z.im < 0.0 || (z.im == 0.0 && z.re > 0.0) {
        -1.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() <= 2e-14 * b.abs().max(1.0), "{a} vs {b}");
    }

    #[test]
    fn values_from_matlab() {
        close(sinint(2.0), 1.605412976802695);
        close(cosint(2.0).re, 0.422980828774865);
        close(ei(2.0), 4.954234356001890);
        close(fresnel(1.5).0, 0.445261176039822);
        close(igamma(2.5, 1.5), 0.930519442786792);
        close(kummer_u(0.5, 1.5, 2.0), 0.707106781186548);
        close(hypergeom(&[1.0, 1.0], &[2.0], -0.5), 0.810930216216329);
        close(wright_omega(3.0), 2.207940031569323);
        close(dilog(0.3).re, 0.889377624286039);
        close(ellip_k(0.7).re, 2.075363135292469);
        close(ellip_e(0.7).re, 1.241670567945823);
        close(ellip_f(1.2, 0.7), 1.429448433022763);
        close(ellip_pi_inc(0.4, 1.2, 0.7), 1.735556917188619);
        close(ellip_pi(-0.5, 0.7), 1.645042868400853);
        close(jacobi(1.3, 0.7).0, 0.898731076260419);
        close(jacobi(1.3, 0.7).3, 1.116867096931372);
        close(jacobi_zeta(1.3, 0.7), 0.197276630196292);
        close(ellip_nome(0.7), 0.074689943537179);
    }
}
