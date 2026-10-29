//! MATLAB's special functions (`toolbox/matlab/specfun`): Bessel and Airy functions, the error,
//! gamma, beta and exponential-integral families, elliptic functions and integrals, Legendre
//! functions, rational approximation, `perms` and the coordinate and colour-space transforms.
//!
//! Bessel and Airy functions come from the `complex-bessel` crate, a port of Amos' TOMS 644
//! routines. The error functions, `gammaln`, `psi` and the real exponential integral are ports
//! of W. J. Cody's public-domain SPECFUN rational approximations. The incomplete gamma and beta
//! functions use series and continued fractions with Temme's form of the prefactor, and their
//! inverses refine a first guess with Halley steps.
// The ported coefficient tables keep their published digits, and Cody's products keep his
// evaluation order (`r = a * b * r` rounds differently from `r *= a * b`).
#![allow(clippy::excessive_precision, clippy::approx_constant, clippy::unreadable_literal, clippy::assign_op_pattern)]

use std::f64::consts::PI;

use complex_bessel as cb;
use num_complex::Complex64;

use crate::builtins::{m_arg, Ret};
use crate::cplx::C;
use crate::error::{rt, LangError};
use crate::nd::{self, Dims};
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "airy", "besselj", "bessely", "besselh", "besseli", "besselk", "beta", "betainc", "betaincinv", "betaln", "ellipj", "ellipke", "erf",
    "erfc", "erfcx", "erfinv", "erfcinv", "expint", "gammainc", "gammaincinv", "gammaln", "psi", "legendre", "rat", "rats", "perms",
    "cart2sph", "cart2pol", "pol2cart", "sph2cart", "hsv2rgb", "rgb2hsv",
];

const EPS: f64 = f64::EPSILON;
const INF: f64 = f64::INFINITY;
const NAN: f64 = f64::NAN;
const SQRT_PI: f64 = 1.772453850905516027298;

// ---------------------------------------------------------------------------------------------
// Argument handling

/// A double or single argument; every other class is undefined for `name`.
fn float_arg(a: &[Value], i: usize, name: &str) -> R<Matrix> {
    let m = m_arg(a, i, name)?;
    if !m.kind.is_float() {
        return rt(format!("Undefined function '{name}' for input arguments of type '{}'.", m.kind.name()));
    }
    Ok(m)
}

/// Single when any input is single, double otherwise.
fn out_kind(ms: &[&Matrix]) -> Kind {
    if ms.iter().any(|m| m.kind == Kind::Single) { Kind::Single } else { Kind::Double }
}

fn real_out(dims: Dims, data: Vec<f64>, kind: Kind) -> Matrix {
    Matrix::from_dims(dims, data).cast_to(kind)
}

fn complex_out(dims: Dims, z: Vec<C>, kind: Kind) -> Matrix {
    let (re, im) = z.into_iter().map(|c| (c.re, c.im)).unzip();
    Matrix::complex(dims, re, im).cast_to(kind)
}

/// Common size of arrays that must agree exactly unless scalar.
fn same_size(ms: &[&Matrix], msg: &str) -> R<Dims> {
    let mut dims: Option<&Dims> = None;
    for m in ms {
        if m.is_scalar() {
            continue;
        }
        match dims {
            None => dims = Some(&m.dims),
            Some(d) if *d == m.dims => {}
            Some(_) => return rt(msg),
        }
    }
    Ok(dims.cloned().unwrap_or_else(|| vec![1, 1]))
}

/// Element `i` of an array that may be a scalar standing for the whole size.
fn at(m: &Matrix, i: usize) -> f64 {
    if m.is_scalar() { m.data[0] } else { m.data[i] }
}

fn cat(m: &Matrix, i: usize) -> C {
    if m.is_scalar() { C::new(m.data[0], m.im_at(0)) } else { C::new(m.data[i], m.im_at(i)) }
}

/// Implicit expansion of any number of arrays: the result size and, per element, the index
/// into each input.
fn expand(ms: &[&Matrix]) -> R<(Dims, Vec<Vec<usize>>)> {
    let mut dims = ms[0].dims.clone();
    for m in &ms[1..] {
        dims = nd::broadcast_dims(&dims, &m.dims)?;
    }
    let n = nd::numel(&dims);
    let idx = ms
        .iter()
        .map(|m| {
            (0..n)
                .map(|lin| {
                    let mut rem = lin;
                    let mut pos = 0;
                    let mut stride = 1;
                    for (k, &d) in dims.iter().enumerate() {
                        let sub = rem % d.max(1);
                        rem /= d.max(1);
                        let md = m.dims.get(k).copied().unwrap_or(1);
                        if md != 1 {
                            pos += sub * stride;
                        }
                        stride *= md;
                    }
                    pos
                })
                .collect()
        })
        .collect();
    Ok((dims, idx))
}

fn text_opt(v: Option<&Value>) -> Option<String> {
    v.filter(|v| v.is_text()).and_then(|v| v.text().ok())
}

fn require_real(m: &Matrix, msg: &str) -> R<()> {
    if m.is_complex() { rt(msg) } else { Ok(()) }
}

// ---------------------------------------------------------------------------------------------
// Error functions (Cody, CALERF)

fn calerf(arg: f64, jint: u8) -> f64 {
    const A: [f64; 5] = [3.16112374387056560e00, 1.13864154151050156e02, 3.77485237685302021e02, 3.20937758913846947e03, 1.85777706184603153e-1];
    const B: [f64; 4] = [2.36012909523441209e01, 2.44024637934444173e02, 1.28261652607737228e03, 2.84423683343917062e03];
    const CC: [f64; 9] = [
        5.64188496988670089e-1, 8.88314979438837594e0, 6.61191906371416295e01, 2.98635138197400131e02, 8.81952221241769090e02,
        1.71204761263407058e03, 2.05107837782607147e03, 1.23033935479799725e03, 2.15311535474403846e-8,
    ];
    const D: [f64; 8] = [
        1.57449261107098347e01, 1.17693950891312499e02, 5.37181101862009858e02, 1.62138957456669019e03, 3.29079923573345963e03,
        4.36261909014324716e03, 3.43936767414372164e03, 1.23033935480374942e03,
    ];
    const P: [f64; 6] = [3.05326634961232344e-1, 3.60344899949804439e-1, 1.25781726111229246e-1, 1.60837851487422766e-2, 6.58749161529837803e-4, 1.63153871373020978e-2];
    const Q: [f64; 5] = [2.56852019228982242e00, 1.87295284992346047e00, 5.27905102951428412e-1, 6.05183413124413191e-2, 2.33520497626869185e-3];
    const SQRPI: f64 = 5.6418958354775628695e-1;
    const THRESH: f64 = 0.46875;
    const XNEG: f64 = -26.628;
    const XSMALL: f64 = 1.11e-16;
    const XBIG: f64 = 26.543;
    const XHUGE: f64 = 6.71e7;
    const XMAX: f64 = 2.53e307;
    let x = arg;
    if x.is_infinite() {
        let neg = x < 0.0;
        return match jint {
            0 => x.signum(),
            1 => if neg { 2.0 } else { 0.0 },
            _ => if neg { INF } else { 0.0 },
        };
    }
    let y = x.abs();
    let mut result;
    if y <= THRESH {
        let ysq = if y > XSMALL { y * y } else { 0.0 };
        let mut xnum = A[4] * ysq;
        let mut xden = ysq;
        for i in 0..3 {
            xnum = (xnum + A[i]) * ysq;
            xden = (xden + B[i]) * ysq;
        }
        result = x * (xnum + A[3]) / (xden + B[3]);
        if jint != 0 {
            result = 1.0 - result;
        }
        if jint == 2 {
            result *= ysq.exp();
        }
        return result;
    } else if y <= 4.0 {
        let mut xnum = CC[8] * y;
        let mut xden = y;
        for i in 0..7 {
            xnum = (xnum + CC[i]) * y;
            xden = (xden + D[i]) * y;
        }
        result = (xnum + CC[7]) / (xden + D[7]);
        if jint != 2 {
            let ysq = (y * 16.0).trunc() / 16.0;
            let del = (y - ysq) * (y + ysq);
            result = (-ysq * ysq).exp() * (-del).exp() * result;
        }
    } else {
        result = 0.0;
        // MATLAB keeps erfc's gradual underflow past Cody's XBIG.
        let skip = y >= XBIG && (jint == 0 || (jint == 2 && y >= XMAX));
        if !skip {
            if jint == 2 && y >= XHUGE {
                result = SQRPI / y;
            } else {
                let ysq = 1.0 / (y * y);
                let mut xnum = P[5] * ysq;
                let mut xden = ysq;
                for i in 0..4 {
                    xnum = (xnum + P[i]) * ysq;
                    xden = (xden + Q[i]) * ysq;
                }
                result = ysq * (xnum + P[4]) / (xden + Q[4]);
                result = (SQRPI - result) / y;
                if jint != 2 {
                    let ysq = (y * 16.0).trunc() / 16.0;
                    let del = (y - ysq) * (y + ysq);
                    result = (-ysq * ysq).exp() * (-del).exp() * result;
                }
            }
        }
    }
    match jint {
        0 => {
            result = (0.5 - result) + 0.5;
            if x < 0.0 {
                result = -result;
            }
        }
        1 => {
            if x < 0.0 {
                result = 2.0 - result;
            }
        }
        _ => {
            if x < 0.0 {
                if x < XNEG {
                    result = INF;
                } else {
                    let ysq = (x * 16.0).trunc() / 16.0;
                    let del = (x - ysq) * (x + ysq);
                    let y = (ysq * ysq).exp() * del.exp();
                    result = (y + y) - result;
                }
            }
        }
    }
    result
}

pub(crate) fn erf(x: f64) -> f64 {
    calerf(x, 0)
}

pub(crate) fn erfc(x: f64) -> f64 {
    calerf(x, 1)
}

fn erfcx(x: f64) -> f64 {
    calerf(x, 2)
}

/// First guess for the inverse error function (Giles' single-precision fit), from
/// w = -log((1-y)(1+y)).
fn erfinv_guess(y: f64, w: f64) -> f64 {
    if w < 5.0 {
        let w = w - 2.5;
        let mut p = 2.81022636e-08;
        for c in [3.43273939e-07, -3.5233877e-06, -4.39150654e-06, 0.00021858087, -0.00125372503, -0.00417768164, 0.246640727, 1.50140941] {
            p = c + p * w;
        }
        p * y
    } else if w < 36.0 {
        let w = w.sqrt() - 3.0;
        let mut p = -0.000200214257;
        for c in [0.000100950558, 0.00134934322, -0.00367342844, 0.00573950773, -0.0076224613, 0.00943887047, 1.00167406, 2.83297682] {
            p = c + p * w;
        }
        p * y
    } else {
        // Far tail: erfc(x) ~ exp(-x^2)/(x sqrt(pi)).
        let t = (w - (0.5 * w).ln()).sqrt();
        t.copysign(y)
    }
}

/// x with erfc(x) = q for 0 < q <= 1, refined with Halley steps on the scaled form
/// erfcx(x) - q*exp(x^2), which keeps full relative accuracy in the far tail.
fn erfc_solve(q: f64, mut x: f64) -> f64 {
    for _ in 0..8 {
        let r = if x < 1.0 { erfc(x) * (x * x).exp() - q * (x * x).exp() } else { erfcx(x) - (x * x + q.ln()).exp() };
        let dx = -0.5 * SQRT_PI * r;
        let step = dx / (1.0 + x * dx);
        x -= step;
        if step.abs() <= 1e-17 * x.abs() {
            break;
        }
    }
    x
}

fn erfinv(y: f64) -> f64 {
    if y.is_nan() || y.abs() > 1.0 {
        return NAN;
    }
    if y.abs() == 1.0 {
        return INF.copysign(y);
    }
    if y == 0.0 {
        return y;
    }
    let a = y.abs();
    let w = -((1.0 - a) * (1.0 + a)).ln();
    let mut x = erfinv_guess(a, w);
    if a <= 0.5 {
        for _ in 0..4 {
            let dx = (erf(x) - a) * 0.5 * SQRT_PI * (x * x).exp();
            let step = dx / (1.0 + x * dx);
            x -= step;
            if step.abs() <= 1e-17 * x {
                break;
            }
        }
    } else {
        x = erfc_solve(1.0 - a, x);
    }
    x.copysign(y)
}

fn erfcinv(q: f64) -> f64 {
    if q.is_nan() || !(0.0..=2.0).contains(&q) {
        return NAN;
    }
    if q == 0.0 {
        return INF;
    }
    if q == 2.0 {
        return -INF;
    }
    if q > 1.0 {
        return -erfcinv(2.0 - q);
    }
    if q > 0.5 {
        return erfinv(1.0 - q);
    }
    let w = -(q * (2.0 - q)).ln();
    erfc_solve(q, erfinv_guess(1.0, w))
}

// ---------------------------------------------------------------------------------------------
// Gamma family (Cody's ALGAMA and PSI)

pub(crate) fn gammaln(x: f64) -> f64 {
    const D1: f64 = -5.772156649015328605195174e-1;
    const P1: [f64; 8] = [
        4.945235359296727046734888e0, 2.018112620856775083915565e2, 2.290838373831346393026739e3, 1.131967205903380828685045e4,
        2.855724635671635335736389e4, 3.848496228443793359990269e4, 2.637748787624195437963534e4, 7.225813979700288197698961e3,
    ];
    const Q1: [f64; 8] = [
        6.748212550303777196073036e1, 1.113332393857199323513008e3, 7.738757056935398733233834e3, 2.763987074403340708898585e4,
        5.499310206226157329794414e4, 6.161122180066002127833352e4, 3.635127591501940507276287e4, 8.785536302431013170870835e3,
    ];
    const D2: f64 = 4.227843350984671393993777e-1;
    const P2: [f64; 8] = [
        4.974607845568932035012064e0, 5.424138599891070494101986e2, 1.550693864978364947665077e4, 1.847932904445632425417223e5,
        1.088204769468828767498470e6, 3.338152967987029735917223e6, 5.106661678927352456275255e6, 3.074109054850539556250927e6,
    ];
    const Q2: [f64; 8] = [
        1.830328399370592604055942e2, 7.765049321445005871323047e3, 1.331903827966074194402448e5, 1.136705821321969608938755e6,
        5.267964117437946917577538e6, 1.346701454311101692290052e7, 1.782736530353274213975932e7, 9.533095591844353613395747e6,
    ];
    const D4: f64 = 1.791759469228055000094023e0;
    const P4: [f64; 8] = [
        1.474502166059939948905062e4, 2.426813369486704502836312e6, 1.214755574045093227939592e8, 2.663432449630976949898078e9,
        2.940378956634553899906876e10, 1.702665737765398868392998e11, 4.926125793377430887588120e11, 5.606251856223951465078242e11,
    ];
    const Q4: [f64; 8] = [
        2.690530175870899333379843e3, 6.393885654300092398984238e5, 4.135599930241388052042842e7, 1.120872109616147941376570e9,
        1.488613728678813811542398e10, 1.016803586272438228077304e11, 3.417476345507377132798597e11, 4.463158187419713286462081e11,
    ];
    const CS: [f64; 7] = [
        -1.910444077728e-03, 8.4171387781295e-04, -5.952379913043012e-04, 7.93650793500350248e-04, -2.777777777777681622553e-03,
        8.333333333333333331554247e-02, 5.7083835261e-03,
    ];
    const SQRTPI: f64 = 0.9189385332046727417803297;
    const XBIG: f64 = 2.55e305;
    const FRTBIG: f64 = 2.25e76;
    const PNT68: f64 = 0.6796875;
    let y = x;
    if y.is_nan() {
        return NAN;
    }
    if !(y > 0.0 && y <= XBIG) {
        return INF;
    }
    let rational = |xm: f64, p: &[f64; 8], q: &[f64; 8], den0: f64| {
        let mut xden = den0;
        let mut xnum = 0.0;
        for i in 0..8 {
            xnum = xnum * xm + p[i];
            xden = xden * xm + q[i];
        }
        xnum / xden
    };
    if y <= EPS {
        -y.ln()
    } else if y <= 1.5 {
        let (corr, xm1) = if y < PNT68 { (-y.ln(), y) } else { (0.0, (y - 0.5) - 0.5) };
        if y <= 0.5 || y >= PNT68 {
            corr + xm1 * (D1 + xm1 * rational(xm1, &P1, &Q1, 1.0))
        } else {
            let xm2 = (y - 0.5) - 0.5;
            corr + xm2 * (D2 + xm2 * rational(xm2, &P2, &Q2, 1.0))
        }
    } else if y <= 4.0 {
        let xm2 = y - 2.0;
        xm2 * (D2 + xm2 * rational(xm2, &P2, &Q2, 1.0))
    } else if y <= 12.0 {
        let xm4 = y - 4.0;
        D4 + xm4 * rational(xm4, &P4, &Q4, -1.0)
    } else {
        let mut res = 0.0;
        if y <= FRTBIG {
            res = CS[6];
            let ysq = y * y;
            for c in &CS[..6] {
                res = res / ysq + c;
            }
        }
        res /= y;
        let corr = y.ln();
        res = res + SQRTPI - 0.5 * corr;
        res + y * (corr - 1.0)
    }
}

/// Digamma for x >= 0 (Cody's PSI with IEEE double constants).
fn digamma(x: f64) -> f64 {
    const P1: [f64; 9] = [
        4.5104681245762934160e-03, 5.4932855833000385356e+00, 3.7646693175929276856e+02, 7.9525490849151998065e+03, 7.1451595818951933210e+04,
        3.0655976301987365674e+05, 6.3606997788964458797e+05, 5.8041312783537569993e+05, 1.6585695029761022321e+05,
    ];
    const Q1: [f64; 8] = [
        9.6141654774222358525e+01, 2.6287715790581193330e+03, 2.9862497022250277920e+04, 1.6206566091533671639e+05, 4.3487880712768329037e+05,
        5.4256384537269993733e+05, 2.4242185002017985252e+05, 6.4155223783576225996e-08,
    ];
    const P2: [f64; 7] = [
        -2.7103228277757834192e+00, -1.5166271776896121383e+01, -1.9784554148719218667e+01, -8.8100958828312219821e+00, -1.4479614616899842986e+00,
        -7.3689600332394549911e-02, -6.5135387732718171306e-21,
    ];
    const Q2: [f64; 6] =
        [4.4992760373789365846e+01, 2.0240955312679931159e+02, 2.4736979003315290057e+02, 1.0742543875702278326e+02, 1.7463965060678569906e+01, 8.8427520398873480342e-01];
    const XSMALL: f64 = 5.80e-09;
    const XLARGE: f64 = 2.71e+14;
    const X01: f64 = 187.0;
    const X01D: f64 = 128.0;
    const X02: f64 = 6.9464496836234126266e-04;
    if x.is_nan() {
        return NAN;
    }
    if x == 0.0 {
        return -INF;
    }
    let mut x = x;
    let mut aug = 0.0;
    if x < 0.5 && x.abs() <= XSMALL {
        aug = -1.0 / x;
        x = 1.0 - x;
    } else if x < 0.5 {
        // Reflection, only reached for 0 < x < 0.5 here.
        let w = x.abs() - x.abs().trunc();
        let nq = (w * 4.0) as i64;
        let mut w = 4.0 * (w - nq as f64 * 0.25);
        let mut n = nq / 2;
        if n + n != nq {
            w = 1.0 - w;
        }
        let z = PI / 4.0 * w;
        let mut sgn = -PI / 4.0;
        if n % 2 != 0 {
            sgn = -sgn;
        }
        n = (nq + 1) / 2;
        aug = if n % 2 == 0 { sgn * (4.0 / z.tan()) } else { sgn * (4.0 * z.tan()) };
        x = 1.0 - x;
    }
    if x <= 3.0 {
        let mut den = x;
        let mut upper = P1[0] * x;
        for i in 0..7 {
            den = (den + Q1[i]) * x;
            upper = (upper + P1[i + 1]) * x;
        }
        let den = (upper + P1[8]) / (den + Q1[7]);
        let xm = (x - X01 / X01D) - X02;
        return den * xm + aug;
    }
    if x < XLARGE {
        let w = 1.0 / (x * x);
        let mut den = w;
        let mut upper = P2[0] * w;
        for i in 0..5 {
            den = (den + Q2[i]) * w;
            upper = (upper + P2[i + 1]) * w;
        }
        aug += (upper + P2[6]) / (den + Q2[5]) - 0.5 / x;
    }
    aug + x.ln()
}

const BERNOULLI: [f64; 15] = [
    1.0 / 6.0,
    -1.0 / 30.0,
    1.0 / 42.0,
    -1.0 / 30.0,
    5.0 / 66.0,
    -691.0 / 2730.0,
    7.0 / 6.0,
    -3617.0 / 510.0,
    43867.0 / 798.0,
    -174611.0 / 330.0,
    854513.0 / 138.0,
    -236364091.0 / 2730.0,
    8553103.0 / 6.0,
    -23749461029.0 / 870.0,
    8615841276005.0 / 14322.0,
];

/// The k-th derivative of digamma (k >= 1) for x >= 0: shift the argument up, then the
/// asymptotic expansion in Bernoulli numbers.
fn polygamma(k: u32, x: f64) -> f64 {
    if x.is_nan() {
        return NAN;
    }
    let kf = k as f64;
    let sign = if k % 2 == 1 { 1.0 } else { -1.0 };
    if x == 0.0 {
        return sign * INF;
    }
    if x.is_infinite() {
        // MATLAB returns an infinity of the pole's sign here.
        return sign * INF;
    }
    let big = 20.0 + kf;
    let shift = if x < big { (big - x).ceil() as usize } else { 0 };
    let z = x + shift as f64;
    // Sum_{j>=0} 1/(z+j)^(k+1) ~ 1/(k z^k) + 1/(2 z^(k+1)) + sum B_2m g_m / z^(2m+k).
    let zk = z.powf(kf);
    let mut tail = 0.0;
    let mut g = (kf + 1.0) / 2.0;
    let zz = z * z;
    let mut zp = zk * zz;
    let mut terms = Vec::new();
    for (m, b) in BERNOULLI.iter().enumerate() {
        let t = b * g / zp;
        terms.push(t);
        if t.abs() < 1e-18 * (1.0 / (kf * zk)) {
            break;
        }
        let m = (m + 1) as f64;
        g *= (2.0 * m + kf) * (2.0 * m + kf + 1.0) / ((2.0 * m + 1.0) * (2.0 * m + 2.0));
        zp *= zz;
    }
    for t in terms.iter().rev() {
        tail += t;
    }
    tail += 1.0 / (2.0 * zk * z);
    tail += 1.0 / (kf * zk);
    for j in (0..shift).rev() {
        tail += (x + j as f64).powf(-(kf + 1.0));
    }
    let mut fact = 1.0;
    for i in 2..=k {
        fact *= i as f64;
    }
    sign * fact * tail
}

/// Stirling's series remainder: lgamma(a) - ((a-1/2) ln a - a + ln(2 pi)/2), for a >= 10.
fn stirling_rem(a: f64) -> f64 {
    let a2 = a * a;
    let c = [1.0 / 12.0, -1.0 / 360.0, 1.0 / 1260.0, -1.0 / 1680.0, 1.0 / 1188.0, -691.0 / 360360.0, 1.0 / 156.0, -3617.0 / 122400.0];
    let mut s = 0.0;
    for t in c.iter().rev() {
        s = s / a2 + t;
    }
    s / a
}

/// ln(1+t) - t, accurate for small t.
fn log1pmx(t: f64) -> f64 {
    if t.abs() > 0.5 {
        return t.ln_1p() - t;
    }
    // -t^2/2 + t^3/3 - ...
    let mut term = -t * t / 2.0;
    let mut sum = term;
    let mut k = 2.0;
    loop {
        term *= -t * k / (k + 1.0);
        sum += term;
        k += 1.0;
        if term.abs() <= EPS * 1e-2 * sum.abs() || k > 200.0 {
            break;
        }
    }
    sum
}

/// a * (ln(r) - (r - 1)) for r = 1 + d/a, given both d and r so that neither cancels.
fn a_log1pmx(a: f64, d: f64, r: f64) -> f64 {
    let t = d / a;
    if t.abs() <= 0.5 { a * log1pmx(t) } else { a * r.ln() - d }
}

// ---------------------------------------------------------------------------------------------
// Incomplete gamma function

/// x^a e^-x / Gamma(a+1) for x > 0.
fn gamma_prefactor(x: f64, a: f64) -> f64 {
    if a >= 10.0 {
        (a / (2.0 * PI)).sqrt() / a * (a_log1pmx(a, x - a, x / a) - stirling_rem(a)).exp()
    } else {
        let p = x.powf(a) * (-x).exp();
        if p.is_normal() && x < 700.0 {
            p / gammaln(a + 1.0).exp()
        } else {
            (a * x.ln() - x - gammaln(a + 1.0)).exp()
        }
    }
}

/// (P, Q, prefactor, scaled-lower, scaled-upper) for x > 0, a > 0.
fn gammainc_parts(x: f64, a: f64) -> (f64, f64, f64) {
    let pre = gamma_prefactor(x, a);
    if x < a + 1.0 {
        // Series: P = pre * sum_{n>=0} x^n / ((a+1)...(a+n)).
        let mut term = 1.0;
        let mut sum = 1.0;
        let mut n = 1.0;
        loop {
            term *= x / (a + n);
            sum += term;
            n += 1.0;
            if term <= EPS * 0.5 * sum || n > 1e7 {
                break;
            }
        }
        let p = pre * sum;
        (p, 1.0 - p, pre)
    } else {
        // Continued fraction for Q (modified Lentz).
        let tiny = 1e-300;
        let mut b = x + 1.0 - a;
        let mut c = 1.0 / tiny;
        let mut d = 1.0 / b;
        let mut h = d;
        let mut i = 1.0;
        loop {
            let an = -i * (i - a);
            b += 2.0;
            d = an * d + b;
            if d.abs() < tiny {
                d = tiny;
            }
            c = b + an / c;
            if c.abs() < tiny {
                c = tiny;
            }
            d = 1.0 / d;
            let del = d * c;
            h *= del;
            i += 1.0;
            if (del - 1.0).abs() <= EPS * 0.5 || i > 1e7 {
                break;
            }
        }
        let q = a * pre * h;
        (1.0 - q, q, pre)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum GTail {
    Lower,
    Upper,
    ScaledLower,
    ScaledUpper,
}

fn gammainc(x: f64, a: f64, tail: GTail) -> C {
    if x.is_nan() || a.is_nan() {
        return C::new(NAN, 0.0);
    }
    let pick = |p: f64, q: f64, pre: f64| -> f64 {
        match tail {
            GTail::Lower => p,
            GTail::Upper => q,
            GTail::ScaledLower => p / pre,
            GTail::ScaledUpper => q / pre,
        }
    };
    if a == 0.0 {
        return C::new(pick(1.0, 0.0, 1.0), 0.0);
    }
    if x == 0.0 {
        return C::new(
            match tail {
                GTail::Lower => 0.0,
                GTail::Upper => 1.0,
                GTail::ScaledLower => 1.0,
                GTail::ScaledUpper => INF,
            },
            0.0,
        );
    }
    if x == INF {
        return C::new(pick(1.0, 0.0, INF), 0.0);
    }
    if a == INF {
        return C::new(pick(0.0, 1.0, 0.0), 0.0);
    }
    if x < 0.0 {
        // The power series continues to negative x; x^a = |x|^a (cos(pi a) + i sin(pi a)).
        let mut term = 1.0;
        let mut sum = 1.0;
        let mut n = 1.0;
        loop {
            term *= x / (a + n);
            sum += term;
            n += 1.0;
            if term.abs() <= EPS * 0.5 * sum.abs() || n > 1e6 {
                break;
            }
        }
        let mag = (a * (-x).ln() - x - gammaln(a + 1.0)).exp();
        let (s, c) = sinpi_cospi(a);
        let p = C::new(mag * sum * c, mag * sum * s);
        return match tail {
            GTail::Lower => p,
            GTail::Upper => C::new(1.0 - p.re, -p.im),
            GTail::ScaledLower => C::new(sum, 0.0),
            GTail::ScaledUpper => C::new(1.0 - p.re, -p.im).div(C::new(mag * c, mag * s)),
        };
    }
    let (p, q, pre) = gammainc_parts(x, a);
    C::new(pick(p, q, pre), 0.0)
}

/// (sin(pi a), cos(pi a)) with exact zeros at integers and half-integers.
fn sinpi_cospi(a: f64) -> (f64, f64) {
    let r = a.rem_euclid(2.0);
    if r.fract() == 0.0 {
        return (0.0, if r == 0.0 { 1.0 } else { -1.0 });
    }
    if (r - 0.5).fract() == 0.0 {
        return (if r == 0.5 { 1.0 } else { -1.0 }, 0.0);
    }
    ((PI * r).sin(), (PI * r).cos())
}

fn gammaincinv(y: f64, a: f64, upper: bool) -> f64 {
    if y.is_nan() || a.is_nan() {
        return NAN;
    }
    let (p, q) = if upper { (1.0 - y, y) } else { (y, 1.0 - y) };
    if a == 0.0 {
        return 0.0;
    }
    if p == 0.0 || (upper && y == 1.0) {
        return 0.0;
    }
    if q == 0.0 || (!upper && y == 1.0) {
        return INF;
    }
    // Solve with the smaller tail for accuracy.
    let use_lower = if upper { y > 0.5 } else { y <= 0.5 };
    let target = if use_lower { p } else { q };
    // First guess (Numerical Recipes' invgammp, with a power-law start for tiny p).
    let mut x;
    if a > 1.0 {
        let pp = if p < 0.5 { p } else { q };
        let t = (-2.0 * pp.ln()).sqrt();
        let mut z = (2.30753 + t * 0.27061) / (1.0 + t * (0.99229 + t * 0.04481)) - t;
        if p < 0.5 {
            z = -z;
        }
        x = (a * (1.0 - 1.0 / (9.0 * a) - z / (3.0 * a.sqrt())).powi(3)).max(1e-3 * a.min(1.0));
    } else {
        let t = 1.0 - a * (0.253 + a * 0.12);
        x = if p < t { (p / t).powf(1.0 / a) } else { 1.0 - (1.0 - (p - t) / (1.0 - t)).ln() };
    }
    let small = ((p.ln() + gammaln(a + 1.0)) / a).exp();
    if use_lower && small < x && (a <= 1.0 || small < 0.2 * a) {
        x = small;
    }
    if x == 0.0 {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0f64, INF);
    for _ in 0..200 {
        let (pv, qv, pre) = gammainc_parts(x, a);
        // dP/dx = x^(a-1) e^-x / Gamma(a) = a * pre / x.
        let deriv = a * pre / x;
        let err = if use_lower { pv - target } else { target - qv };
        if err > 0.0 {
            hi = hi.min(x);
        } else {
            lo = lo.max(x);
        }
        if deriv == 0.0 || !deriv.is_finite() {
            break;
        }
        let u = err / deriv;
        let corr = u / (1.0 - 0.5 * (u * ((a - 1.0) / x - 1.0)).min(1.0));
        if corr.abs() <= 2.0 * EPS * x {
            x -= corr;
            break;
        }
        let mut xn = x - corr;
        if xn <= lo || xn >= hi {
            xn = if hi.is_finite() { 0.5 * (lo + hi) } else { 2.0 * x.max(lo) };
            if lo == 0.0 && hi.is_finite() {
                xn = (hi * x.max(hi * 1e-300)).sqrt().min(0.5 * hi);
            }
        }
        x = xn;
        if x == 0.0 {
            break;
        }
    }
    x
}

// ---------------------------------------------------------------------------------------------
// Beta family

pub(crate) fn betaln(z: f64, w: f64) -> f64 {
    gammaln(z) + gammaln(w) - gammaln(z + w)
}

/// x^a y^b / B(a,b) with y = 1 - x.
fn beta_prefactor(x: f64, y: f64, a: f64, b: f64) -> f64 {
    if a >= 10.0 && b >= 10.0 {
        let c = a + b;
        let d = x * b - y * a;
        let e = a_log1pmx(a, d, x * c / a) + a_log1pmx(b, -d, y * c / b) - (stirling_rem(a) + stirling_rem(b) - stirling_rem(c));
        return (a * b / (2.0 * PI * c)).sqrt() * e.exp();
    }
    if a.max(b) >= 10.0 {
        // One large parameter: Gamma(a+b)/Gamma(big) by Stirling, the small one directly.
        let (big, sm, xb, xs) = if a >= b { (a, b, x, y) } else { (b, a, y, x) };
        let c = big + sm;
        let l = big * xb.ln() + sm * xs.ln() + (big - 0.5) * (sm / big).ln_1p() + sm * c.ln() - sm - stirling_rem(big) + stirling_rem(c) - gammaln(sm);
        return l.exp();
    }
    let p = x.powf(a) * y.powf(b);
    if p.is_normal() {
        p / betaln(a, b).exp()
    } else {
        (a * x.ln() + b * y.ln() - betaln(a, b)).exp()
    }
}

/// Continued fraction for the incomplete beta function (modified Lentz).
fn betacf(x: f64, a: f64, b: f64) -> f64 {
    let tiny = 1e-300;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut h = d;
    let mut m = 1.0;
    while m < 1e7 {
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() <= EPS * 0.5 {
            break;
        }
        m += 1.0;
    }
    h
}

/// (lower, upper) regularized incomplete beta at x, one of them computed directly.
fn betainc_pair(x: f64, a: f64, b: f64) -> (f64, f64) {
    if x == 0.0 {
        return (0.0, 1.0);
    }
    if x == 1.0 {
        return (1.0, 0.0);
    }
    if a == 0.0 && b == 0.0 || a.is_infinite() && b.is_infinite() {
        return (NAN, NAN);
    }
    if a == 0.0 || b.is_infinite() {
        return (1.0, 0.0);
    }
    if b == 0.0 || a.is_infinite() {
        return (0.0, 1.0);
    }
    let y = 1.0 - x;
    let front = beta_prefactor(x, y, a, b);
    if x < (a + 1.0) / (a + b + 2.0) {
        let l = front * betacf(x, a, b) / a;
        (l, 1.0 - l)
    } else {
        let u = front * betacf(y, b, a) / b;
        (1.0 - u, u)
    }
}

fn betaincinv(yv: f64, a: f64, b: f64, upper: bool) -> f64 {
    if yv.is_nan() || a.is_nan() || b.is_nan() {
        return NAN;
    }
    let p = if upper { 1.0 - yv } else { yv };
    if (!upper && yv == 0.0) || (upper && yv == 1.0) {
        return 0.0;
    }
    if (!upper && yv == 1.0) || (upper && yv == 0.0) {
        return 1.0;
    }
    if a == 0.0 {
        return 0.0;
    }
    if b == 0.0 {
        return 1.0;
    }
    let use_lower = if upper { yv > 0.5 } else { yv <= 0.5 };
    let target = if use_lower { p } else if upper { yv } else { 1.0 - yv };
    // First guess (Numerical Recipes' invbetai).
    let mut x;
    if a >= 1.0 && b >= 1.0 {
        let pp = if p < 0.5 { p } else { 1.0 - p };
        let t = (-2.0 * pp.ln()).sqrt();
        let mut z = (2.30753 + t * 0.27061) / (1.0 + t * (0.99229 + t * 0.04481)) - t;
        if p < 0.5 {
            z = -z;
        }
        let al = (z * z - 3.0) / 6.0;
        let h = 2.0 / (1.0 / (2.0 * a - 1.0) + 1.0 / (2.0 * b - 1.0));
        let w = z * (al + h).sqrt() / h - (1.0 / (2.0 * b - 1.0) - 1.0 / (2.0 * a - 1.0)) * (al + 5.0 / 6.0 - 2.0 / (3.0 * h));
        x = a / (a + b * (2.0 * w).exp());
    } else {
        let lna = (a / (a + b)).ln();
        let lnb = (b / (a + b)).ln();
        let t = (a * lna).exp() / a;
        let u = (b * lnb).exp() / b;
        let w = t + u;
        x = if p < t / w { (a * w * p).powf(1.0 / a) } else { 1.0 - (b * w * (1.0 - p)).powf(1.0 / b) };
    }
    if !(x > 0.0 && x < 1.0) {
        x = 0.5;
    }
    let lb = betaln(a, b);
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..300 {
        let (l, u) = betainc_pair(x, a, b);
        let err = if use_lower { l - target } else { target - u };
        if err > 0.0 {
            hi = hi.min(x);
        } else {
            lo = lo.max(x);
        }
        if err == 0.0 {
            break;
        }
        let deriv = ((a - 1.0) * x.ln() + (b - 1.0) * (-x).ln_1p() - lb).exp();
        if deriv == 0.0 || !deriv.is_finite() {
            let xn = 0.5 * (lo + hi);
            if xn == x {
                break;
            }
            x = xn;
            continue;
        }
        let u = err / deriv;
        let corr = u / (1.0 - 0.5 * (u * ((a - 1.0) / x - (b - 1.0) / (1.0 - x))).min(1.0));
        if corr.abs() <= 2.0 * EPS * x {
            x -= corr;
            break;
        }
        let mut xn = x - corr;
        if xn <= lo || xn >= hi {
            xn = if lo == 0.0 { (x.min(hi) * 0.5).min(0.5 * (lo + hi)) } else { 0.5 * (lo + hi) };
        }
        x = xn;
    }
    x
}

// ---------------------------------------------------------------------------------------------
// Exponential integral

/// Cody's CALCEI: `int` 1 gives Ei(x), 2 gives E1(x) for x > 0.
fn calcei(arg: f64, int: u8) -> f64 {
    const A: [f64; 7] = [
        1.1669552669734461083368e2, 2.1500672908092918123209e3, 1.5924175980637303639884e4, 8.9904972007457256553251e4, 1.5026059476436982420737e5,
        -1.4815102102575750838086e5, 5.0196785185439843791020e0,
    ];
    const B: [f64; 6] = [4.0205465640027706061433e1, 7.5043163907103936624165e2, 8.1258035174768735759855e3, 5.2440529172056355429883e4, 1.8434070063353677359298e5, 2.5666493484897117319268e5];
    const C9: [f64; 9] = [
        3.828573121022477169108e-1, 1.107326627786831743809e+1, 7.246689782858597021199e+1, 1.700632978311516129328e+2, 1.698106763764238382705e+2,
        7.633628843705946890896e+1, 1.487967702840464066613e+1, 9.999989642347613068437e-1, 1.737331760720576030932e-8,
    ];
    const D9: [f64; 9] = [
        8.258160008564488034698e-2, 4.344836335509282083360e+0, 4.662179610356861756812e+1, 1.775728186717289799677e+2, 2.953136335677908517423e+2,
        2.342573504717625153053e+2, 9.021658450529372642314e+1, 1.587964570758947927903e+1, 1.000000000000000000000e+0,
    ];
    const E: [f64; 10] = [
        1.3276881505637444622987e+2, 3.5846198743996904308695e+4, 1.7283375773777593926828e+5, 2.6181454937205639647381e+5, 1.7503273087497081314708e+5,
        5.9346841538837119172356e+4, 1.0816852399095915622498e+4, 1.0611777263550331766871e03, 5.2199632588522572481039e+1, 9.9999999999999999087819e-1,
    ];
    const F: [f64; 10] = [
        3.9147856245556345627078e+4, 2.5989762083608489777411e+5, 5.5903756210022864003380e+5, 5.4616842050691155735758e+5, 2.7858134710520842139357e+5,
        7.9231787945279043698718e+4, 1.2842808586627297365998e+4, 1.1635769915320848035459e+3, 5.4199632588522559414924e+1, 1.0e0,
    ];
    const PLG: [f64; 4] = [-2.4562334077563243311e+01, 2.3642701335621505212e+02, -5.4989956895857911039e+02, 3.5687548468071500413e+02];
    const QLG: [f64; 4] = [-3.5553900764052419184e+01, 1.9400230218539473193e+02, -3.3442903192607538956e+02, 1.7843774234035750207e+02];
    const P: [f64; 10] = [
        -1.2963702602474830028590e01, -1.2831220659262000678155e03, -1.4287072500197005777376e04, -1.4299841572091610380064e06, -3.1398660864247265862050e05,
        -3.5377809694431133484800e08, 3.1984354235237738511048e08, -2.5301823984599019348858e10, 1.2177698136199594677580e10, -2.0829040666802497120940e11,
    ];
    const Q: [f64; 10] = [
        7.6886718750000000000000e01, -5.5648470543369082846819e03, 1.9418469440759880361415e05, -4.2648434812177161405483e06, 6.4698830956576428587653e07,
        -7.0108568774215954065376e08, 5.4229617984472955011862e09, -2.8986272696554495342658e10, 9.8900934262481749439886e10, -8.9673749185755048616855e10,
    ];
    const RR: [f64; 10] = [
        -2.645677793077147237806e00, -2.378372882815725244124e00, -2.421106956980653511550e01, 1.052976392459015155422e01, 1.945603779539281810439e01,
        -3.015761863840593359165e01, 1.120011024227297451523e01, -3.988850730390541057912e00, 9.565134591978630774217e00, 9.981193787537396413219e-1,
    ];
    const S: [f64; 9] = [
        1.598517957704779356479e-4, 4.644185932583286942650e00, 3.697412299772985940785e02, -8.791401054875438925029e00, 7.608194509086645763123e02,
        2.852397548119248700147e01, 4.731097187816050252967e02, -2.369210235636181001661e02, 1.249884822712447891440e00,
    ];
    const P1: [f64; 10] = [
        -1.647721172463463140042e00, -1.860092121726437582253e01, -1.000641913989284829961e01, -2.105740799548040450394e01, -9.134835699998742552432e-1,
        -3.323612579343962284333e01, 2.495487730402059440626e01, 2.652575818452799819855e01, -1.845086232391278674524e00, 9.999933106160568739091e-1,
    ];
    const Q1: [f64; 9] = [
        9.792403599217290296840e01, 6.403800405352415551324e01, 5.994932325667407355255e01, 2.538819315630708031713e02, 4.429413178337928401161e01,
        1.192832423968601006985e03, 1.991004470817742470726e02, -1.093556195391091143924e01, 1.001533852045342697818e00,
    ];
    const P2: [f64; 10] = [
        1.75338801265465972390e02, -2.23127670777632409550e02, -1.81949664929868906455e01, -2.79798528624305389340e01, -7.63147701620253630855e00,
        -1.52856623636929636839e01, -7.06810977895029358836e00, -5.00006640413131002475e00, -3.00000000320981265753e00, 1.00000000000000485503e00,
    ];
    const Q2: [f64; 9] = [
        3.97845977167414720840e04, 3.97277109100414518365e00, 1.37790390235747998793e02, 1.17179220502086455287e02, 7.04831847180424675988e01,
        -1.20187763547154743238e01, -7.99243595776339741065e00, -2.99999894040324959612e00, 1.99999999999048104167e00,
    ];
    const X01: f64 = 381.5;
    const X11: f64 = 1024.0;
    const X02: f64 = -5.1182968633365538008e-5;
    const X0: f64 = 3.7250741078136663466e-1;
    const XMAX: f64 = 716.351;
    const XBIG: f64 = 701.84;
    const EXP40: f64 = 2.3538526683701998541e17;
    let x = arg;
    if x == 0.0 {
        return if int == 2 { INF } else { -INF };
    }
    if x < 0.0 || int == 2 {
        let y = x.abs();
        let mut ei;
        if y <= 1.0 {
            let mut sump = A[6] * y + A[0];
            let mut sumq = y + B[0];
            for i in 1..6 {
                sump = sump * y + A[i];
                sumq = sumq * y + B[i];
            }
            ei = y.ln() - sump / sumq;
        } else if y <= 4.0 {
            let w = 1.0 / y;
            let mut sump = C9[0];
            let mut sumq = D9[0];
            for i in 1..9 {
                sump = sump * w + C9[i];
                sumq = sumq * w + D9[i];
            }
            ei = -sump / sumq * (-y).exp();
        } else if y > XBIG {
            ei = 0.0;
        } else {
            let w = 1.0 / y;
            let mut sump = E[0];
            let mut sumq = F[0];
            for i in 1..10 {
                sump = sump * w + E[i];
                sumq = sumq * w + F[i];
            }
            ei = -w * (1.0 - w * sump / sumq) * (-y).exp();
        }
        if int == 2 {
            ei = -ei;
        }
        return ei;
    }
    if x < 6.0 {
        let t = (x + x) / 3.0 - 2.0;
        let mut px = [0.0; 10];
        let mut qx = [0.0; 10];
        px[1] = P[0];
        qx[1] = Q[0];
        for i in 1..9 {
            px[i + 1] = t * px[i] - px[i - 1] + P[i];
            qx[i + 1] = t * qx[i] - qx[i - 1] + Q[i];
        }
        let sump = 0.5 * t * px[9] - px[8] + P[9];
        let sumq = 0.5 * t * qx[9] - qx[8] + Q[9];
        let frac = sump / sumq;
        let xmx0 = (x - X01 / X11) - X02;
        if xmx0.abs() >= 0.037 {
            (x / X0).ln() + xmx0 * frac
        } else {
            let y = xmx0 / (x + X0);
            let ysq = y * y;
            let mut sump = PLG[0];
            let mut sumq = ysq + QLG[0];
            for i in 1..4 {
                sump = sump * ysq + PLG[i];
                sumq = sumq * ysq + QLG[i];
            }
            (sump / (sumq * (x + X0)) + frac) * xmx0
        }
    } else if x < 12.0 {
        let mut frac = 0.0;
        for i in 0..9 {
            frac = S[i] / (RR[i] + x + frac);
        }
        (RR[9] + frac) / x * x.exp()
    } else if x <= 24.0 {
        let mut frac = 0.0;
        for i in 0..9 {
            frac = Q1[i] / (P1[i] + x + frac);
        }
        (P1[9] + frac) / x * x.exp()
    } else if x >= XMAX {
        INF
    } else {
        let y = 1.0 / x;
        let mut frac = 0.0;
        for i in 0..9 {
            frac = Q2[i] / (P2[i] + x + frac);
        }
        frac += P2[9];
        let ei = y + y * y * frac;
        if x <= XMAX - 24.0 { ei * x.exp() } else { (ei * (x - 40.0).exp()) * EXP40 }
    }
}

fn expint_real(x: f64) -> C {
    // MATLAB's own values at the non-finite points.
    if x.is_nan() {
        C::new(0.0, 0.0)
    } else if x == INF {
        C::new(NAN, 0.0)
    } else if x >= 0.0 {
        C::new(calcei(x, 2), 0.0)
    } else if x == -INF {
        C::new(NAN, -PI)
    } else {
        C::new(-calcei(-x, 1), -PI)
    }
}

/// E1(z) for complex z: the power series near the origin and around the negative real axis,
/// the continued fraction elsewhere.
fn expint_complex(z: C) -> C {
    if z.im == 0.0 {
        return expint_real(z.re);
    }
    if z.re.is_nan() || z.im.is_nan() {
        return C::new(NAN, NAN);
    }
    let r = z.abs();
    if r <= 2.0 || r + z.re < 3.0 {
        // -gamma - log z - sum_{k>=1} (-z)^k / (k k!)
        let mut term = C::new(1.0, 0.0);
        let mut sum = C::new(0.0, 0.0);
        let mz = C::new(-z.re, -z.im);
        let mut k = 1.0;
        loop {
            term = term.mul(mz).div(C::new(k, 0.0));
            let t = term.div(C::new(k, 0.0));
            sum = sum.add(t);
            if t.abs() <= EPS * 0.25 * sum.abs() || k > 5000.0 {
                break;
            }
            k += 1.0;
        }
        let euler = 0.57721566490153286061;
        return C::new(-euler, 0.0).sub(z.ln()).sub(sum);
    }
    // E1(z) = e^-z / (z + 1 - 1/(z + 3 - 4/(z + 5 - ...)))
    let tiny = 1e-300;
    let mut b = z.add(C::new(1.0, 0.0));
    let mut c = C::new(1.0 / tiny, 0.0);
    let mut d = C::new(1.0, 0.0).div(b);
    let mut h = d;
    let mut i = 1.0;
    while i < 1e6 {
        let an = C::new(-i * i, 0.0);
        b = b.add(C::new(2.0, 0.0));
        d = C::new(1.0, 0.0).div(an.mul(d).add(b));
        c = b.add(an.div(c));
        let del = c.mul(d);
        h = h.mul(del);
        if del.sub(C::new(1.0, 0.0)).abs() <= EPS * 0.5 {
            break;
        }
        i += 1.0;
    }
    h.mul(C::new(-z.re, -z.im).exp())
}

// ---------------------------------------------------------------------------------------------
// Elliptic functions

/// Arithmetic-geometric mean sequence for parameter m: (a_n, c_n) for n = 0..=steps.
fn agm(m: f64, steps: usize) -> (Vec<f64>, Vec<f64>) {
    let mut a = vec![1.0];
    let mut c = vec![m.sqrt()];
    let mut b = (1.0 - m).sqrt();
    for _ in 0..steps {
        let an = *a.last().unwrap();
        a.push((an + b) / 2.0);
        c.push((an - b) / 2.0);
        b = (an * b).sqrt();
    }
    (a, c)
}

/// AGM steps until |c_n| <= tol.
fn agm_steps(m: f64, tol: f64) -> usize {
    let (mut a, mut b, mut c) = (1.0f64, (1.0 - m).sqrt(), m.sqrt());
    let mut n = 0;
    while c.abs() > tol && n < 2000 {
        let an = (a + b) / 2.0;
        c = (a - b) / 2.0;
        b = (a * b).sqrt();
        a = an;
        n += 1;
    }
    n
}

/// Complete elliptic integrals; MATLAB iterates all elements together, so every element takes
/// as many AGM steps as the slowest one.
fn ellipke(ms: &[f64], tol: f64) -> (Vec<f64>, Vec<f64>) {
    let steps = ms.iter().filter(|m| m.is_finite() && **m < 1.0).map(|&m| agm_steps(m, tol)).max().unwrap_or(0);
    ms.iter()
        .map(|&m| {
            if m.is_nan() {
                return (NAN, NAN);
            }
            if m == 1.0 {
                return (INF, 1.0);
            }
            let (a, c) = agm(m, steps);
            let mut s = 0.0;
            let mut w = 0.5;
            for cn in &c {
                s += w * cn * cn;
                w *= 2.0;
            }
            let k = PI / (2.0 * a[steps]);
            (k, k * (1.0 - s))
        })
        .unzip()
}

fn ellipj(us: &[f64], ms: &[f64], tol: f64) -> Vec<(f64, f64, f64)> {
    let steps = ms.iter().filter(|m| **m < 1.0).map(|&m| agm_steps(m, tol)).max().unwrap_or(0);
    us.iter()
        .zip(ms)
        .map(|(&u, &m)| {
            if m == 1.0 {
                let sech = 1.0 / u.cosh();
                return (u.tanh(), sech, sech);
            }
            let (a, c) = agm(m, steps);
            // u is first reduced by the period 4K, as MATLAB does.
            let k = PI / (2.0 * a[steps]);
            let u = u % (4.0 * k);
            let mut phi = 2f64.powi(steps as i32) * a[steps] * u;
            for n in (1..=steps).rev() {
                phi = ((c[n] * phi.sin() / a[n]).asin() + phi) / 2.0;
            }
            let sn = phi.sin();
            (sn, phi.cos(), (1.0 - m * sn * sn).sqrt())
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Legendre functions

#[derive(Clone, Copy, PartialEq)]
enum LegNorm {
    Unnorm,
    Sch,
    Norm,
}

/// All orders m = 0..=n at one point, by backward recursion in m on the Schmidt-like functions
/// Q(n,m) (P = sqrt((n+m)!/(n-m)!) Q).
fn legendre_point(n: usize, x: f64, norm: LegNorm) -> Vec<f64> {
    let nf = n as f64;
    let sq = f64::sqrt;
    let mut q = vec![0.0; n + 2];
    if x.is_nan() {
        q.truncate(n + 1);
        q.iter_mut().for_each(|v| *v = 0.0);
    } else if x.abs() == 1.0 {
        q[0] = x.powi(n as i32);
    } else {
        let s = sq(1.0 - x * x);
        let tc = -2.0 * x / s;
        let mut c = 1.0;
        for k in 1..=n {
            c *= 1.0 - 1.0 / (2.0 * k as f64);
        }
        q[n] = sq(c) * (-s).powi(n as i32);
        if n >= 1 {
            q[n - 1] = q[n] * tc * nf / sq(2.0 * nf);
        }
        for m in (0..n.saturating_sub(1)).rev() {
            let mf = m as f64;
            let a = tc * (mf + 1.0);
            q[m] = (q[m + 1] * a - q[m + 2] * sq(nf + mf + 2.0) * sq(nf - mf - 1.0)) / (sq(nf + mf + 1.0) * sq(nf - mf));
        }
    }
    q.truncate(n + 1);
    match norm {
        LegNorm::Unnorm => (0..=n)
            .map(|m| {
                let mut f = 1.0;
                for k in (n - m + 1)..=(n + m) {
                    f *= sq(k as f64);
                }
                q[m] * f
            })
            .collect(),
        LegNorm::Sch => (0..=n).map(|m| if m == 0 { q[0] } else { sign_m(m) * sq(2.0) * q[m] }).collect(),
        LegNorm::Norm => {
            if n == 0 {
                return vec![q[0] / sq(2.0)];
            }
            (0..=n).map(|m| sign_m(m) * sq(nf + 0.5) * q[m]).collect()
        }
    }
}

fn sign_m(m: usize) -> f64 {
    if m.is_multiple_of(2) { 1.0 } else { -1.0 }
}

// ---------------------------------------------------------------------------------------------
// Rational approximation

/// One element's continued fraction: (terms, numerator, denominator).
fn rat_expand(x: f64, tol: f64) -> (Vec<f64>, f64, f64) {
    if !x.is_finite() {
        return (vec![x], if x.is_nan() { 0.0 } else { x.signum() }, 0.0);
    }
    let first = x.round();
    let mut terms = vec![first];
    let (mut n, mut d) = (first, 1.0);
    let (mut ln, mut ld) = (1.0, 0.0);
    let mut frac = x - first;
    // Stop at the tolerance, or once the error is within eps(x).
    while (x - n / d).abs() >= tol && (x - n / d).abs() > spacing(x) && frac != 0.0 && terms.len() < 64 {
        let flip = 1.0 / frac;
        let step = flip.round();
        frac = flip - step;
        terms.push(step);
        let nn = n * step + ln;
        let dd = d * step + ld;
        ln = n;
        ld = d;
        n = nn;
        d = dd;
    }
    (terms, n * d.signum(), d.abs())
}

/// eps(x): the distance to the next larger double in magnitude.
fn spacing(x: f64) -> f64 {
    let a = x.abs();
    if a < f64::MIN_POSITIVE { return f64::from_bits(1) }
    f64::from_bits(a.to_bits() + 1) - a
}

fn int_text(v: f64) -> String {
    if v.is_nan() {
        "NaN".into()
    } else if v.is_infinite() {
        if v > 0.0 { "Inf".into() } else { "-Inf".into() }
    } else if v == 0.0 && v.is_sign_negative() {
        "-0".into()
    } else {
        format!("{v:.0}")
    }
}

fn rat_text(terms: &[f64]) -> String {
    let mut s = int_text(terms[0]);
    for t in &terms[1..] {
        s.push_str(" + 1/(");
        s.push_str(&int_text(*t));
    }
    s.push_str(&")".repeat(terms.len() - 1));
    s
}

fn char_rows(rows: &[String]) -> Matrix {
    let width = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    let mut data = vec![32.0; rows.len() * width];
    for (i, r) in rows.iter().enumerate() {
        for (j, ch) in r.chars().enumerate() {
            data[j * rows.len() + i] = ch as u32 as f64;
        }
    }
    Matrix::from_dims(vec![rows.len(), width], data).with_kind(Kind::Char)
}

fn default_rat_tol(m: &Matrix) -> f64 {
    let mut s = 0.0;
    for i in 0..m.numel() {
        let v = C::new(m.data[i], m.im_at(i)).abs();
        if v.is_finite() {
            s += v;
        }
    }
    1e-6 * s
}

/// One element laid out in a rats field of `len` + 1 characters: fractions aligned on the
/// slash, integers centred, and an asterisk when the approximation does not fit.
fn rats_field(n: f64, d: f64, len: usize) -> String {
    let slash = len.div_ceil(2);
    let center = |s: &str| -> String {
        let l = s.chars().count();
        let left = (len - l) / 2;
        format!(" {}{}{}", " ".repeat(left), s, " ".repeat(len - l - left))
    };
    let (num, den) = if d == 0.0 {
        let s = if n == 0.0 { "0" } else if n > 0.0 { "1" } else { "-1" };
        (s.to_string(), "0".to_string())
    } else if d == 1.0 {
        let s = int_text(if n == 0.0 { 0.0 } else { n });
        return if s.len() <= len { center(&s) } else { center("*") };
    } else {
        (int_text(n), int_text(d))
    };
    if len < 2 || num.len() > slash - 1 || den.len() > len - slash {
        return center("*");
    }
    format!(" {:>w1$}/{:<w2$}", num, den, w1 = slash - 1, w2 = len - slash)
}

// ---------------------------------------------------------------------------------------------
// Colour spaces

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> [f64; 3] {
    let h6 = 6.0 * h;
    let k = h6.floor();
    let f = h6 - k;
    let t = 1.0 - s;
    let n = 1.0 - s * f;
    let p = 1.0 - s * (1.0 - f);
    let c = match (k as i64).rem_euclid(6) {
        0 => [1.0, p, t],
        1 => [n, 1.0, t],
        2 => [t, 1.0, p],
        3 => [t, n, 1.0],
        4 => [p, t, 1.0],
        _ => [1.0, t, n],
    };
    [c[0] * v, c[1] * v, c[2] * v]
}

fn rgb_to_hsv(r: f64, g: f64, b: f64) -> [f64; 3] {
    let v = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = v - mn;
    let s = if v == 0.0 { 0.0 } else { d / v };
    let mut h = 0.0;
    if d != 0.0 {
        if r == v {
            h = (g - b) / (6.0 * d);
        } else if g == v {
            h = (b - r) / (6.0 * d) + 1.0 / 3.0;
        } else {
            h = (r - g) / (6.0 * d) + (1.0 - 1.0 / 3.0);
        }
        if h < 0.0 {
            h += 1.0;
        }
    }
    [h, s, v]
}

// ---------------------------------------------------------------------------------------------
// Builtins

pub fn call(name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    let outs = match name {
        "besselj" | "bessely" | "besseli" | "besselk" | "besselh" => vec![bessel(name, a)?],
        "airy" => {
            if nargout > 1 {
                return rt("Too many output arguments.");
            }
            vec![airy(a)?]
        }
        "erf" | "erfc" | "erfcx" | "erfinv" | "erfcinv" => {
            let x = float_arg(a, 0, name)?;
            require_real(&x, "Input must be real and full.")?;
            let f: fn(f64) -> f64 = match name {
                "erf" => erf,
                "erfc" => erfc,
                "erfcx" => erfcx,
                "erfinv" => erfinv,
                _ => erfcinv,
            };
            vec![Value::Num(real_out(x.dims.clone(), x.data.iter().map(|&v| f(v)).collect(), x.kind))]
        }
        "gammaln" => {
            let x = float_arg(a, 0, name)?;
            require_real(&x, "Input must be real and full.")?;
            if x.data.iter().any(|&v| v < 0.0) {
                return rt("Input must be nonnegative.");
            }
            vec![Value::Num(real_out(x.dims.clone(), x.data.iter().map(|&v| gammaln(v)).collect(), x.kind))]
        }
        "psi" => vec![psi(a)?],
        "beta" | "betaln" => {
            let z = float_arg(a, 0, name)?;
            let w = float_arg(a, 1, name)?;
            if z.is_complex() || w.is_complex() {
                return rt("Input must be real and full.");
            }
            if z.data.iter().chain(&w.data).any(|&v| v < 0.0) {
                return rt("Input must be nonnegative.");
            }
            let (dims, idx) = expand(&[&z, &w])?;
            let data = (0..nd::numel(&dims))
                .map(|i| {
                    let l = betaln(z.data[idx[0][i]], w.data[idx[1][i]]);
                    if name == "beta" { l.exp() } else { l }
                })
                .collect();
            vec![Value::Num(real_out(dims, data, out_kind(&[&z, &w])))]
        }
        "betainc" | "betaincinv" => vec![betainc_builtin(name, a)?],
        "gammainc" | "gammaincinv" => vec![gammainc_builtin(name, a)?],
        "expint" => {
            let x = float_arg(a, 0, name)?;
            let z: Vec<C> = (0..x.numel())
                .map(|i| if x.is_complex() { expint_complex(C::new(x.data[i], x.im_at(i))) } else { expint_real(x.data[i]) })
                .collect();
            vec![Value::Num(complex_out(x.dims.clone(), z, x.kind))]
        }
        "ellipke" => ellipke_builtin(a, nargout)?,
        "ellipj" => ellipj_builtin(a, nargout)?,
        "legendre" => vec![legendre_builtin(a)?],
        "rat" => rat_builtin(a, nargout)?,
        "rats" => vec![rats_builtin(a)?],
        "perms" => vec![perms(a)?],
        "cart2pol" | "pol2cart" | "cart2sph" | "sph2cart" => coords(name, a, nargout)?,
        "hsv2rgb" | "rgb2hsv" => colour(name, a, nargout)?,
        _ => unreachable!(),
    };
    Ok(Some(outs))
}

fn to_c64(z: C) -> Complex64 {
    Complex64::new(z.re, z.im)
}

fn from_c64(z: Complex64) -> C {
    C::new(z.re, z.im)
}

fn scale_arg(a: &[Value], i: usize) -> R<bool> {
    match a.get(i) {
        None => Ok(false),
        Some(Value::Num(m)) if m.kind.is_float() => {
            if m.numel() != 1 || m.is_complex() || (m.data[0] != 0.0 && m.data[0] != 1.0) {
                return rt("SCALE must be 0 or 1.");
            }
            Ok(m.data[0] == 1.0)
        }
        Some(_) => rt("Inputs must be single or double."),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum BKind {
    J,
    Y,
    I,
    K,
    H1,
    H2,
}

fn bessel_value(kind: BKind, nu: f64, z: C, scaled: bool) -> C {
    if nu.is_nan() || z.re.is_nan() || z.im.is_nan() {
        return C::new(NAN, 0.0);
    }
    if z.re.is_infinite() || z.im.is_infinite() {
        return match kind {
            BKind::I if z.im == 0.0 && z.re > 0.0 && !scaled => C::new(INF, 0.0),
            BKind::K if z.im == 0.0 && z.re > 0.0 => C::new(0.0, 0.0),
            _ => C::new(NAN, 0.0),
        };
    }
    if z.re == 0.0 && z.im == 0.0 {
        return match kind {
            BKind::J | BKind::I => {
                if nu == 0.0 {
                    C::new(1.0, 0.0)
                } else if nu > 0.0 {
                    C::new(0.0, 0.0)
                } else if nu.fract() == 0.0 {
                    C::new(if (nu as i64) % 2 == 0 { 0.0 } else { -0.0 }, 0.0)
                } else {
                    C::new(INF, 0.0)
                }
            }
            BKind::Y => C::new(-INF, 0.0),
            BKind::K => C::new(INF, 0.0),
            BKind::H1 | BKind::H2 => C::new(NAN, 0.0),
        };
    }
    if z.im != 0.0 {
        return amos(kind, nu, z, scaled);
    }
    let x = z.re;
    if x > 0.0 {
        // Real on the positive axis; Amos leaves rounding noise in the imaginary part.
        return match kind {
            BKind::H1 | BKind::H2 => amos(kind, nu, z, scaled),
            _ => C::new(amos(kind, nu, z, scaled).re, 0.0),
        };
    }
    // Negative real axis, approached from above: reflect with exact sin(pi nu), cos(pi nu).
    let ax = C::new(-x, 0.0);
    let (s, c) = sinpi_cospi(nu);
    let j = || amos(BKind::J, nu, ax, false).re;
    let y = || amos(BKind::Y, nu, ax, false).re;
    match kind {
        BKind::J => C::new(c * j(), s * j()),
        BKind::I => {
            let i = amos(BKind::I, nu, ax, scaled).re;
            C::new(c * i, s * i)
        }
        BKind::Y => {
            let (jv, yv) = (j(), y());
            C::new(c * yv, -s * yv + 2.0 * c * jv)
        }
        BKind::K => {
            // K(-x) = e^(-i pi nu) K(x) - i pi I(x); scaled by e^z = e^-|x|.
            let (k, i) = if scaled {
                (amos(BKind::K, nu, ax, true).re * (2.0 * x).exp(), amos(BKind::I, nu, ax, true).re)
            } else {
                (amos(BKind::K, nu, ax, false).re, amos(BKind::I, nu, ax, false).re)
            };
            C::new(c * k, -s * k - PI * i)
        }
        BKind::H1 | BKind::H2 => {
            let (jv, yv) = (C::new(c * j(), s * j()), {
                let (jr, yr) = (j(), y());
                C::new(c * yr, -s * yr + 2.0 * c * jr)
            });
            let iy = C::new(-yv.im, yv.re);
            let h = if kind == BKind::H1 { jv.add(iy) } else { jv.sub(iy) };
            if !scaled {
                return h;
            }
            // H1 * e^(-iz), H2 * e^(iz) with z = x real.
            let ph = if kind == BKind::H1 { -x } else { x };
            h.mul(C::new(ph.cos(), ph.sin()))
        }
    }
}

/// One Amos evaluation, with MATLAB's treatment of its error flags: overflow gives Inf, and
/// J and Y keep their leading asymptotic term where Amos gives up for huge real arguments.
fn amos(kind: BKind, nu: f64, z: C, scaled: bool) -> C {
    let w = to_c64(z);
    let r = match (kind, scaled) {
        (BKind::J, false) => cb::besselj(nu, w),
        (BKind::J, true) => cb::besselj_scaled(nu, w),
        (BKind::Y, false) => cb::bessely(nu, w),
        (BKind::Y, true) => cb::bessely_scaled(nu, w),
        (BKind::I, false) => cb::besseli(nu, w),
        (BKind::I, true) => cb::besseli_scaled(nu, w),
        (BKind::K, false) => cb::besselk(nu, w),
        (BKind::K, true) => cb::besselk_scaled(nu, w),
        (BKind::H1, false) => cb::hankel1(nu, w),
        (BKind::H1, true) => cb::hankel1_scaled(nu, w),
        (BKind::H2, false) => cb::hankel2(nu, w),
        (BKind::H2, true) => cb::hankel2_scaled(nu, w),
    };
    match r {
        Ok(v) => from_c64(v),
        Err(cb::Error::Overflow) => C::new(INF, 0.0),
        Err(cb::Error::TotalPrecisionLoss) if z.im == 0.0 && z.re > 0.0 && matches!(kind, BKind::J | BKind::Y) => {
            let x = z.re;
            let amp = (2.0 / (PI * x)).sqrt();
            let (sp, cp) = (PI * (nu / 2.0 + 0.25)).sin_cos();
            let (sx, cx) = x.sin_cos();
            let v = if kind == BKind::J { cx * cp + sx * sp } else { sx * cp - cx * sp };
            C::new(amp * v, 0.0)
        }
        Err(_) => C::new(NAN, 0.0),
    }
}

fn bessel(name: &str, a: &[Value]) -> R<Value> {
    let (kind, zi) = match name {
        "besselj" => (BKind::J, 1),
        "bessely" => (BKind::Y, 1),
        "besseli" => (BKind::I, 1),
        "besselk" => (BKind::K, 1),
        _ => {
            if a.len() >= 3 {
                let k = float_arg(a, 1, name)?;
                let kind = match (k.numel(), k.data.first()) {
                    (1, Some(&1.0)) => BKind::H1,
                    (1, Some(&2.0)) => BKind::H2,
                    _ => return rt("K must be 1 or 2."),
                };
                (kind, 2)
            } else {
                (BKind::H1, 1)
            }
        }
    };
    let nu = float_arg(a, 0, name)?;
    let z = float_arg(a, zi, name)?;
    let scaled = scale_arg(a, zi + 1)?;
    if nu.is_complex() {
        return rt("NU must be real.");
    }
    let dims = same_size(&[&nu, &z], "NU and Z must be the same size or one must be a scalar.")?;
    let n = nd::numel(&dims);
    let out: Vec<C> = (0..n).map(|i| bessel_value(kind, at(&nu, i), cat(&z, i), scaled)).collect();
    Ok(Value::Num(complex_out(dims, out, out_kind(&[&nu, &z]))))
}

fn airy(a: &[Value]) -> R<Value> {
    let (k, zi) = if a.len() >= 2 {
        let k = float_arg(a, 0, "airy")?;
        if k.numel() != 1 || ![0.0, 1.0, 2.0, 3.0].contains(&k.data[0]) {
            return rt("K must be 0, 1, 2, or 3.");
        }
        (k.data[0] as u8, 1)
    } else {
        (0, 0)
    };
    let z = float_arg(a, zi, "airy")?;
    let scaled = scale_arg(a, zi + 1)?;
    let out: Vec<C> = (0..z.numel())
        .map(|i| {
            let w = C::new(z.data[i], z.im_at(i));
            if w.re.is_nan() || w.im.is_nan() {
                return C::new(NAN, 0.0);
            }
            if w.re.is_infinite() && w.im == 0.0 {
                return match (k, w.re > 0.0) {
                    (0 | 1, true) => C::new(0.0, 0.0),
                    (2 | 3, true) if !scaled => C::new(INF, 0.0),
                    _ => C::new(NAN, 0.0),
                };
            }
            let s = if scaled { cb::Scaling::Exponential } else { cb::Scaling::Unscaled };
            let r = match k {
                0 => cb::airy_raw(to_c64(w), s).map(|r| r.value),
                1 => cb::airyprime_raw(to_c64(w), s).map(|r| r.value),
                2 => cb::biry_raw(to_c64(w), s).map(|r| r.value),
                _ => cb::biryprime_raw(to_c64(w), s).map(|r| r.value),
            };
            match r {
                // Real arguments give real values; drop Amos' rounding noise.
                Ok(v) if w.im == 0.0 => C::new(v.re, 0.0),
                Ok(v) => from_c64(v),
                Err(cb::Error::Overflow) => C::new(INF, 0.0),
                Err(_) => C::new(NAN, 0.0),
            }
        })
        .collect();
    Ok(Value::Num(complex_out(z.dims.clone(), out, z.kind)))
}

fn psi(a: &[Value]) -> R<Value> {
    let (k, x) = if a.len() >= 2 {
        let k = m_arg(a, 0, "psi")?;
        if k.numel() != 1 {
            return rt("K must be a scalar.");
        }
        let kv = k.data[0];
        if k.is_complex() || kv < 0.0 || kv.fract() != 0.0 || kv >= 2147483647.0 {
            return rt("Order must be nonnegative integer value smaller than 2^31-1.");
        }
        (kv as u32, float_arg(a, 1, "psi")?)
    } else {
        (0, float_arg(a, 0, "psi")?)
    };
    if x.is_complex() || x.data.iter().any(|&v| v < 0.0) {
        return rt("X must be nonnegative.");
    }
    let data = x.data.iter().map(|&v| if k == 0 { digamma(v) } else { polygamma(k, v) }).collect();
    Ok(Value::Num(real_out(x.dims.clone(), data, x.kind)))
}

fn gammainc_builtin(name: &str, a: &[Value]) -> R<Value> {
    let x = float_arg(a, 0, name)?;
    let s = float_arg(a, 1, name)?;
    let tail = text_opt(a.get(2)).map(|t| t.to_lowercase());
    if x.is_complex() || s.is_complex() {
        return rt("Inputs must be real, full, and double or single.");
    }
    if s.data.iter().any(|&v| v < 0.0 || v.is_nan()) {
        return rt("A must be real and non-negative.");
    }
    let dims = same_size(&[&x, &s], "Input sizes must match.")?;
    let n = nd::numel(&dims);
    let kind = out_kind(&[&x, &s]);
    if name == "gammaincinv" {
        let upper = match tail.as_deref() {
            None | Some("lower") => false,
            Some("upper") => true,
            _ => return rt("TAIL must be 'lower' or 'upper'."),
        };
        if x.data.iter().any(|&v| !(0.0..=1.0).contains(&v)) {
            return rt("Y must be in the interval [0,1].");
        }
        let data = (0..n).map(|i| gammaincinv(at(&x, i), at(&s, i), upper)).collect();
        return Ok(Value::Num(real_out(dims, data, kind)));
    }
    let tail = match tail.as_deref() {
        None | Some("lower") => GTail::Lower,
        Some("upper") => GTail::Upper,
        Some("scaledlower") => GTail::ScaledLower,
        Some("scaledupper") => GTail::ScaledUpper,
        _ => return rt("TAIL must be 'lower', 'upper', 'scaledlower', or 'scaledupper'."),
    };
    let data: Vec<C> = (0..n).map(|i| gammainc(at(&x, i), at(&s, i), tail)).collect();
    Ok(Value::Num(complex_out(dims, data, kind)))
}

fn betainc_builtin(name: &str, a: &[Value]) -> R<Value> {
    let x = float_arg(a, 0, name)?;
    let z = float_arg(a, 1, name)?;
    let w = float_arg(a, 2, name)?;
    let upper = match text_opt(a.get(3)).map(|t| t.to_lowercase()).as_deref() {
        None | Some("lower") => false,
        Some("upper") => true,
        _ => return rt("TAIL must be 'lower' or 'upper'."),
    };
    if x.is_complex() || z.is_complex() || w.is_complex() {
        return rt("Inputs must be real, full, and double or single.");
    }
    let first = if name == "betainc" { "X" } else { "Y" };
    if x.data.iter().any(|&v| !(0.0..=1.0).contains(&v)) {
        return rt(format!("{first} must be in the interval [0,1]."));
    }
    if z.data.iter().any(|&v| v < 0.0 || v.is_nan()) {
        return rt("Z must be real and non-negative.");
    }
    if w.data.iter().any(|&v| v < 0.0 || v.is_nan()) {
        return rt("W must be real and non-negative.");
    }
    let dims = same_size(&[&x, &z, &w], "Input sizes must match.")?;
    let data = (0..nd::numel(&dims))
        .map(|i| {
            let (xv, zv, wv) = (at(&x, i), at(&z, i), at(&w, i));
            if name == "betaincinv" {
                betaincinv(xv, zv, wv, upper)
            } else {
                let (l, u) = betainc_pair(xv, zv, wv);
                if upper { u } else { l }
            }
        })
        .collect();
    Ok(Value::Num(real_out(dims, data, out_kind(&[&x, &z, &w]))))
}

fn ellip_tol(a: &[Value], i: usize, kind: Kind) -> R<f64> {
    match a.get(i) {
        Some(v) => v.scalar(),
        None => Ok(if kind == Kind::Single { f32::EPSILON as f64 } else { EPS }),
    }
}

fn ellipke_builtin(a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let m = float_arg(a, 0, "ellipke")?;
    if m.is_complex() {
        return rt("Input arguments must be real.");
    }
    if m.data.iter().any(|&v| !(0.0..=1.0).contains(&v) && !v.is_nan()) {
        return rt("M must be in the range 0 <= M <= 1.");
    }
    let tol = ellip_tol(a, 1, m.kind)?;
    let (k, e) = ellipke(&m.data, tol);
    let mut out = vec![Value::Num(real_out(m.dims.clone(), k, m.kind))];
    if nargout > 1 {
        out.push(Value::Num(real_out(m.dims.clone(), e, m.kind)));
    }
    Ok(out)
}

fn ellipj_builtin(a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let u = float_arg(a, 0, "ellipj")?;
    let m = float_arg(a, 1, "ellipj")?;
    if u.is_complex() || m.is_complex() {
        return rt("Input arguments must be real.");
    }
    let dims = same_size(&[&u, &m], "U and M must be the same size.")?;
    if m.data.iter().any(|&v| !(0.0..=1.0).contains(&v)) {
        return rt("M must be in the range 0 <= M <= 1.");
    }
    let kind = out_kind(&[&u, &m]);
    let tol = ellip_tol(a, 2, kind)?;
    let n = nd::numel(&dims);
    let us: Vec<f64> = (0..n).map(|i| at(&u, i)).collect();
    let ms: Vec<f64> = (0..n).map(|i| at(&m, i)).collect();
    let r = ellipj(&us, &ms, tol);
    let mut out = vec![Value::Num(real_out(dims.clone(), r.iter().map(|t| t.0).collect(), kind))];
    if nargout > 1 {
        out.push(Value::Num(real_out(dims.clone(), r.iter().map(|t| t.1).collect(), kind)));
    }
    if nargout > 2 {
        out.push(Value::Num(real_out(dims, r.iter().map(|t| t.2).collect(), kind)));
    }
    Ok(out)
}

fn legendre_builtin(a: &[Value]) -> R<Value> {
    let nm = m_arg(a, 0, "legendre")?;
    if nm.numel() != 1 || nm.is_complex() || nm.data[0] < 0.0 || nm.data[0].fract() != 0.0 {
        return rt("N must be a positive scalar integer.");
    }
    let n = nm.data[0] as usize;
    let x = float_arg(a, 1, "legendre")?;
    let norm = match text_opt(a.get(2)) {
        None => LegNorm::Unnorm,
        Some(t) => match t.as_str() {
            "unnorm" => LegNorm::Unnorm,
            "sch" => LegNorm::Sch,
            "norm" => LegNorm::Norm,
            other => return rt(format!("Normalization option {other} not recognized")),
        },
    };
    if x.is_empty() {
        return rt(
            "Operands to the logical AND (&&) and OR (||) operators must be convertible to logical scalar values. Use the ANY or ALL functions to reduce operands to logical scalar values.",
        );
    }
    if x.is_complex() || x.data.iter().any(|&v| v.abs() > 1.0) {
        return rt("X must be real and in the range (-1,1).");
    }
    let mut data = Vec::with_capacity((n + 1) * x.numel());
    for &v in &x.data {
        data.extend(legendre_point(n, v, norm));
    }
    let dims = if x.is_vector() || x.is_scalar() {
        vec![n + 1, x.numel()]
    } else {
        let mut d = vec![n + 1];
        d.extend(&x.dims);
        if n == 0 {
            d.retain(|&k| k != 1);
            while d.len() < 2 {
                d.push(1);
            }
        }
        d
    };
    Ok(Value::Num(real_out(dims, data, x.kind)))
}

fn rat_builtin(a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let x = match a.first() {
        Some(Value::Num(m)) if m.kind.is_float() => m.clone(),
        Some(_) => return rt("First argument must be single or double."),
        None => return rt("Not enough input arguments."),
    };
    let tol = match a.get(1) {
        Some(v) => v.scalar()?,
        None => default_rat_tol(&x),
    };
    if x.is_empty() {
        return Ok(vec![Value::Num(Matrix::empty()); nargout.max(1)]);
    }
    if nargout >= 2 {
        let mut nre = Vec::new();
        let mut nim = Vec::new();
        let mut den = Vec::new();
        for i in 0..x.numel() {
            let (_, n1, d1) = rat_expand(x.data[i], tol);
            if x.is_complex() {
                let (_, n2, d2) = rat_expand(x.im_at(i), tol);
                let l = lcm(d1, d2);
                nre.push(n1 * (l / d1));
                nim.push(n2 * (l / d2));
                den.push(l);
            } else {
                nre.push(n1);
                den.push(d1);
            }
        }
        let n = if x.is_complex() { Matrix::complex(x.dims.clone(), nre, nim) } else { Matrix::from_dims(x.dims.clone(), nre) };
        return Ok(vec![Value::Num(n), Value::Num(Matrix::from_dims(x.dims.clone(), den))]);
    }
    let mut rows = Vec::new();
    for i in 0..x.numel() {
        rows.push(rat_text(&rat_expand(x.data[i], tol).0));
        if x.is_complex() {
            rows.push(" +i* ...".into());
            rows.push(rat_text(&rat_expand(x.im_at(i), tol).0));
        }
    }
    Ok(vec![Value::Num(char_rows(&rows))])
}

fn lcm(a: f64, b: f64) -> f64 {
    if a == 0.0 || b == 0.0 {
        return a.max(b);
    }
    let (mut x, mut y) = (a, b);
    while y != 0.0 {
        let t = x % y;
        x = y;
        y = t;
    }
    a / x * b
}

fn rats_builtin(a: &[Value]) -> R<Value> {
    let x = match a.first() {
        Some(Value::Num(m)) if m.kind.is_float() => m.clone(),
        Some(_) => return rt("First argument must be single or double."),
        None => return rt("Not enough input arguments."),
    };
    let len = match a.get(1) {
        Some(v) => v.scalar()?.max(1.0) as usize,
        None => 13,
    };
    if x.is_empty() {
        return Ok(Value::Num(Matrix::from_dims(vec![0, 0], Vec::new()).with_kind(Kind::Char)));
    }
    // rat's default tolerance scaled to the field width, and never coarser than 0.1.
    let tol = (default_rat_tol(&x) * 1e6 * 10f64.powf(-((len as f64) - 1.0) / 2.0)).min(0.1);
    let field = |v: f64| -> String {
        let (_, n, d) = rat_expand(v, tol);
        rats_field(n, d, len)
    };
    let rows = x.rows();
    let cols = x.numel() / rows.max(1);
    let mut lines = vec![String::new(); rows];
    for c in 0..cols {
        for (r, line) in lines.iter_mut().enumerate() {
            let i = c * rows + r;
            let re = field(x.data[i]);
            if x.is_complex() {
                let im = x.im_at(i);
                let mut f = field(im.abs());
                f.remove(0);
                let end = f.trim_end().len();
                f.insert(end, 'i');
                line.push_str(&re);
                line.push(if im < 0.0 { '-' } else { '+' });
                line.push_str(&f);
            } else {
                line.push_str(&re);
            }
        }
    }
    Ok(Value::Num(char_rows(&lines)))
}

fn perms(a: &[Value]) -> R<Value> {
    let v = a.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?;
    let n = v.numel();
    // All permutations of the positions in reverse lexicographic order.
    let mut rowsv: Vec<Vec<usize>> = Vec::new();
    let mut p: Vec<usize> = (0..n).rev().collect();
    loop {
        rowsv.push(p.clone());
        // Previous permutation in lexicographic order.
        let Some(i) = (1..n).rev().find(|&i| p[i - 1] > p[i]) else { break };
        let j = (i..n).rev().find(|&j| p[j] < p[i - 1]).unwrap();
        p.swap(i - 1, j);
        p[i..].reverse();
    }
    let nr = rowsv.len();
    let order: Vec<usize> = (0..n).flat_map(|c| rowsv.iter().map(move |r| r[c])).collect();
    let dims = if n == 0 { vec![1, 0] } else { vec![nr, n] };
    Ok(match v {
        Value::Num(m) => {
            let mut out = m.gather(&order, dims);
            out.kind = m.kind;
            out
        }
        .into_value(),
        Value::Cell(c) => Value::Cell(CellArray::from_dims(dims, order.iter().map(|&i| c.data[i].clone()).collect())),
        Value::Str(s) => Value::Str(StrArray::from_dims(dims, order.iter().map(|&i| s.data[i].clone()).collect())),
        other => return rt(format!("Undefined function 'perms' for input arguments of type '{}'.", other.class_name())),
    })
}

trait IntoValue {
    fn into_value(self) -> Value;
}

impl IntoValue for Matrix {
    fn into_value(self) -> Value {
        Value::Num(self)
    }
}

fn coords(name: &str, a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let (nin, nout) = match name {
        "cart2pol" | "pol2cart" => (2, 2),
        _ => (3, 3),
    };
    let max_out = if a.len() >= 3 && nin == 2 { 3 } else { nout };
    if nargout > max_out {
        let out = if nargout == 3 { "z" } else { "out" };
        return rt(format!("Output argument \"{out}\" (and possibly others) not assigned a value in the execution with \"{name}\" function."));
    }
    if a.len() < nin {
        return rt("Not enough input arguments.");
    }
    let ms: Vec<Matrix> = (0..a.len().min(3)).map(|i| m_arg(a, i, name)).collect::<R<_>>()?;
    if ms.iter().any(|m| m.is_complex()) {
        return rt("Inputs must be real.");
    }
    let refs: Vec<&Matrix> = ms.iter().take(nin).collect();
    let (dims, idx) = expand(&refs)?;
    let kind = out_kind(&refs);
    let n = nd::numel(&dims);
    let g = |k: usize, i: usize| ms[k].data[idx[k][i]];
    let mut cols: Vec<Vec<f64>> = vec![Vec::with_capacity(n); nout];
    for i in 0..n {
        let r: Vec<f64> = match name {
            "cart2pol" => vec![g(1, i).atan2(g(0, i)), g(0, i).hypot(g(1, i))],
            "pol2cart" => vec![g(1, i) * g(0, i).cos(), g(1, i) * g(0, i).sin()],
            "cart2sph" => {
                let (x, y, z) = (g(0, i), g(1, i), g(2, i));
                let hxy = x.hypot(y);
                vec![y.atan2(x), z.atan2(hxy), hxy.hypot(z)]
            }
            _ => {
                let (az, el, r) = (g(0, i), g(1, i), g(2, i));
                let rc = r * el.cos();
                vec![rc * az.cos(), rc * az.sin(), r * el.sin()]
            }
        };
        for (c, v) in cols.iter_mut().zip(r) {
            c.push(v);
        }
    }
    let mut out: Vec<Value> = cols.into_iter().map(|c| Value::Num(real_out(dims.clone(), c, kind))).collect();
    if nin == 2 && ms.len() == 3 {
        out.push(Value::Num(ms[2].clone()));
    }
    out.truncate(nargout.max(1));
    Ok(out)
}

fn colour(name: &str, a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let to_hsv = name == "rgb2hsv";
    let argname = if to_hsv { "M" } else { "H" };
    // Three separate planes: hsv2rgb(h, s, v) returns them stacked along dimension 3.
    let (m, image) = if a.len() >= 3 {
        let p: Vec<Matrix> = (0..3).map(|i| m_arg(a, i, name)).collect::<R<_>>()?;
        let d = p[0].dims.clone();
        let mut data = Vec::new();
        for q in &p {
            data.extend(q.data.iter().copied());
        }
        let mut dims = vec![d[0], d.get(1).copied().unwrap_or(1), 3];
        dims.truncate(3);
        (Matrix::from_dims(dims, data), true)
    } else {
        let m = m_arg(a, 0, name)?;
        let image = m.dims.len() == 3 && m.dims[2] == 3;
        (m, image)
    };
    let allowed: &[Kind] = if image {
        if to_hsv { &[Kind::Double, Kind::Single, Kind::Int(IntClass::U8), Kind::Int(IntClass::U16)] } else { &[Kind::Double, Kind::Single, Kind::Logical] }
    } else if to_hsv {
        &[Kind::Double]
    } else {
        &[Kind::Double, Kind::Single, Kind::Logical]
    };
    if !allowed.contains(&m.kind) {
        let list = if !image && to_hsv { "double" } else if to_hsv { "double, single, uint8, uint16" } else { "double, single, logical" };
        return rt(format!("Expected {argname} to be one of these types:\n\n{list}\n\nInstead its type was {}.", m.kind.name()));
    }
    let scale = match m.kind {
        Kind::Int(IntClass::U8) => 255.0,
        Kind::Int(IntClass::U16) => 65535.0,
        _ => 1.0,
    };
    let rows = if image {
        m.dims[0] * m.dims[1]
    } else {
        if to_hsv && m.is_empty() {
            return rt("Expected M to be nonempty.");
        }
        if m.dims.len() != 2 || m.dims[1] != 3 || m.is_empty() {
            return rt("MAP must be a Mx3 array.");
        }
        if m.data.iter().any(|&v| !(0.0..=1.0).contains(&v)) {
            return rt("Valid colormaps cannot have values outside the range [0,1].");
        }
        m.dims[0]
    };
    let mut out = vec![0.0; rows * 3];
    for r in 0..rows {
        let c = [m.data[r] / scale, m.data[rows + r] / scale, m.data[2 * rows + r] / scale];
        let t = if to_hsv { rgb_to_hsv(c[0], c[1], c[2]) } else { hsv_to_rgb(c[0], c[1], c[2]) };
        for k in 0..3 {
            out[k * rows + r] = t[k];
        }
    }
    let kind = if m.kind == Kind::Single { Kind::Single } else { Kind::Double };
    if nargout >= 2 {
        let pdims = if image { vec![m.dims[0], m.dims[1]] } else { vec![rows, 1] };
        return Ok((0..3).map(|k| Value::Num(real_out(pdims.clone(), out[k * rows..(k + 1) * rows].to_vec(), kind))).collect());
    }
    Ok(vec![Value::Num(real_out(m.dims.clone(), out, kind))])
}
