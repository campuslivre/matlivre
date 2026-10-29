//! MATLAB's data analysis functions (`toolbox/matlab/datafun`): Fourier transforms, filtering
//! and convolution, covariance and correlation, moving-window statistics, percentiles, numerical
//! gradients, missing-data and outlier handling, smoothing, normalization and grouping.
//!
//! The FFTs use `rustfft` (any length, as FFTW); everything else is computed directly from the
//! definitions in MATLAB's documentation.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::f64::consts::PI;

use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

use crate::builtins::{arg, default_dim, one, Ret};
use crate::cplx::C;
use crate::error::{rt, LangError};
use crate::indexing;
use crate::interp::Interpreter;
use crate::nd::{self, Dims, IndexArg};
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "bounds", "prctile", "quantile", "iqr", "issortedrows", "maxk", "mink", "topkrows", "movsum", "movprod", "movmin", "movmax", "movmean",
    "movmedian", "movstd", "movvar", "movmad", "gradient", "del2", "corrcoef", "cov", "xcorr", "xcov", "filter", "filter2", "conv2", "convn",
    "detrend", "fft", "fft2", "fftn", "ifft", "ifft2", "ifftn", "fftshift", "ifftshift", "fftw", "nufft", "nufftn", "standardizeMissing",
    "rmmissing", "fillmissing", "fillmissing2", "ischange", "islocalmax", "islocalmin", "isoutlier", "isuniform", "filloutliers", "rmoutliers",
    "smoothdata", "smoothdata2", "trenddecomp", "rescale", "normalize", "discretize", "findgroups", "groupcounts", "groupfilter", "groupsummary",
    "grouptransform", "histcounts2", "pivot", "splitapply", "mape", "rmse", "histc",
];

// ---- shared helpers --------------------------------------------------------------------------

/// Lower-case text of a char row vector or string scalar.
fn lower(v: &Value) -> Option<String> {
    if v.is_text() { v.text().ok().map(|t| t.to_lowercase()) } else { None }
}

fn is_flag(v: &Value, flags: &[&str]) -> bool {
    lower(v).map(|t| flags.contains(&t.as_str())).unwrap_or(false)
}

fn c_at(m: &Matrix, i: usize) -> C {
    C::new(m.data[i], m.im_at(i))
}

/// A numeric argument, refusing other classes with MATLAB's wording for `fname`.
fn num_arg(a: &[Value], i: usize, fname: &str) -> R<Matrix> {
    match arg(a, i, fname)? {
        Value::Num(m) => Ok(m.clone()),
        Value::Sym(s) => s.to_matrix(),
        other => rt(format!("Undefined function '{fname}' for input arguments of type '{}'.", other.class_name())),
    }
}

fn logical(dims: Dims, data: Vec<f64>) -> Matrix {
    Matrix::from_dims(dims, data).with_kind(Kind::Logical)
}

fn bool_val(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

/// Double or single result class for floating-point computations on `k`.
fn float_kind(k: Kind) -> Kind {
    if k == Kind::Single { Kind::Single } else { Kind::Double }
}

/// Real or complex matrix from parts, keeping a (possibly all-zero) imaginary part when `cplx`.
fn from_parts(dims: Dims, re: Vec<f64>, im: Option<Vec<f64>>) -> Matrix {
    let mut m = Matrix::from_dims(dims, re);
    m.im = im;
    m
}

fn from_c(dims: Dims, z: &[C], keep_complex: bool) -> Matrix {
    let re = z.iter().map(|c| c.re).collect();
    let im: Vec<f64> = z.iter().map(|c| c.im).collect();
    let mut m = from_parts(dims, re, Some(im));
    if !keep_complex {
        m.narrow();
    }
    m
}

/// (stride, length, outer count) of the layout along 1-based `dim`.
fn layout(d: &[usize], dim: usize) -> (usize, usize, usize) {
    let k = dim - 1;
    let stride: usize = d[..k.min(d.len())].iter().product();
    let len = d.get(k).copied().unwrap_or(1);
    let outer: usize = if k + 1 < d.len() { d[k + 1..].iter().product() } else { 1 };
    (stride, len, outer)
}

/// Dims of `d` with dimension `dim` set to `n`.
fn with_dim(d: &[usize], dim: usize, n: usize) -> Dims {
    let mut out = d.to_vec();
    while out.len() < dim {
        out.push(1);
    }
    out[dim - 1] = n;
    nd::normalize(out)
}

/// Maps every vector along `dim` to a vector of length `out_len` (real data).
fn map_dim(m: &Matrix, dim: usize, out_len: usize, mut f: impl FnMut(&[f64], usize) -> Vec<f64>) -> Matrix {
    let (stride, len, outer) = layout(&m.dims, dim);
    let dims = with_dim(&m.dims, dim, out_len);
    let mut out = vec![0.0; stride * out_len * outer];
    let mut buf = vec![0.0; len];
    for o in 0..outer {
        for i in 0..stride {
            for (j, b) in buf.iter_mut().enumerate() {
                *b = m.data[o * stride * len + j * stride + i];
            }
            let r = f(&buf, o * stride + i);
            for (j, v) in r.into_iter().enumerate().take(out_len) {
                out[o * stride * out_len + j * stride + i] = v;
            }
        }
    }
    Matrix::from_dims(dims, out)
}

/// Complex version of [`map_dim`]; the result keeps an imaginary part.
fn map_dim_c(m: &Matrix, dim: usize, out_len: usize, mut f: impl FnMut(&[C]) -> Vec<C>) -> Matrix {
    let (stride, len, outer) = layout(&m.dims, dim);
    let dims = with_dim(&m.dims, dim, out_len);
    let n = stride * out_len * outer;
    let (mut re, mut im) = (vec![0.0; n], vec![0.0; n]);
    let mut buf = vec![C::new(0.0, 0.0); len];
    for o in 0..outer {
        for i in 0..stride {
            for (j, b) in buf.iter_mut().enumerate() {
                *b = c_at(m, o * stride * len + j * stride + i);
            }
            let r = f(&buf);
            for (j, v) in r.into_iter().enumerate().take(out_len) {
                let p = o * stride * out_len + j * stride + i;
                re[p] = v.re;
                im[p] = v.im;
            }
        }
    }
    from_parts(dims, re, Some(im))
}

/// The working dimension from argument `i`: a positive integer, or the default.
fn dim_at(a: &[Value], i: usize, d: &[usize]) -> R<usize> {
    match a.get(i) {
        Some(Value::Num(m)) if !m.is_empty() && m.kind != Kind::Char => {
            let v = m.data[0];
            if m.numel() != 1 || v < 1.0 || v.fract() != 0.0 || !v.is_finite() {
                return rt("Dimension argument must be a positive integer scalar within indexing range.");
            }
            Ok(v as usize)
        }
        _ => Ok(default_dim(d)),
    }
}

/// Name-value arguments and flags after the positional ones. Names match case-insensitively
/// and by unique prefix, as MATLAB's input parsing does.
struct Opts {
    pos: Vec<Value>,
    flags: Vec<String>,
    nv: Vec<(String, Value)>,
}

impl Opts {
    fn parse(a: &[Value], flags: &[&str], names: &[&str]) -> R<Opts> {
        let mut o = Opts { pos: Vec::new(), flags: Vec::new(), nv: Vec::new() };
        let mut i = 0;
        while i < a.len() {
            if let Some(t) = lower(&a[i]) {
                if let Some(f) = flags.iter().find(|f| **f == t) {
                    o.flags.push(f.to_string());
                    i += 1;
                    continue;
                }
                let hits: Vec<&&str> = names.iter().filter(|n| n.to_lowercase().starts_with(&t)).collect();
                let exact = names.iter().find(|n| n.to_lowercase() == t);
                if let Some(n) = exact.or(if hits.len() == 1 && !t.is_empty() && t.len() >= 3 { Some(hits[0]) } else { None }) {
                    let Some(v) = a.get(i + 1) else { return rt(format!("Expected a value after the name '{n}'.")) };
                    o.nv.push((n.to_string(), v.clone()));
                    i += 2;
                    continue;
                }
            }
            if !o.flags.is_empty() || !o.nv.is_empty() {
                // A positional argument after the options is a misplaced or unknown option.
                if let Some(t) = a[i].text().ok().filter(|_| a[i].is_text()) {
                    return rt(format!("Invalid option '{t}'."));
                }
            }
            o.pos.push(a[i].clone());
            i += 1;
        }
        Ok(o)
    }

    fn get(&self, name: &str) -> Option<&Value> {
        self.nv.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }

    /// The last of `options` given as a flag.
    fn last_of(&self, options: &[&str]) -> Option<&str> {
        self.flags.iter().rev().find(|f| options.contains(&f.as_str())).map(|s| s.as_str())
    }
}

/// `omitnan` (or `omitmissing`) among the flags.
fn omit_nan(o: &Opts, default: bool) -> bool {
    match o.last_of(&["omitnan", "omitmissing", "includenan", "includemissing"]) {
        Some("omitnan" | "omitmissing") => true,
        Some(_) => false,
        None => default,
    }
}

const NANFLAGS: &[&str] = &["omitnan", "omitmissing", "includenan", "includemissing"];

fn cmp_nan_last(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => a.partial_cmp(&b).unwrap(),
    }
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    if v.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { mid(v[n / 2 - 1], v[n / 2]) }
}

/// The midpoint of two numbers as MATLAB's median computes it (no overflow, exact for equal
/// values).
fn mid(a: f64, b: f64) -> f64 {
    if a == b { a } else if (a < 0.0) != (b < 0.0) || !(a + b).is_finite() { a / 2.0 + b / 2.0 } else { a + (b - a) / 2.0 }
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

/// Variance with normalization weight 0 (n-1) or 1 (n).
fn var(v: &[f64], w: f64) -> f64 {
    let n = v.len();
    if n == 0 {
        return f64::NAN;
    }
    if n == 1 {
        return if v[0].is_finite() { 0.0 } else { f64::NAN };
    }
    let mu = mean(v);
    let s: f64 = v.iter().map(|x| (x - mu) * (x - mu)).sum();
    s / if w == 1.0 { n as f64 } else { (n - 1) as f64 }
}

fn row_or_col(len: usize, row: bool, data: Vec<f64>) -> Matrix {
    if row { Matrix::new(1, len, data) } else { Matrix::new(len, 1, data) }
}

// ---- special functions used by statistics ------------------------------------------------------

fn erfc_inv(y: f64) -> f64 {
    if y <= 0.0 {
        return f64::INFINITY;
    }
    if y >= 2.0 {
        return f64::NEG_INFINITY;
    }
    // Initial rational approximation, then Newton steps on erfc(x) - y.
    let p = if y < 1.0 { y } else { 2.0 - y };
    let t = (-2.0 * (p / 2.0).ln()).sqrt();
    let mut x = -0.70711 * ((2.30753 + t * 0.27061) / (1.0 + t * (0.99229 + t * 0.04481)) - t);
    for _ in 0..3 {
        let err = erfc(x) - p;
        x += err / (1.12837916709551257 * (-x * x).exp() - x * err);
    }
    if y < 1.0 { x } else { -x }
}

fn erfc(x: f64) -> f64 {
    // W. J. Cody's rational Chebyshev approximations.
    let ax = x.abs();
    let r = if ax < 0.5 {
        return 1.0 - erf_small(x);
    } else if ax < 4.0 {
        const P: [f64; 9] = [
            5.64188496988670089e-1, 8.88314979438837594, 6.61191906371416295e1, 2.98635138197400131e2, 8.81952221241769090e2, 1.71204761263407058e3,
            2.05107837782607147e3, 1.23033935479799725e3, 2.15311535474403846e-8,
        ];
        const Q: [f64; 8] = [
            1.57449261107098347e1, 1.17693950891312499e2, 5.37181101862009858e2, 1.62138957456669019e3, 3.29079923573345963e3, 4.36261909014324716e3,
            3.43936767414372164e3, 1.23033935480374942e3,
        ];
        let mut num = P[8] * ax;
        let mut den = ax;
        for i in 0..7 {
            num = (num + P[i]) * ax;
            den = (den + Q[i]) * ax;
        }
        let res = (num + P[7]) / (den + Q[7]);
        let z = (ax * 16.0).trunc() / 16.0;
        let del = (ax - z) * (ax + z);
        (-z * z).exp() * (-del).exp() * res
    } else {
        const P: [f64; 6] = [3.05326634961232344e-1, 3.60344899949804439e-1, 1.25781726111229246e-1, 1.60837851487422766e-2, 6.58749161529837803e-4, 1.63153871373020978e-2];
        const Q: [f64; 5] = [2.56852019228982242, 1.87295284992346725, 5.27905102951428412e-1, 6.05183413124413191e-2, 2.33520497626869185e-3];
        if ax > 26.6 {
            0.0
        } else {
            let z2 = 1.0 / (ax * ax);
            let mut num = P[5] * z2;
            let mut den = z2;
            for i in 0..4 {
                num = (num + P[i]) * z2;
                den = (den + Q[i]) * z2;
            }
            let res = z2 * (num + P[4]) / (den + Q[4]);
            let res = (0.56418958354775628695 - res) / ax;
            let z = (ax * 16.0).trunc() / 16.0;
            let del = (ax - z) * (ax + z);
            (-z * z).exp() * (-del).exp() * res
        }
    };
    if x < 0.0 { 2.0 - r } else { r }
}

fn erf_small(x: f64) -> f64 {
    const A: [f64; 5] = [3.16112374387056560, 1.13864154151050156e2, 3.77485237685302021e2, 3.20937758913846947e3, 1.85777706184603153e-1];
    const B: [f64; 4] = [2.36012909523441209e1, 2.44024637934444173e2, 1.28261652607737228e3, 2.84423683343917062e3];
    let y = x * x;
    let mut num = A[4] * y;
    let mut den = y;
    for i in 0..3 {
        num = (num + A[i]) * y;
        den = (den + B[i]) * y;
    }
    x * (num + A[3]) / (den + B[3])
}

/// Normal quantile.
fn norm_inv(p: f64) -> f64 {
    -std::f64::consts::SQRT_2 * erfc_inv(2.0 * p)
}

fn ln_gamma(x: f64) -> f64 {
    // Lanczos approximation (g = 7, n = 9).
    const G: [f64; 9] = [
        0.99999999999980993, 676.5203681218851, -1259.1392167224028, 771.32342877765313, -176.61502916214059, 12.507343278686905,
        -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7,
    ];
    if x < 0.5 {
        return (PI / (PI * x).sin()).abs().ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut s = G[0];
    for (i, g) in G.iter().enumerate().skip(1) {
        s += g / (x + i as f64);
    }
    let t = x + 7.5;
    0.5 * (2.0 * PI).ln() + (x + 0.5) * t.ln() - t + s.ln()
}

/// Continued fraction for the regularized incomplete beta function.
fn beta_cf(a: f64, b: f64, x: f64) -> f64 {
    let tiny = 1e-300;
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..1000 {
        let m = m as f64;
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
        if (del - 1.0).abs() < 1e-16 {
            break;
        }
    }
    h
}

/// Regularized incomplete beta I_x(a, b).
fn betainc(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bt = (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) { bt * beta_cf(a, b, x) / a } else { 1.0 - bt * beta_cf(b, a, 1.0 - x) / b }
}

/// Student's t upper-tail probability P(T > t) for t >= 0.
fn t_upper(t: f64, v: f64) -> f64 {
    if t.is_nan() || v.is_nan() {
        return f64::NAN;
    }
    if t.is_infinite() {
        return if t > 0.0 { 0.0 } else { 1.0 };
    }
    let p = 0.5 * betainc(v / (v + t * t), v / 2.0, 0.5);
    if t >= 0.0 { p } else { 1.0 - p }
}

/// Student's t quantile.
fn t_inv(p: f64, v: f64) -> f64 {
    if !(0.0..=1.0).contains(&p) || v.is_nan() || v <= 0.0 {
        return f64::NAN;
    }
    if p == 0.5 {
        return 0.0;
    }
    if p < 0.5 {
        return -t_inv(1.0 - p, v);
    }
    let q = 1.0 - p;
    // Bisection in the probability, then Newton polishing.
    let (mut lo, mut hi) = (0.0, 1.0);
    while t_upper(hi, v) > q {
        hi *= 2.0;
        if hi > 1e300 {
            return f64::INFINITY;
        }
    }
    for _ in 0..200 {
        let m = 0.5 * (lo + hi);
        if t_upper(m, v) > q {
            lo = m;
        } else {
            hi = m;
        }
        if hi - lo <= 4.0 * f64::EPSILON * hi {
            break;
        }
    }
    let mut x = 0.5 * (lo + hi);
    for _ in 0..3 {
        let pdf = (ln_gamma((v + 1.0) / 2.0) - ln_gamma(v / 2.0) - 0.5 * (v * PI).ln() - (v + 1.0) / 2.0 * (1.0 + x * x / v).ln()).exp();
        if pdf <= 0.0 {
            break;
        }
        let step = (t_upper(x, v) - q) / pdf;
        if !step.is_finite() {
            break;
        }
        x += step;
    }
    x
}

// ---- Fourier transforms --------------------------------------------------------------------------

thread_local! {
    static PLANNER: RefCell<FftPlanner<f64>> = RefCell::new(FftPlanner::new());
    static FFTW_PLANNER: RefCell<String> = RefCell::new("estimate".into());
    static FFTW_WISDOM: RefCell<(String, String)> = const { RefCell::new((String::new(), String::new())) };
}

/// In-place complex FFT of one vector (unscaled in both directions).
fn fft_vec(buf: &mut [Complex64], inverse: bool) {
    if buf.len() <= 1 {
        return;
    }
    PLANNER.with(|p| {
        let mut p = p.borrow_mut();
        let plan = if inverse { p.plan_fft_inverse(buf.len()) } else { p.plan_fft_forward(buf.len()) };
        plan.process(buf);
    });
}

/// Checks the classes FFTs accept and returns the result class.
fn fft_input(x: &Matrix) -> R<Kind> {
    match x.kind {
        Kind::Single => Ok(Kind::Single),
        Kind::Double | Kind::Logical | Kind::Int(IntClass::I8 | IntClass::U8 | IntClass::I16 | IntClass::U16 | IntClass::I32 | IntClass::U32) => Ok(Kind::Double),
        _ => rt("Invalid data type. First argument must be double, single, int8, uint8, int16, uint16, int32, uint32, or logical."),
    }
}

/// Transform length argument: empty means the current length.
fn fft_len(v: Option<&Value>, current: usize) -> R<usize> {
    match v {
        None => Ok(current),
        Some(Value::Num(m)) if m.is_empty() => Ok(current),
        Some(v) => {
            let n = v.scalar()?;
            if n < 0.0 || n.fract() != 0.0 || !n.is_finite() || v.numel() != 1 {
                return rt("FFT length must be a nonnegative integer scalar.");
            }
            Ok(n as usize)
        }
    }
}

/// FFT along `dim` with length `n` (padding with zeros or truncating). With `symmetric` (an
/// inverse only), the input is treated as conjugate symmetric and the result is real.
fn fft_dim(x: &Matrix, n: usize, dim: usize, inverse: bool, symmetric: bool) -> Matrix {
    let scale = if inverse && n > 0 { 1.0 / n as f64 } else { 1.0 };
    let mut buf = vec![Complex64::new(0.0, 0.0); n];
    let mut out = map_dim_c(x, dim, n, |v| {
        for (k, b) in buf.iter_mut().enumerate() {
            *b = v.get(k).map(|c| Complex64::new(c.re, c.im)).unwrap_or_default();
        }
        if symmetric && n > 0 {
            // Only the first half counts: the rest is the conjugate mirror image.
            buf[0].im = 0.0;
            for k in n / 2 + 1..n {
                buf[k] = buf[n - k].conj();
            }
            if n % 2 == 0 {
                buf[n / 2].im = 0.0;
            }
        }
        fft_vec(&mut buf, inverse);
        buf.iter().map(|c| C::new(c.re * scale, c.im * scale)).collect()
    });
    if symmetric {
        out.im = None;
    }
    out
}

/// Whether every vector along `dim` is exactly conjugate symmetric.
fn conj_symmetric(x: &Matrix, dim: usize) -> bool {
    if !x.is_complex() {
        // A real input is symmetric only in the trivial cases ifft handles as real anyway.
        let (stride, len, outer) = layout(&x.dims, dim);
        for o in 0..outer {
            for i in 0..stride {
                let at = |j: usize| x.data[o * stride * len + j * stride + i];
                for k in 1..len {
                    if at(k) != at(len - k) {
                        return false;
                    }
                }
            }
        }
        return true;
    }
    let (stride, len, outer) = layout(&x.dims, dim);
    for o in 0..outer {
        for i in 0..stride {
            let at = |j: usize| c_at(x, o * stride * len + j * stride + i);
            if len > 0 && at(0).im != 0.0 {
                return false;
            }
            for k in 1..len {
                let (p, q) = (at(k), at(len - k));
                if p.re != q.re || p.im != -q.im {
                    return false;
                }
            }
        }
    }
    true
}

fn finish_fft(mut m: Matrix, kind: Kind, real: bool) -> Matrix {
    if real {
        m.im = None;
    }
    m.narrow();
    if kind == Kind::Single {
        m = m.cast_to(Kind::Single);
    }
    m
}

fn symmetric_flag(a: &[Value]) -> R<(Vec<Value>, Option<bool>)> {
    let mut rest = Vec::new();
    let mut sym = None;
    for v in a {
        match lower(v).as_deref() {
            Some("symmetric") => sym = Some(true),
            Some("nonsymmetric") => sym = Some(false),
            Some(t) => return rt(format!("Invalid option '{t}'.")),
            None => rest.push(v.clone()),
        }
    }
    Ok((rest, sym))
}

fn fft1(name: &str, a: &[Value]) -> R<Matrix> {
    let inverse = name == "ifft";
    let (a, sym) = symmetric_flag(a)?;
    let x = num_arg(&a, 0, name)?;
    let kind = fft_input(&x)?;
    let dim = match a.get(2) {
        Some(v) => {
            let d = v.scalar()?;
            if d < 1.0 || d.fract() != 0.0 {
                return rt("Dimension argument must be a positive integer scalar within indexing range.");
            }
            d as usize
        }
        None => default_dim(&x.dims),
    };
    let cur = x.dims.get(dim - 1).copied().unwrap_or(1);
    let n = fft_len(a.get(1), cur)?;
    if x.dims == [0, 0] && a.get(1).map(|v| v.is_empty_num()).unwrap_or(true) && a.get(2).is_none() {
        return Ok(Matrix::empty().cast_to(kind));
    }
    let symmetric = inverse && sym == Some(true);
    let real = inverse && (symmetric || (sym.is_none() && n == cur && conj_symmetric(&x, dim)));
    Ok(finish_fft(fft_dim(&x, n, dim, inverse, symmetric), kind, real))
}

trait EmptyNum {
    fn is_empty_num(&self) -> bool;
}

impl EmptyNum for Value {
    fn is_empty_num(&self) -> bool {
        matches!(self, Value::Num(m) if m.is_empty())
    }
}

/// fftn / ifftn over the dimensions listed with their lengths.
fn fft_multi(x: &Matrix, lens: &[(usize, usize)], inverse: bool, sym: Option<bool>) -> Matrix {
    let symmetric = inverse && sym == Some(true);
    let real = inverse && (symmetric || (sym.is_none() && lens.iter().all(|&(d, n)| n == x.dims.get(d - 1).copied().unwrap_or(1)) && conj_symmetric_nd(x)));
    let mut m = x.clone();
    // The symmetric (real-output) transform treats the last non-singleton dimension as the
    // halved one, so it runs last.
    let mut order: Vec<(usize, usize)> = lens.to_vec();
    if symmetric {
        if let Some(k) = order.iter().rposition(|&(_, n)| n > 1) {
            let d = order.remove(k);
            order.push(d);
        }
    }
    for (k, &(d, n)) in order.iter().enumerate() {
        let last = k + 1 == order.len();
        m = fft_dim(&m, n, d, inverse, symmetric && last);
    }
    if real {
        m.im = None;
    } else if m.im.is_none() {
        m.im = Some(vec![0.0; m.numel()]);
    }
    m
}

/// N-D conjugate symmetry: X(k) == conj(X(-k mod size)) for every index.
fn conj_symmetric_nd(x: &Matrix) -> bool {
    let d = &x.dims;
    let n = x.numel();
    let mut sub = vec![0usize; d.len()];
    for i in 0..n {
        // Index of the mirrored element.
        let mut j = 0;
        let mut mul = 1;
        for (k, &s) in sub.iter().enumerate() {
            j += ((d[k] - s) % d[k]) * mul;
            mul *= d[k];
        }
        if x.data[i] != x.data[j] || x.im_at(i) != -x.im_at(j) {
            return false;
        }
        for (k, s) in sub.iter_mut().enumerate() {
            *s += 1;
            if *s < d[k] {
                break;
            }
            *s = 0;
        }
    }
    true
}

fn fft_nd(name: &str, a: &[Value]) -> R<Matrix> {
    let inverse = name.starts_with('i');
    let (a, sym) = symmetric_flag(a)?;
    let x = num_arg(&a, 0, name)?;
    let kind = fft_input(&x)?;
    let lens: Vec<(usize, usize)> = if name.ends_with('2') {
        let m = fft_len(a.get(1), x.dims[0])?;
        let n = fft_len(a.get(2), x.dims[1])?;
        if a.len() == 2 {
            return rt(format!("Not enough input arguments for '{name}'."));
        }
        vec![(1, m), (2, n)]
    } else {
        match a.get(1) {
            Some(v) if !v.is_empty_num() => {
                let s = v.to_matrix()?;
                if s.data.iter().any(|&q| q < 0.0 || q.fract() != 0.0) {
                    return rt("Transform lengths must be a vector of nonnegative integers.");
                }
                if s.numel() < x.dims.len() {
                    return rt("Transform lengths must be a vector of nonnegative integers with at least ndims(X) elements.");
                }
                s.data.iter().enumerate().map(|(k, &q)| (k + 1, q as usize)).collect()
            }
            _ => x.dims.iter().enumerate().map(|(k, &q)| (k + 1, q)).collect(),
        }
    };
    if x.is_empty() && lens.iter().all(|&(d, n)| n == x.dims.get(d - 1).copied().unwrap_or(1)) {
        return Ok(Matrix::from_dims(x.dims.clone(), vec![]).cast_to(kind));
    }
    Ok(finish_fft(fft_multi(&x, &lens, inverse, sym), kind, false))
}

/// fftshift / ifftshift: circular shift by half the length along the chosen dimensions.
fn fftshift(name: &str, a: &[Value]) -> Ret {
    let v = arg(a, 0, name)?;
    let d = v.dims();
    let dims: Vec<usize> = match a.get(1) {
        Some(x) => {
            let k = x.scalar()?;
            if k < 1.0 || k.fract() != 0.0 {
                return rt("Dimension argument must be a positive integer scalar within indexing range.");
            }
            vec![k as usize]
        }
        None => (1..=d.len()).collect(),
    };
    let nd_count = d.len().max(dims.iter().copied().max().unwrap_or(0));
    let idx: Vec<IndexArg> = (0..nd_count)
        .map(|k| {
            let n = d.get(k).copied().unwrap_or(1);
            if !dims.contains(&(k + 1)) || n == 0 {
                return IndexArg::All;
            }
            // Output position j takes input position (j - shift) mod n.
            let shift = if name == "fftshift" { n / 2 } else { n - n / 2 };
            let ix: Vec<usize> = (0..n).map(|j| (j + n - shift) % n).collect();
            IndexArg::List { dims: vec![1, n], ix, from_mask: false }
        })
        .collect();
    one(indexing::index(v, &idx)?)
}

fn fftw(a: &[Value]) -> Ret {
    let what = arg(a, 0, "fftw")?.text()?.to_lowercase();
    match what.as_str() {
        "planner" => {
            let old = FFTW_PLANNER.with(|p| p.borrow().clone());
            if let Some(m) = a.get(1) {
                let m = m.text()?.to_lowercase();
                if !["estimate", "measure", "patient", "exhaustive", "hybrid"].contains(&m.as_str()) {
                    return rt("Invalid planner method. Valid methods are 'estimate', 'measure', 'patient', 'exhaustive', and 'hybrid'.");
                }
                FFTW_PLANNER.with(|p| *p.borrow_mut() = m);
                return Ok(Some(vec![Value::chars(&old)]));
            }
            one(Value::chars(&old))
        }
        "dwisdom" | "swisdom" => {
            let single = what == "swisdom";
            let get = || FFTW_WISDOM.with(|w| if single { w.borrow().1.clone() } else { w.borrow().0.clone() });
            if let Some(s) = a.get(1) {
                let s = if s.is_empty_num() { String::new() } else { s.text()? };
                let old = get();
                FFTW_WISDOM.with(|w| if single { w.borrow_mut().1 = s } else { w.borrow_mut().0 = s });
                return Ok(Some(vec![Value::chars(&old)]));
            }
            one(Value::chars(&get()))
        }
        _ => rt("First input must be 'planner', 'dwisdom', or 'swisdom'."),
    }
}

/// exp(2 pi i v) for v in turns, exact at multiples of a quarter turn.
fn cis_turns(v: f64) -> C {
    let r = v - v.round();
    let q = r * 4.0;
    if q.fract() == 0.0 {
        return match q as i64 {
            0 => C::new(1.0, 0.0),
            1 => C::new(0.0, 1.0),
            -1 => C::new(0.0, -1.0),
            _ => C::new(-1.0, 0.0),
        };
    }
    let a = 2.0 * PI * r;
    C::new(a.cos(), a.sin())
}

/// nufft(x, t, f, dim): sum_j x(j) exp(-2 pi i t(j) f(k)).
fn nufft(a: &[Value]) -> R<Matrix> {
    let x = num_arg(a, 0, "nufft")?;
    let kind = fft_input(&x)?;
    let dim = match a.get(3) {
        Some(v) => v.as_usize()?.max(1),
        None => default_dim(&x.dims),
    };
    let len = x.dims.get(dim - 1).copied().unwrap_or(1);
    let t: Vec<f64> = match a.get(1) {
        Some(v) if !v.is_empty_num() => v.to_matrix()?.data,
        _ => (0..len).map(|j| j as f64).collect(),
    };
    if t.len() != len {
        return rt("Sample points must be a vector with the same length as the operating dimension of the input.");
    }
    let f: Vec<f64> = match a.get(2) {
        Some(v) if !v.is_empty_num() => v.to_matrix()?.data,
        _ => (0..len).map(|k| k as f64 / len as f64).collect(),
    };
    let out = map_dim_c(&x, dim, f.len(), |v| {
        f.iter()
            .map(|&fk| {
                v.iter().zip(&t).fold(C::new(0.0, 0.0), |acc, (xj, &tj)| {
                    let w = cis_turns(-tj * fk);
                    acc.add(xj.mul(w))
                })
            })
            .collect()
    });
    Ok(finish_fft(out, kind, false))
}

/// nufftn(X, t, f): t and f are matrices of points (one column per dimension) or cell arrays of
/// grid vectors.
fn nufftn(a: &[Value]) -> R<Matrix> {
    let x = num_arg(a, 0, "nufftn")?;
    let kind = fft_input(&x)?;
    let grid_dims: Dims = x.dims.clone();
    // Points of a grid given as one vector per dimension.
    fn grid(vs: &[Vec<f64>]) -> Vec<Vec<f64>> {
        let total: usize = vs.iter().map(|v| v.len()).product();
        let mut pts = Vec::with_capacity(total);
        let mut sub = vec![0usize; vs.len()];
        for _ in 0..total {
            pts.push(sub.iter().enumerate().map(|(k, &s)| vs[k][s]).collect());
            for (k, s) in sub.iter_mut().enumerate() {
                *s += 1;
                if *s < vs[k].len() {
                    break;
                }
                *s = 0;
            }
        }
        pts
    }
    let points = |v: Option<&Value>, default: &dyn Fn() -> Vec<Vec<f64>>| -> R<Vec<Vec<f64>>> {
        match v {
            None => Ok(default()),
            Some(v) if v.is_empty_num() => Ok(default()),
            Some(Value::Cell(c)) => {
                let vs: Vec<Vec<f64>> = c.data.iter().map(|e| e.to_matrix().map(|m| m.data)).collect::<R<_>>()?;
                Ok(grid(&vs))
            }
            Some(v) => {
                let m = v.to_matrix()?;
                let (r, c) = (m.rows(), m.cols());
                Ok((0..r).map(|i| (0..c).map(|k| m.data[k * r + i]).collect()).collect())
            }
        }
    };
    let t = points(a.get(1), &|| grid(&grid_dims.iter().map(|&n| (0..n).map(|j| j as f64).collect()).collect::<Vec<_>>()))?;
    let f = points(a.get(2), &|| grid(&grid_dims.iter().map(|&n| (0..n).map(|k| k as f64 / n as f64).collect()).collect::<Vec<_>>()))?;
    if t.len() != x.numel() {
        return rt("The total number of sample points must match the number of elements in the first input argument.");
    }
    let mut re = Vec::with_capacity(f.len());
    let mut im = Vec::with_capacity(f.len());
    for fk in &f {
        let mut acc = C::new(0.0, 0.0);
        for (j, tj) in t.iter().enumerate() {
            let dot: f64 = tj.iter().zip(fk).map(|(p, q)| p * q).sum();
            let w = cis_turns(-dot);
            acc = acc.add(c_at(&x, j).mul(w));
        }
        re.push(acc.re);
        im.push(acc.im);
    }
    // Default frequencies keep the input's shape; given ones produce a column.
    let dims = if a.get(2).map(|v| v.is_empty_num()).unwrap_or(true) { x.dims.clone() } else { vec![re.len(), 1] };
    Ok(finish_fft(from_parts(dims, re, Some(im)), kind, false))
}

// ---- filtering and convolution ---------------------------------------------------------------

/// Result class of filter/conv: single if any input is single, otherwise double.
fn single_any(ms: &[&Matrix]) -> Kind {
    if ms.iter().any(|m| m.kind == Kind::Single) { Kind::Single } else { Kind::Double }
}

fn filter(a: &[Value], nargout: usize) -> Ret {
    let b = num_arg(a, 0, "filter")?;
    let den = num_arg(a, 1, "filter")?;
    let x = num_arg(a, 2, "filter")?;
    if b.is_empty() || den.is_empty() || !(b.is_vector() || b.is_scalar()) || !(den.is_vector() || den.is_scalar()) {
        return rt("Filter coefficients must be non-empty vectors.");
    }
    let a0 = c_at(&den, 0);
    if a0.re == 0.0 && a0.im == 0.0 {
        return rt("First denominator filter coefficient must be non-zero.");
    }
    let dim = dim_at(a, 4, &x.dims)?;
    let kind = single_any(&[&b, &den, &x]);
    let cplx = b.is_complex() || den.is_complex() || x.is_complex() || matches!(a.get(3), Some(Value::Num(z)) if z.is_complex());
    let nb = b.numel();
    let na = den.numel();
    let nz = nb.max(na) - 1;
    let bn: Vec<C> = (0..nz + 1).map(|k| if k < nb { c_at(&b, k).div(a0) } else { C::new(0.0, 0.0) }).collect();
    let an: Vec<C> = (0..nz + 1).map(|k| if k < na { c_at(&den, k).div(a0) } else { C::new(0.0, 0.0) }).collect();
    let (stride, _, outer) = layout(&x.dims, dim);
    let nslices = stride * outer;
    let zi: Vec<C> = match a.get(3) {
        Some(Value::Num(z)) if !z.is_empty() => {
            if z.numel() != nz * nslices || (nslices > 1 && z.dims[0] != nz) {
                return rt("Initial conditions must be a vector of length max(length(a),length(b))-1, or an array with the leading dimension of size max(length(a),length(b))-1 and with remaining dimensions matching those of x.");
            }
            (0..z.numel()).map(|i| c_at(z, i)).collect()
        }
        _ => vec![C::new(0.0, 0.0); nz * nslices],
    };
    let mut zf = vec![C::new(0.0, 0.0); nz * nslices];
    let mut slice = 0;
    let len = x.dims.get(dim - 1).copied().unwrap_or(1);
    let mut y = map_dim_c(&x, dim, len, |v| {
        let s = slice;
        slice += 1;
        let mut z: Vec<C> = zi[s * nz..(s + 1) * nz].to_vec();
        z.push(C::new(0.0, 0.0));
        let out: Vec<C> = v
            .iter()
            .map(|&xn| {
                let yn = bn[0].mul(xn).add(z[0]);
                for k in 1..=nz {
                    z[k - 1] = bn[k].mul(xn).add(z[k]).sub(an[k].mul(yn));
                }
                yn
            })
            .collect();
        zf[s * nz..(s + 1) * nz].copy_from_slice(&z[..nz]);
        out
    });
    if !cplx {
        y.im = None;
    }
    let y = y.cast_to(kind);
    if nargout < 2 {
        return one(Value::Num(y));
    }
    // zf: nz by the dimensions of x other than dim.
    let mut zd = vec![nz];
    zd.extend(x.dims.iter().enumerate().filter(|&(k, _)| k + 1 != dim).map(|(_, &n)| n));
    if zd.len() == 1 {
        zd.push(1);
    }
    let mut zf = from_c(zd, &zf, true);
    if !cplx {
        zf.im = None;
    }
    Ok(Some(vec![Value::Num(y), Value::Num(zf.cast_to(kind))]))
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Full,
    Same,
    Valid,
}

fn shape_arg(v: Option<&Value>) -> R<Shape> {
    match v.map(lower) {
        None => Ok(Shape::Full),
        Some(Some(t)) if t == "full" => Ok(Shape::Full),
        Some(Some(t)) if t == "same" => Ok(Shape::Same),
        Some(Some(t)) if t == "valid" => Ok(Shape::Valid),
        _ => rt("SHAPE must be 'full', 'same', or 'valid'."),
    }
}

fn strides(d: &[usize]) -> Vec<usize> {
    let mut s = vec![1; d.len()];
    for k in 1..d.len() {
        s[k] = s[k - 1] * d[k - 1];
    }
    s
}

/// N-D convolution with the requested shape.
fn convolve(x: &Matrix, h: &Matrix, shape: Shape) -> Matrix {
    let nd_ = x.dims.len().max(h.dims.len());
    let dx: Vec<usize> = (0..nd_).map(|k| x.dims.get(k).copied().unwrap_or(1)).collect();
    let dh: Vec<usize> = (0..nd_).map(|k| h.dims.get(k).copied().unwrap_or(1)).collect();
    let full: Vec<usize> = (0..nd_).map(|k| (dx[k] + dh[k]).saturating_sub(1).max(dx[k]).max(dh[k])).collect();
    // Output region in full coordinates: (start, size) per dimension.
    let region: Vec<(usize, usize)> = (0..nd_)
        .map(|k| match shape {
            Shape::Full => (0, full[k]),
            Shape::Same => (dh[k] / 2, dx[k]),
            Shape::Valid => (dh[k].saturating_sub(1), if dh[k] == 0 { 0 } else { (dx[k] + 1).saturating_sub(dh[k]) }),
        })
        .collect();
    let out_dims: Vec<usize> = region.iter().map(|r| r.1).collect();
    let n_out = nd::numel(&out_dims);
    let cplx = x.is_complex() || h.is_complex();
    let mut re = vec![0.0; n_out];
    let mut im = vec![0.0; n_out];
    let so = strides(&out_dims);
    let nh = h.numel();
    let mut sub_x = vec![0usize; nd_];
    let mut sub_h = vec![0usize; nd_];
    for ix in 0..x.numel() {
        let xv = c_at(x, ix);
        sub_h.iter_mut().for_each(|s| *s = 0);
        for ih in 0..nh {
            // Full position = sub_x + sub_h; keep it if inside the region.
            let mut p = 0;
            let mut inside = true;
            for k in 0..nd_ {
                let f = sub_x[k] + sub_h[k];
                if f < region[k].0 || f - region[k].0 >= region[k].1 {
                    inside = false;
                    break;
                }
                p += (f - region[k].0) * so[k];
            }
            if inside {
                let hv = c_at(h, ih);
                if cplx {
                    let z = xv.mul(hv);
                    re[p] += z.re;
                    im[p] += z.im;
                } else {
                    re[p] += xv.re * hv.re;
                }
            }
            for (k, s) in sub_h.iter_mut().enumerate() {
                *s += 1;
                if *s < dh[k] {
                    break;
                }
                *s = 0;
            }
        }
        for (k, s) in sub_x.iter_mut().enumerate() {
            *s += 1;
            if *s < dx[k] {
                break;
            }
            *s = 0;
        }
    }
    let mut m = Matrix::from_dims(out_dims, re);
    if cplx {
        m.im = Some(im);
        m.narrow();
    }
    m
}

fn conv_input(m: &Matrix) -> R<()> {
    if m.kind == Kind::Char {
        return rt("Invalid data type. First and second arguments must be numeric or logical.");
    }
    Ok(())
}

fn conv2(a: &[Value]) -> R<Matrix> {
    // conv2(u, v, A, shape) when the third argument is numeric.
    if a.len() >= 3 && !a[2].is_text() {
        let u = num_arg(a, 0, "conv2")?;
        let v = num_arg(a, 1, "conv2")?;
        let x = num_arg(a, 2, "conv2")?;
        for m in [&u, &v, &x] {
            conv_input(m)?;
        }
        if !(u.is_vector() || u.is_empty()) || !(v.is_vector() || v.is_empty()) {
            return rt("First and second arguments must be vectors.");
        }
        let shape = shape_arg(a.get(3))?;
        let kind = single_any(&[&u, &v, &x]);
        let uc = u.clone().reshaped(vec![u.numel(), 1]);
        let vr = v.clone().reshaped(vec![1, v.numel()]);
        let r = convolve(&convolve(&x, &uc, Shape::Full), &vr, Shape::Full);
        return Ok(crop_full(&r, &x, (u.numel(), v.numel()), shape).cast_to(kind));
    }
    let x = num_arg(a, 0, "conv2")?;
    let h = num_arg(a, 1, "conv2")?;
    conv_input(&x)?;
    conv_input(&h)?;
    if x.dims.len() > 2 || h.dims.len() > 2 {
        return rt("N-D arrays are not supported.");
    }
    let shape = shape_arg(a.get(2))?;
    let kind = single_any(&[&x, &h]);
    Ok(convolve(&x, &h, shape).cast_to(kind))
}

/// Takes the `shape` part of a full 2-D convolution of `x` with a kernel of size `k`.
fn crop_full(full: &Matrix, x: &Matrix, k: (usize, usize), shape: Shape) -> Matrix {
    if shape == Shape::Full {
        return full.clone();
    }
    let (mx, nx) = (x.rows(), x.cols());
    let (mk, nk) = k;
    let (r0, rn, c0, cn) = match shape {
        Shape::Same => (mk / 2, mx, nk / 2, nx),
        _ => (mk.saturating_sub(1), (mx + 1).saturating_sub(mk), nk.saturating_sub(1), (nx + 1).saturating_sub(nk)),
    };
    let mut pos = Vec::with_capacity(rn * cn);
    for c in 0..cn {
        for r in 0..rn {
            pos.push((c + c0) * full.rows() + r + r0);
        }
    }
    let mut m = full.gather(&pos, vec![rn, cn]);
    m.kind = full.kind;
    m
}

fn convn(a: &[Value]) -> R<Matrix> {
    let x = num_arg(a, 0, "convn")?;
    let h = num_arg(a, 1, "convn")?;
    conv_input(&x)?;
    conv_input(&h)?;
    let shape = shape_arg(a.get(2))?;
    Ok(convolve(&x, &h, shape).cast_to(single_any(&[&x, &h])))
}

fn filter2(a: &[Value]) -> R<Matrix> {
    let h = num_arg(a, 0, "filter2")?;
    let x = num_arg(a, 1, "filter2")?;
    let shape = match a.get(2) {
        None => Shape::Same,
        v => shape_arg(v)?,
    };
    if x.dims.len() > 2 || h.dims.len() > 2 {
        return rt("N-D arrays are not supported.");
    }
    // Correlation: convolution with the kernel rotated by 180 degrees.
    let pos: Vec<usize> = (0..h.numel()).rev().collect();
    let rot = h.gather(&pos, h.dims.clone());
    Ok(convolve(&x, &rot, shape).cast_to(single_any(&[&x, &h])))
}

/// Householder least squares: minimizes |A x - b| for a tall A (rows x cols, column-major).
fn lstsq(a: &[f64], rows: usize, cols: usize, b: &[f64]) -> Vec<f64> {
    let mut q = a.to_vec();
    let mut y = b.to_vec();
    for k in 0..cols.min(rows) {
        let norm = (k..rows).map(|i| q[k * rows + i].powi(2)).sum::<f64>().sqrt();
        if norm == 0.0 {
            continue;
        }
        let alpha = if q[k * rows + k] > 0.0 { -norm } else { norm };
        let mut v: Vec<f64> = (k..rows).map(|i| q[k * rows + i]).collect();
        v[0] -= alpha;
        let vn: f64 = v.iter().map(|x| x * x).sum();
        if vn == 0.0 {
            continue;
        }
        for j in k..cols {
            let s: f64 = (k..rows).map(|i| v[i - k] * q[j * rows + i]).sum::<f64>() * 2.0 / vn;
            for i in k..rows {
                q[j * rows + i] -= s * v[i - k];
            }
        }
        let s: f64 = (k..rows).map(|i| v[i - k] * y[i]).sum::<f64>() * 2.0 / vn;
        for i in k..rows {
            y[i] -= s * v[i - k];
        }
    }
    let big = (0..cols.min(rows)).map(|k| q[k * rows + k].abs()).fold(0.0, f64::max);
    let mut x = vec![0.0; cols];
    for k in (0..cols.min(rows)).rev() {
        let s: f64 = (k + 1..cols).map(|j| q[j * rows + k] * x[j]).sum();
        let d = q[k * rows + k];
        x[k] = if d.abs() > 1e-13 * big { (y[k] - s) / d } else { 0.0 };
    }
    x
}

fn detrend(a: &[Value]) -> R<Matrix> {
    let o = Opts::parse(a, NANFLAGS, &["SamplePoints", "Continuous"])?;
    let x = num_arg(&o.pos, 0, "detrend")?;
    if !x.kind.is_float() {
        return rt("Input data must be of type single or double.");
    }
    let degree = match o.pos.get(1) {
        None => 1,
        Some(v) => match lower(v).as_deref() {
            Some("constant") => 0,
            Some("linear") => 1,
            Some("quadratic") => 2,
            Some(t) => return rt(format!("Invalid option '{t}'.")),
            None => {
                let n = v.scalar()?;
                if n < 0.0 || n.fract() != 0.0 {
                    return rt("Polynomial degree must be a nonnegative integer scalar.");
                }
                n as usize
            }
        },
    };
    let bps: Vec<f64> = match o.pos.get(2) {
        Some(v) => v.to_matrix()?.data,
        None => Vec::new(),
    };
    let continuous = match o.get("Continuous") {
        Some(v) => v.is_true()?,
        None => true,
    };
    let row = x.dims.len() == 2 && x.dims[0] == 1;
    let dim = if row { 2 } else { 1 };
    let len = x.dims[dim - 1];
    let t: Vec<f64> = match o.get("SamplePoints") {
        Some(v) => v.to_matrix()?.data,
        None => (1..=len).map(|k| k as f64).collect(),
    };
    if t.len() != len {
        return rt("Sample points must have the same length as the operating dimension.");
    }
    let (lo, hi) = (t.first().copied().unwrap_or(0.0), t.last().copied().unwrap_or(0.0));
    let mut bps: Vec<f64> = bps.into_iter().filter(|&b| b > lo && b < hi).collect();
    bps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    bps.dedup();
    // Scale the sample points to [-1, 1] for conditioning.
    let half = if hi > lo { (hi - lo) / 2.0 } else { 1.0 };
    let sc = |v: f64| (v - (lo + hi) / 2.0) / half;
    let ts: Vec<f64> = t.iter().map(|&v| sc(v)).collect();
    let bs: Vec<f64> = bps.iter().map(|&v| sc(v)).collect();
    let fit = |idx: &[usize], v: &[f64], bs: &[f64]| -> Vec<f64> {
        let rows = idx.len();
        let mut cols: Vec<Vec<f64>> = (0..=degree).map(|p| idx.iter().map(|&i| ts[i].powi(p as i32)).collect()).collect();
        // Continuity at the breakpoints is of the function only: (t - b)_+^k for k = 1..n.
        for &b in bs {
            for k in 1..=degree {
                cols.push(idx.iter().map(|&i| if ts[i] > b { (ts[i] - b).powi(k as i32) } else { 0.0 }).collect());
            }
        }
        let ncols = cols.len().min(rows.max(1));
        let flat: Vec<f64> = cols.iter().take(ncols).flatten().copied().collect();
        let rhs: Vec<f64> = idx.iter().map(|&i| v[i]).collect();
        let coef = lstsq(&flat, rows, ncols, &rhs);
        (0..rows).map(|r| rhs[r] - (0..ncols).map(|c| flat[c * rows + r] * coef[c]).sum::<f64>()).collect()
    };
    let out = map_dim(&x, dim, len, |v, _| {
        if continuous && degree == 0 && !bs.is_empty() {
            // A continuous piecewise constant is the constant of the first segment.
            let first: Vec<f64> = (0..len).filter(|&i| ts[i] <= bs[0]).map(|i| v[i]).collect();
            let mu = mean(&first);
            return v.iter().map(|x| x - mu).collect();
        }
        if continuous || bs.is_empty() {
            let all: Vec<usize> = (0..len).collect();
            fit(&all, v, &bs)
        } else {
            segments(&ts, &bs).into_iter().flat_map(|seg| fit(&seg, v, &[])).collect()
        }
    });
    Ok(out.cast_to(x.kind))
}

/// Index lists of the segments between breakpoints (a point at a breakpoint starts a segment).
fn segments(t: &[f64], bps: &[f64]) -> Vec<Vec<usize>> {
    let mut segs = vec![Vec::new(); bps.len() + 1];
    for (i, &v) in t.iter().enumerate() {
        let k = bps.iter().filter(|&&b| v >= b).count();
        segs[k].push(i);
    }
    segs.into_iter().filter(|s| !s.is_empty()).collect()
}

// ---- covariance and correlation --------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Rows {
    All,
    Omit,
    Partial,
}

/// Rows and columns of the observations: a vector becomes one column.
fn obs_columns(m: &Matrix) -> (usize, usize) {
    if m.is_vector() || m.is_scalar() { (m.numel(), 1) } else { (m.rows(), m.cols()) }
}

fn is_nan_c(z: C) -> bool {
    z.re.is_nan() || z.im.is_nan()
}

/// Covariance matrix of the columns of an n x p complex data matrix.
fn cov_matrix(x: &[C], n: usize, p: usize, w: f64, rows: Rows) -> Vec<C> {
    let nanrow = |i: usize| (0..p).any(|j| is_nan_c(x[j * n + i]));
    let keep: Vec<usize> = match rows {
        Rows::Omit => (0..n).filter(|&i| !nanrow(i)).collect(),
        _ => (0..n).collect(),
    };
    let mut c = vec![C::new(0.0, 0.0); p * p];
    for i in 0..p {
        for j in i..p {
            let idx: Vec<usize> =
                if rows == Rows::Partial { keep.iter().copied().filter(|&r| !is_nan_c(x[i * n + r]) && !is_nan_c(x[j * n + r])).collect() } else { keep.clone() };
            let m = idx.len();
            let v = if m == 0 {
                C::new(f64::NAN, 0.0)
            } else {
                let mi = idx.iter().fold(C::new(0.0, 0.0), |s, &r| s.add(x[i * n + r]));
                let mj = idx.iter().fold(C::new(0.0, 0.0), |s, &r| s.add(x[j * n + r]));
                let (mi, mj) = (C::new(mi.re / m as f64, mi.im / m as f64), C::new(mj.re / m as f64, mj.im / m as f64));
                let s = idx.iter().fold(C::new(0.0, 0.0), |s, &r| s.add(x[i * n + r].sub(mi).conj().mul(x[j * n + r].sub(mj))));
                let d = if w == 1.0 || m == 1 { m as f64 } else { (m - 1) as f64 };
                C::new(s.re / d, s.im / d)
            };
            c[j * p + i] = if i == j { C::new(v.re, 0.0) } else { v };
            c[i * p + j] = if i == j { C::new(v.re, 0.0) } else { v.conj() };
        }
    }
    c
}

/// Parses cov/corrcoef data arguments: one matrix, or two arrays taken as two variables.
fn data_columns(x: &Matrix, y: Option<&Matrix>) -> R<(Vec<C>, usize, usize)> {
    match y {
        Some(y) => {
            if x.numel() != y.numel() {
                return rt("The number of elements in x and y must match.");
            }
            let n = x.numel();
            let mut d: Vec<C> = (0..n).map(|i| c_at(x, i)).collect();
            d.extend((0..n).map(|i| c_at(y, i)));
            Ok((d, n, 2))
        }
        None => {
            let (n, p) = obs_columns(x);
            Ok(((0..x.numel()).map(|i| c_at(x, i)).collect(), n, p))
        }
    }
}

fn cov(a: &[Value]) -> R<Matrix> {
    let mut rows = Rows::All;
    let mut nums: Vec<Matrix> = Vec::new();
    for v in a {
        match lower(v).as_deref() {
            Some("includenan" | "includemissing") => rows = Rows::All,
            Some("omitrows") => rows = Rows::Omit,
            Some("partialrows") => rows = Rows::Partial,
            Some(t) => return rt(format!("Invalid option '{t}'.")),
            None => nums.push(v.to_matrix()?),
        }
    }
    if nums.is_empty() {
        return rt("Not enough input arguments.");
    }
    let mut w = 0.0;
    let is_w = |m: &Matrix| m.is_scalar() && (m.data[0] == 0.0 || m.data[0] == 1.0);
    if nums.len() == 3 || (nums.len() == 2 && is_w(&nums[1])) {
        let wm = nums.pop().unwrap();
        if !is_w(&wm) {
            return rt("W must be 0 or 1.");
        }
        w = wm.data[0];
    }
    let kind = single_any(&nums.iter().collect::<Vec<_>>());
    let x = &nums[0];
    if nums.len() == 1 && x.is_empty() {
        let p = if x.dims == [0, 0] { 1 } else { x.cols() };
        return Ok(Matrix::filled_dims(vec![p, p], f64::NAN).cast_to(kind));
    }
    let (d, n, p) = data_columns(x, nums.get(1))?;
    let c = cov_matrix(&d, n, p, w, rows);
    Ok(from_c(vec![p, p], &c, false).cast_to(kind))
}

fn corrcoef(a: &[Value], nargout: usize) -> Ret {
    let o = Opts::parse(a, &[], &["Alpha", "Rows"])?;
    let x = num_arg(&o.pos, 0, "corrcoef")?;
    let y = match o.pos.get(1) {
        Some(v) => Some(v.to_matrix()?),
        None => None,
    };
    let kind = single_any(&[&x]);
    let rows = match o.get("Rows").map(|v| v.text().map(|t| t.to_lowercase())).transpose()?.as_deref() {
        None | Some("all") => Rows::All,
        Some("complete") => Rows::Omit,
        Some("pairwise") => Rows::Partial,
        Some(_) => return rt("Invalid 'Rows' value. Valid values are 'all', 'complete', and 'pairwise'."),
    };
    let alpha = match o.get("Alpha") {
        Some(v) => {
            let al = v.scalar()?;
            if !(al > 0.0 && al < 1.0) {
                return rt("The 'Alpha' parameter value must be a scalar between 0 and 1.");
            }
            al
        }
        None => 0.05,
    };
    if y.is_none() && x.is_empty() {
        let nan = Value::Num(Matrix::scalar(f64::NAN).cast_to(kind));
        return Ok(Some(vec![nan.clone(), nan.clone(), nan.clone(), nan]));
    }
    let (d, n, p) = data_columns(&x, y.as_ref())?;
    let c = cov_matrix(&d, n, p, 0.0, rows);
    let mut r = vec![C::new(0.0, 0.0); p * p];
    for i in 0..p {
        for j in 0..p {
            let (cij, cii, cjj) = if rows == Rows::Partial && i != j {
                // Pairwise: both variances over the rows where the pair is complete.
                let keep: Vec<usize> = (0..n).filter(|&k| !is_nan_c(d[i * n + k]) && !is_nan_c(d[j * n + k])).collect();
                let sub: Vec<C> = keep.iter().map(|&k| d[i * n + k]).chain(keep.iter().map(|&k| d[j * n + k])).collect();
                let cc = cov_matrix(&sub, keep.len(), 2, 0.0, Rows::All);
                (cc[2], cc[0].re, cc[3].re)
            } else {
                (c[j * p + i], c[i * p + i].re, c[j * p + j].re)
            };
            let s = (cii * cjj).sqrt();
            let mut v = C::new(cij.re / s, cij.im / s);
            if i == j {
                v = C::new(if s.is_nan() || s == 0.0 { f64::NAN } else { 1.0 }, 0.0);
            } else if v.im == 0.0 {
                v.re = v.re.clamp(-1.0, 1.0);
            } else if v.abs() > 1.0 {
                let m = v.abs();
                v = C::new(v.re / m, v.im / m);
            }
            r[j * p + i] = v;
        }
    }
    let rm = from_c(vec![p, p], &r, false).cast_to(kind);
    if nargout <= 1 {
        return one(Value::Num(rm));
    }
    // Observations per pair (for the p-values and bounds).
    let nanrow = |i: usize| (0..p).any(|j| is_nan_c(d[j * n + i]));
    let count = |i: usize, j: usize| -> f64 {
        (0..n)
            .filter(|&k| match rows {
                Rows::All => true,
                Rows::Omit => !nanrow(k),
                Rows::Partial => !is_nan_c(d[i * n + k]) && !is_nan_c(d[j * n + k]),
            })
            .count() as f64
    };
    let mut pv = vec![0.0; p * p];
    let mut lo = vec![0.0; p * p];
    let mut hi = vec![0.0; p * p];
    let z = norm_inv(1.0 - alpha / 2.0);
    for i in 0..p {
        for j in 0..p {
            let k = j * p + i;
            let rv = r[k].re;
            let m = count(i, j);
            if i == j {
                pv[k] = if rv.is_nan() { f64::NAN } else { 1.0 };
                lo[k] = rv;
                hi[k] = rv;
                continue;
            }
            let t = rv * ((m - 2.0) / (1.0 - rv * rv)).sqrt();
            pv[k] = if t.is_nan() { f64::NAN } else { (2.0 * t_upper(t.abs(), m - 2.0)).min(1.0) };
            let zr = 0.5 * ((1.0 + rv) / (1.0 - rv)).ln();
            let half = z / (m - 3.0).sqrt();
            lo[k] = (zr - half).tanh();
            hi[k] = (zr + half).tanh();
        }
    }
    let mk = |v: Vec<f64>| Value::Num(Matrix::from_dims(vec![p, p], v).cast_to(kind));
    Ok(Some(vec![Value::Num(rm), mk(pv), mk(lo), mk(hi)]))
}

fn xcorr(name: &str, a: &[Value]) -> Ret {
    let mut scale = "none".to_string();
    let mut nums: Vec<Matrix> = Vec::new();
    for v in a {
        match lower(v) {
            Some(t) if ["none", "biased", "unbiased", "normalized", "coeff"].contains(&t.as_str()) => scale = t,
            Some(t) => return rt(format!("Invalid option '{t}'.")),
            None => nums.push(v.to_matrix()?),
        }
    }
    if nums.is_empty() {
        return rt("Not enough input arguments.");
    }
    let x = nums.remove(0);
    // xcorr(x, maxlag): a scalar second argument is the maximum lag.
    let (y, maxlag) = match nums.len() {
        0 => (None, None),
        1 if nums[0].is_scalar() => (None, Some(nums[0].data[0])),
        1 => (Some(nums.remove(0)), None),
        _ => {
            let y = nums.remove(0);
            let ml = nums.remove(0);
            (Some(y), if ml.is_empty() { None } else { Some(ml.data[0]) })
        }
    };
    let kind = single_any(&[&x]);
    let center = |m: &Matrix, cols: usize| -> Vec<Vec<C>> {
        let rows = m.numel() / cols.max(1);
        (0..cols)
            .map(|j| {
                let v: Vec<C> = (0..rows).map(|i| c_at(m, j * rows + i)).collect();
                if name == "xcov" && !v.is_empty() {
                    let s = v.iter().fold(C::new(0.0, 0.0), |s, z| s.add(*z));
                    let mu = C::new(s.re / rows as f64, s.im / rows as f64);
                    v.into_iter().map(|z| z.sub(mu)).collect()
                } else {
                    v
                }
            })
            .collect()
    };
    let xvec = x.is_vector() || x.is_scalar();
    let mut cols: Vec<Vec<C>> = if xvec { center(&x, 1) } else { center(&x, x.cols()) };
    let pairs: Vec<(usize, usize)>;
    if let Some(y) = &y {
        if !xvec || !(y.is_vector() || y.is_scalar()) {
            return rt("When the second input is a vector, the first input must be a vector.");
        }
        let yc = center(y, 1).remove(0);
        let len = cols[0].len().max(yc.len());
        if scale != "none" && cols[0].len() != yc.len() {
            return rt("Scale option must be 'none' for input vectors of different lengths.");
        }
        cols[0].resize(len, C::new(0.0, 0.0));
        let mut yc = yc;
        yc.resize(len, C::new(0.0, 0.0));
        cols.push(yc);
        pairs = vec![(0, 1)];
    } else {
        let p = cols.len();
        pairs = (0..p).flat_map(|i| (0..p).map(move |j| (i, j))).collect();
    }
    let n = cols[0].len();
    let ml = match maxlag {
        Some(m) => {
            if m < 0.0 || m.fract() != 0.0 {
                return rt("Maximum lag must be a nonnegative integer scalar.");
            }
            m as usize
        }
        None => n.saturating_sub(1),
    };
    let nl = 2 * ml + 1;
    let mut out = Vec::with_capacity(nl * pairs.len());
    for &(i, j) in &pairs {
        let (u, v) = (&cols[i], &cols[j]);
        let corr = |m: isize| -> C {
            let mut s = C::new(0.0, 0.0);
            for k in 0..n as isize {
                let p = k + m;
                if p >= 0 && (p as usize) < n {
                    s = s.add(u[p as usize].mul(v[k as usize].conj()));
                }
            }
            s
        };
        let r0 = if scale == "normalized" || scale == "coeff" {
            let e = |w: &Vec<C>| w.iter().map(|z| z.re * z.re + z.im * z.im).sum::<f64>();
            (e(u) * e(v)).sqrt()
        } else {
            1.0
        };
        for lag in -(ml as isize)..=(ml as isize) {
            let z = if lag.unsigned_abs() >= n { C::new(0.0, 0.0) } else { corr(lag) };
            let d = match scale.as_str() {
                "biased" => n as f64,
                "unbiased" => (n as f64 - lag.unsigned_abs() as f64).max(1.0),
                "normalized" | "coeff" => r0,
                _ => 1.0,
            };
            out.push(C::new(z.re / d, z.im / d));
        }
    }
    let row = xvec && x.dims[0] == 1 && x.numel() > 1;
    let dims = if row && pairs.len() == 1 { vec![1, nl] } else { vec![nl, pairs.len()] };
    let r = from_c(dims, &out, false).cast_to(kind);
    let lags = Matrix::row((-(ml as isize)..=(ml as isize)).map(|l| l as f64).collect());
    Ok(Some(vec![Value::Num(r), Value::Num(lags)]))
}


// ---- moving-window statistics -------------------------------------------------------------------

/// A moving window: `back`/`fwd` extents in sample-point units. A scalar window length is
/// half-open on the forward side ([t - k/2, t + k/2)); a two-element one is closed.
#[derive(Clone, Copy)]
struct Window {
    back: f64,
    fwd: f64,
    half_open: bool,
}

fn window_arg(v: &Value) -> R<Window> {
    let bad = || rt("Window length must be a finite, positive, real scalar or 2-element vector of finite, nonnegative, real scalars.");
    let m = match v {
        Value::Num(m) if m.kind != Kind::Char && !m.is_complex() => m,
        _ => return bad(),
    };
    match m.numel() {
        1 if m.data[0] > 0.0 && m.data[0].is_finite() => Ok(Window { back: m.data[0] / 2.0, fwd: m.data[0] / 2.0, half_open: true }),
        2 if m.data.iter().all(|&x| x >= 0.0 && x.is_finite()) => Ok(Window { back: m.data[0], fwd: m.data[1], half_open: false }),
        _ => bad(),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Ends {
    Shrink,
    Discard,
    Fill(f64),
    Pad(f64),
}

fn ends_arg(v: Option<&Value>) -> R<Ends> {
    match v {
        None => Ok(Ends::Shrink),
        Some(v) => match lower(v).as_deref() {
            Some("shrink") => Ok(Ends::Shrink),
            Some("discard") => Ok(Ends::Discard),
            Some("fill") => Ok(Ends::Fill(f64::NAN)),
            Some(_) => rt("Endpoints must be 'shrink', 'discard', 'fill', or a numeric or logical scalar."),
            None => match v {
                Value::Num(m) if m.numel() == 1 && m.kind != Kind::Char => Ok(Ends::Pad(m.data[0])),
                _ => rt("Endpoints must be 'shrink', 'discard', 'fill', or a numeric or logical scalar."),
            },
        },
    }
}

/// For each point: the index range of its window and the number of padding values that a
/// numeric Endpoints value adds before and after it.
struct Span {
    lo: usize,
    hi: usize,
    pad_before: usize,
    pad_after: usize,
    complete: bool,
}

fn spans(t: Option<&[f64]>, n: usize, w: Window) -> Vec<Span> {
    let pos = |i: usize| t.map(|t| t[i]).unwrap_or((i + 1) as f64);
    let inside = |c: f64, x: f64| x >= c - w.back && if w.half_open { x < c + w.fwd } else { x <= c + w.fwd };
    // Element counts behind and ahead for unit spacing (used by padding and completeness).
    let nb = w.back.floor() as usize;
    let nf = if w.half_open { (w.fwd.ceil() as usize).saturating_sub(1) } else { w.fwd.floor() as usize };
    let mut out = Vec::with_capacity(n);
    let (mut lo, mut hi) = (0usize, 0usize);
    for i in 0..n {
        let c = pos(i);
        while lo < n && !inside(c, pos(lo)) && pos(lo) < c {
            lo += 1;
        }
        if hi < lo {
            hi = lo;
        }
        while hi < n && inside(c, pos(hi)) {
            hi += 1;
        }
        let (pb, pa, complete) = match t {
            None => (nb.saturating_sub(i), (i + nf + 1).saturating_sub(n), i >= nb && i + nf < n),
            Some(t) => (0, 0, c - w.back >= t[0] && c + w.fwd <= t[n - 1]),
        };
        out.push(Span { lo, hi, pad_before: pb, pad_after: pa, complete });
    }
    out
}

/// Statistic of one window (values already filtered for NaN when omitted).
fn window_stat(name: &str, v: &mut Vec<f64>, w: f64, omit: bool) -> f64 {
    if omit {
        v.retain(|x| !x.is_nan());
    }
    match name {
        "movsum" => v.iter().sum(),
        "movprod" => v.iter().product(),
        "movmean" => {
            if v.is_empty() {
                f64::NAN
            } else {
                mean(v)
            }
        }
        "movmin" | "movmax" => {
            if v.iter().any(|x| x.is_nan()) {
                return f64::NAN;
            }
            if v.is_empty() {
                return f64::NAN;
            }
            if name == "movmin" { v.iter().copied().fold(f64::INFINITY, f64::min) } else { v.iter().copied().fold(f64::NEG_INFINITY, f64::max) }
        }
        "movmedian" => median(v),
        "movvar" => var(v, w),
        "movstd" => var(v, w).sqrt(),
        "movmad" => {
            let m = median(&mut v.clone());
            let mut d: Vec<f64> = v.iter().map(|x| (x - m).abs()).collect();
            median(&mut d)
        }
        _ => unreachable!(),
    }
}

/// Applies a moving statistic to one vector.
fn moving_vec(name: &str, x: &[f64], sp: &[Span], ends: Ends, w: f64, omit: bool) -> Vec<f64> {
    let mut out = Vec::with_capacity(x.len());
    let mut buf = Vec::new();
    for s in sp {
        if ends == Ends::Discard && !s.complete {
            continue;
        }
        if let Ends::Fill(f) = ends {
            if !s.complete {
                out.push(f);
                continue;
            }
        }
        buf.clear();
        if let Ends::Pad(p) = ends {
            buf.extend(std::iter::repeat_n(p, s.pad_before));
        }
        buf.extend_from_slice(&x[s.lo..s.hi]);
        if let Ends::Pad(p) = ends {
            buf.extend(std::iter::repeat_n(p, s.pad_after));
        }
        out.push(window_stat(name, &mut buf, w, omit));
    }
    out
}

/// Parsed sample points along a dimension of length `n`.
fn sample_points(v: Option<&Value>, n: usize) -> R<Option<Vec<f64>>> {
    match v {
        None => Ok(None),
        Some(v) => {
            let t = match v {
                Value::Time(_) => return rt("Sample points of class datetime or duration are not supported."),
                v => v.to_matrix()?.data,
            };
            if t.len() != n {
                return rt("Sample points must have the same length as the operating dimension.");
            }
            if t.windows(2).any(|p| !(p[1] > p[0])) || t.iter().any(|x| !x.is_finite()) {
                return rt("Sample points must be unique, sorted, and finite.");
            }
            Ok(Some(t))
        }
    }
}

fn moving(name: &str, a: &[Value]) -> R<Matrix> {
    let o = Opts::parse(a, NANFLAGS, &["Endpoints", "SamplePoints"])?;
    let x = num_arg(&o.pos, 0, name)?;
    if x.is_complex() && !matches!(name, "movsum" | "movmean" | "movprod") {
        return rt(format!("Complex values are not supported by {name}."));
    }
    let win = window_arg(arg(&o.pos, 1, name)?)?;
    let has_w = matches!(name, "movstd" | "movvar");
    let w = if has_w {
        match o.pos.get(2) {
            Some(Value::Num(m)) if !m.is_empty() => {
                if !(m.data[0] == 0.0 || m.data[0] == 1.0) {
                    return rt("Normalization factor must be 0 or 1.");
                }
                m.data[0]
            }
            _ => 0.0,
        }
    } else {
        0.0
    };
    let dim = dim_at(&o.pos, if has_w { 3 } else { 2 }, &x.dims)?;
    let ends = ends_arg(o.get("Endpoints"))?;
    let omit = omit_nan(&o, matches!(name, "movmin" | "movmax"));
    let len = x.dims.get(dim - 1).copied().unwrap_or(1);
    let t = sample_points(o.get("SamplePoints"), len)?;
    if let Some(t) = &t {
        let uniform = t.len() < 3 || t.windows(2).all(|p| ((p[1] - p[0]) - (t[1] - t[0])).abs() <= 4.0 * f64::EPSILON * t[t.len() - 1].abs().max(t[0].abs()));
        if ends != Ends::Shrink && !uniform {
            return rt("'Endpoints' value must be 'shrink' when the 'SamplePoints' values are not uniformly spaced.");
        }
    }
    let sp = spans(t.as_deref(), len, win);
    let out_len = if ends == Ends::Discard { sp.iter().filter(|s| s.complete).count() } else { len };
    let kind = match (name, x.kind) {
        ("movmin" | "movmax", k) => k,
        (_, Kind::Single) => Kind::Single,
        _ => Kind::Double,
    };
    let mut out = if x.is_complex() {
        // movsum/movmean/movprod of complex data: real and imaginary parts (products need the
        // full complex arithmetic).
        let mut idx = 0;
        let _ = &mut idx;
        map_dim_c(&x, dim, out_len, |v| {
            let mut r = Vec::with_capacity(out_len);
            for s in &sp {
                if ends == Ends::Discard && !s.complete {
                    continue;
                }
                if matches!(ends, Ends::Fill(_)) && !s.complete {
                    r.push(C::new(f64::NAN, 0.0));
                    continue;
                }
                let mut vals: Vec<C> = Vec::new();
                if let Ends::Pad(p) = ends {
                    vals.extend(std::iter::repeat_n(C::new(p, 0.0), s.pad_before));
                }
                vals.extend_from_slice(&v[s.lo..s.hi]);
                if let Ends::Pad(p) = ends {
                    vals.extend(std::iter::repeat_n(C::new(p, 0.0), s.pad_after));
                }
                if omit {
                    vals.retain(|z| !is_nan_c(*z));
                }
                let z = match name {
                    "movprod" => vals.iter().fold(C::new(1.0, 0.0), |acc, z| acc.mul(*z)),
                    _ => {
                        let s = vals.iter().fold(C::new(0.0, 0.0), |acc, z| acc.add(*z));
                        if name == "movmean" { C::new(s.re / vals.len() as f64, s.im / vals.len() as f64) } else { s }
                    }
                };
                r.push(z);
            }
            r
        })
    } else {
        map_dim(&x, dim, out_len, |v, _| moving_vec(name, v, &sp, ends, w, omit))
    };
    out.narrow();
    Ok(out.cast_to(kind))
}

// ---- percentiles, bounds and top-k ---------------------------------------------------------------

/// Reorders `m` so that the dimensions in `vecdim` become one leading dimension; returns the
/// reshaped matrix and the output dims template (vecdim dims set to 1).
fn fold_dims(m: &Matrix, vecdim: &[usize]) -> R<(Matrix, Dims)> {
    let nd_ = m.dims.len().max(vecdim.iter().copied().max().unwrap_or(1));
    let d: Vec<usize> = (0..nd_).map(|k| m.dims.get(k).copied().unwrap_or(1)).collect();
    let mut order: Vec<usize> = vecdim.iter().map(|&k| k - 1).collect();
    order.extend((0..nd_).filter(|k| !vecdim.contains(&(k + 1))));
    let (pd, data) = nd::permute(&d, &m.data, &order)?;
    let im = match &m.im {
        Some(im) => Some(nd::permute(&d, im, &order)?.1),
        None => None,
    };
    let lead: usize = vecdim.iter().map(|&k| d[k - 1]).product();
    let mut nd2 = vec![lead];
    nd2.extend(pd.iter().skip(vecdim.len()));
    if nd2.len() == 1 {
        nd2.push(1);
    }
    let mut out = Matrix::from_dims(nd2, data);
    out.im = im;
    out.kind = m.kind;
    let mut tmpl = d.clone();
    for &k in vecdim {
        tmpl[k - 1] = 1;
    }
    Ok((out, tmpl))
}

/// A dimension specification: a dimension, a vector of dimensions or 'all'.
enum DimSpec {
    Dim(usize),
    Vec(Vec<usize>),
    All,
    Default,
}

fn dim_spec(v: Option<&Value>) -> R<DimSpec> {
    match v {
        None => Ok(DimSpec::Default),
        Some(v) if lower(v).as_deref() == Some("all") => Ok(DimSpec::All),
        Some(Value::Num(m)) if m.kind != Kind::Char => {
            if m.data.iter().any(|&d| d < 1.0 || d.fract() != 0.0 || !d.is_finite()) || m.is_empty() {
                return rt("Dimension argument must be a positive integer scalar or a vector of unique positive integers.");
            }
            if m.numel() == 1 { Ok(DimSpec::Dim(m.data[0] as usize)) } else { Ok(DimSpec::Vec(m.data.iter().map(|&d| d as usize).collect())) }
        }
        Some(v) => rt(format!("Invalid dimension argument of class '{}'.", v.class_name())),
    }
}

/// Applies a reduction producing `k` values per vector along a dimension spec. Returns the
/// result and whether the vector-input orientation rule applies.
fn reduce_spec(m: &Matrix, spec: &DimSpec, k: usize, f: impl FnMut(&[f64], usize) -> Vec<f64>) -> R<Matrix> {
    match spec {
        DimSpec::Dim(d) => Ok(map_dim(m, *d, k, f)),
        DimSpec::Default => {
            if m.dims == [0, 0] {
                return Ok(map_dim(&m.clone().reshaped(vec![0, 1]), 1, k, f));
            }
            Ok(map_dim(m, default_dim(&m.dims), k, f))
        }
        DimSpec::All => {
            let col = m.clone().reshaped(vec![m.numel(), 1]);
            Ok(map_dim(&col, 1, k, f))
        }
        DimSpec::Vec(v) => {
            let (folded, tmpl) = fold_dims(m, v)?;
            let r = map_dim(&folded, 1, k, f);
            let mut dims = tmpl;
            dims[v[0] - 1] = k;
            if k == 1 {
                return Ok(r.reshaped(dims));
            }
            Ok(r.reshaped(dims))
        }
    }
}

/// MATLAB's exact percentile of sorted, NaN-free data at `p` percent.
fn percentile_sorted(x: &[f64], p: f64) -> f64 {
    let n = x.len();
    if n == 0 {
        return f64::NAN;
    }
    if n == 1 {
        return x[0];
    }
    let r = n as f64 * p / 100.0 + 0.5;
    if r < 1.0 {
        return x[0];
    }
    if r >= n as f64 {
        return x[n - 1];
    }
    let lo = r.floor();
    let f = r - lo;
    let (a, b) = (x[lo as usize - 1], x[lo as usize]);
    if f == 0.0 { a } else { a + f * (b - a) }
}

fn prctile(name: &str, a: &[Value]) -> R<Matrix> {
    let o = Opts::parse(a, &[], &["Method"])?;
    let x = num_arg(&o.pos, 0, name)?;
    if x.is_complex() {
        return rt("Input data must be real.");
    }
    let pm = num_arg(&o.pos, 1, name)?;
    let mut ps: Vec<f64> = pm.data.clone();
    let q = name == "quantile";
    if q && pm.numel() == 1 && pm.data[0] > 1.0 && pm.data[0].fract() == 0.0 {
        let n = pm.data[0];
        ps = (1..=n as usize).map(|k| 100.0 * k as f64 / (n + 1.0)).collect();
    } else if q {
        if pm.is_empty() || ps.iter().any(|&p| !(0.0..=1.0).contains(&p)) {
            return rt("Second argument must be a scalar or non-empty vector of real values in the interval [0,1].");
        }
        ps.iter_mut().for_each(|p| *p *= 100.0);
    } else if pm.is_empty() || ps.iter().any(|&p| !(0.0..=100.0).contains(&p)) {
        return rt("Second argument must be a scalar or non-empty vector of real values in the interval [0,100].");
    }
    if let Some(m) = o.get("Method") {
        let m = m.text()?.to_lowercase();
        if m != "exact" && m != "approximate" {
            return rt("Method must be 'exact' or 'approximate'.");
        }
    }
    let spec = dim_spec(o.pos.get(2))?;
    let k = ps.len();
    let mut r = reduce_spec(&x, &spec, k, |v, _| {
        let mut s: Vec<f64> = v.iter().copied().filter(|x| !x.is_nan()).collect();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ps.iter().map(|&p| percentile_sorted(&s, p)).collect()
    })?;
    // A vector input without a dimension: the result has the orientation of p.
    if matches!(spec, DimSpec::Default) && (x.is_vector() || x.dims == [0, 0] || x.is_scalar()) {
        let pd = if quantile_count_given(q, &pm) { vec![1, k] } else { pm.dims.clone() };
        let row = pd.len() == 2 && pd[0] == 1;
        r = r.reshaped(if row { vec![1, k] } else { vec![k, 1] });
    }
    Ok(r.cast_to(x.kind))
}

fn quantile_count_given(q: bool, pm: &Matrix) -> bool {
    q && pm.numel() == 1 && pm.data[0] > 1.0 && pm.data[0].fract() == 0.0
}

fn iqr(a: &[Value]) -> R<Matrix> {
    let x = num_arg(a, 0, "iqr")?;
    let spec = dim_spec(a.get(1))?;
    let r = reduce_spec(&x, &spec, 1, |v, _| {
        let mut s: Vec<f64> = v.iter().copied().filter(|x| !x.is_nan()).collect();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        vec![percentile_sorted(&s, 75.0) - percentile_sorted(&s, 25.0)]
    })?;
    Ok(r.cast_to(x.kind))
}

fn bounds(a: &[Value]) -> Ret {
    let o = Opts::parse(a, NANFLAGS, &[])?;
    let x = num_arg(&o.pos, 0, "bounds")?;
    let omit = omit_nan(&o, true);
    let spec = dim_spec(o.pos.get(1))?;
    if x.is_empty() && matches!(spec, DimSpec::Default) {
        let e = Value::Num(Matrix::empty().cast_to(x.kind));
        return Ok(Some(vec![e.clone(), e]));
    }
    let key = |m: &Matrix, i: usize| if m.is_complex() { c_at(m, i).abs() } else { m.data[i] };
    let keys = Matrix::from_dims(x.dims.clone(), (0..x.numel()).map(|i| key(&x, i)).collect());
    let positions = Matrix::from_dims(x.dims.clone(), (0..x.numel()).map(|i| i as f64).collect());
    let pos_r = reduce_spec(&positions, &spec, 2, |v, _| {
        let vals: Vec<f64> = v.iter().map(|&p| keys.data[p as usize]).collect();
        let has_nan = vals.iter().any(|x| x.is_nan());
        if has_nan && !omit {
            let p = v[vals.iter().position(|x| x.is_nan()).unwrap()];
            return vec![p, p];
        }
        let (mut lo, mut hi) = (None::<usize>, None::<usize>);
        for (k, &val) in vals.iter().enumerate() {
            if val.is_nan() {
                continue;
            }
            if lo.map(|l| val < vals[l]).unwrap_or(true) {
                lo = Some(k);
            }
            if hi.map(|h| val > vals[h]).unwrap_or(true) {
                hi = Some(k);
            }
        }
        match (lo, hi) {
            (Some(l), Some(h)) => vec![v[l], v[h]],
            _ => vec![-1.0, -1.0],
        }
    })?;
    // Split the interleaved (min, max) pairs along the reduced dimension.
    let d = match &spec {
        DimSpec::Dim(d) => *d,
        DimSpec::Vec(v) => v[0],
        DimSpec::All => 1,
        DimSpec::Default => default_dim(&x.dims),
    };
    let take = |which: usize| -> Matrix {
        let sel = map_dim(&pos_r, if matches!(spec, DimSpec::All) { 1 } else { d }, 1, |v, _| vec![v[which]]);
        let pos: Vec<f64> = sel.data.clone();
        let mut out = Matrix::from_dims(sel.dims.clone(), pos.iter().map(|&p| if p < 0.0 { f64::NAN } else { x.data[p as usize] }).collect());
        if x.is_complex() {
            out.im = Some(pos.iter().map(|&p| if p < 0.0 { 0.0 } else { x.im_at(p as usize) }).collect());
            out.narrow();
        }
        out.kind = x.kind;
        out
    };
    Ok(Some(vec![Value::Num(take(0)), Value::Num(take(1))]))
}

/// Sort key comparison for maxk/mink/topkrows: complex values by magnitude then angle, or by
/// the chosen ComparisonMethod.
fn cmp_key(method: &str, a: C, b: C) -> Ordering {
    let real = method == "real" || (method == "auto" && a.im == 0.0 && b.im == 0.0);
    if real {
        return cmp_nan_last(a.re, b.re).then_with(|| if method == "real" { cmp_nan_last(a.im, b.im) } else { Ordering::Equal });
    }
    cmp_nan_last(a.abs(), b.abs()).then_with(|| cmp_nan_last(a.arg(), b.arg()))
}

fn comparison_method(o: &Opts) -> R<String> {
    match o.get("ComparisonMethod") {
        None => Ok("auto".into()),
        Some(v) => {
            let m = v.text()?.to_lowercase();
            if !["auto", "real", "abs"].contains(&m.as_str()) {
                return rt("ComparisonMethod must be 'auto', 'real', or 'abs'.");
            }
            Ok(m)
        }
    }
}

fn maxk(name: &str, a: &[Value]) -> Ret {
    let o = Opts::parse(a, &[], &["ComparisonMethod"])?;
    let x = num_arg(&o.pos, 0, name)?;
    let kv = arg(&o.pos, 1, name)?.scalar()?;
    if kv < 0.0 || kv.fract() != 0.0 {
        return rt("Second argument must be a nonnegative integer scalar.");
    }
    let method = comparison_method(&o)?;
    let dim = dim_at(&o.pos, 2, &x.dims)?;
    let len = x.dims.get(dim - 1).copied().unwrap_or(1);
    let k = (kv as usize).min(len);
    let desc = name == "maxk";
    let positions = Matrix::from_dims(x.dims.clone(), (0..x.numel()).map(|i| i as f64).collect());
    let mut idx_out = Vec::new();
    let pos = map_dim(&positions, dim, k, |v, _| {
        let mut order: Vec<usize> = (0..v.len()).collect();
        order.sort_by(|&p, &q| {
            let (zp, zq) = (c_at(&x, v[p] as usize), c_at(&x, v[q] as usize));
            let (np, nq) = (is_nan_c(zp), is_nan_c(zq));
            match (np, nq) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => {
                    let c = cmp_key(&method, zp, zq);
                    if desc { c.reverse() } else { c }
                }
            }
        });
        order.truncate(k);
        idx_out.push(order.iter().map(|&j| (j + 1) as f64).collect::<Vec<_>>());
        order.iter().map(|&j| v[j]).collect()
    });
    let mut vals = x.gather(&pos.data.iter().map(|&p| p as usize).collect::<Vec<_>>(), pos.dims.clone());
    vals.kind = x.kind;
    let mut it = idx_out.into_iter();
    let idx = map_dim(&positions, dim, k, |_, _| it.next().unwrap_or_default());
    Ok(Some(vec![Value::Num(vals), Value::Num(idx)]))
}

/// Sorting columns for topkrows/issortedrows: (column, descending).
fn row_columns(spec: Option<&Value>, ncols: usize, default_desc: bool, names: Option<&[String]>) -> R<Vec<(usize, bool)>> {
    match spec {
        None => Ok((0..ncols).map(|c| (c, default_desc)).collect()),
        Some(v) => {
            if let (Some(names), Some(_)) = (names, lower(v)) {
                let t = v.text()?;
                let k = names.iter().position(|n| *n == t).ok_or_else(|| LangError::new(format!("Unrecognized table variable name '{t}'."), 0))?;
                return Ok(vec![(k, default_desc)]);
            }
            if let (Some(names), Value::Cell(c)) = (names, v) {
                return c
                    .data
                    .iter()
                    .map(|e| {
                        let t = e.text()?;
                        let k = names.iter().position(|n| *n == t).ok_or_else(|| LangError::new(format!("Unrecognized table variable name '{t}'."), 0))?;
                        Ok((k, default_desc))
                    })
                    .collect();
            }
            let m = v.to_matrix()?;
            m.data
                .iter()
                .map(|&c| {
                    if c == 0.0 || c.fract() != 0.0 || c.abs() as usize > ncols {
                        return rt("Column index must be a positive integer less than or equal to the number of columns.");
                    }
                    Ok(((c.abs() as usize) - 1, if c < 0.0 { !default_desc } else { default_desc }))
                })
                .collect()
        }
    }
}

/// Compares rows p and q of a numeric matrix by the given columns; NaN sorts as the largest
/// value unless placed otherwise.
fn cmp_rows(m: &Matrix, cols: &[(usize, bool)], p: usize, q: usize, method: &str, nan_last_always: bool) -> Ordering {
    let r = m.rows();
    for &(c, desc) in cols {
        let (a, b) = (c_at(m, c * r + p), c_at(m, c * r + q));
        let (na, nb) = (is_nan_c(a), is_nan_c(b));
        let o = match (na, nb) {
            (true, true) => Ordering::Equal,
            (true, false) => {
                if nan_last_always || !desc {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
            (false, true) => {
                if nan_last_always || !desc {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            _ => {
                let o = cmp_key(method, a, b);
                if desc { o.reverse() } else { o }
            }
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

fn topkrows(a: &[Value]) -> Ret {
    let o = Opts::parse(a, &["ascend", "descend"], &["ComparisonMethod"])?;
    let method = comparison_method(&o)?;
    let src = arg(&o.pos, 0, "topkrows")?.clone();
    let kv = arg(&o.pos, 1, "topkrows")?.scalar()?;
    if kv < 0.0 || kv.fract() != 0.0 {
        return rt("Second argument must be a nonnegative integer scalar.");
    }
    let desc = o.last_of(&["ascend", "descend"]) != Some("ascend");
    let (m, names) = match &src {
        Value::Table(t) => {
            let cols: Vec<Matrix> = t.vars.iter().map(|v| v.to_matrix()).collect::<R<_>>()?;
            let rows = t.nrows;
            let mut data = Vec::new();
            for c in &cols {
                data.extend_from_slice(&c.data[..rows.min(c.data.len())]);
            }
            (Matrix::from_dims(vec![rows, cols.len()], data), Some(t.names.clone()))
        }
        _ => (src.to_matrix()?, None),
    };
    if let Some(Value::Num(c)) = o.pos.get(2) {
        if c.data.iter().any(|&v| v < 1.0 || v.fract() != 0.0 || v > m.cols() as f64) {
            return rt("Column sorting vector must contain positive integers between 1 and the number of columns in the first argument.");
        }
    }
    let cols = row_columns(o.pos.get(2), m.cols(), desc, names.as_deref())?;
    let n = m.rows();
    let k = (kv as usize).min(n);
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&p, &q| cmp_rows(&m, &cols, p, q, &method, true));
    order.truncate(k);
    let rows = IndexArg::List { ix: order.clone(), dims: vec![k, 1], from_mask: false };
    let out = match &src {
        Value::Table(t) => Value::Table(Box::new(t.paren(&[Value::Num(Matrix::col(order.iter().map(|&i| (i + 1) as f64).collect())), Value::chars(":")])?)),
        v => indexing::index(v, &[rows, IndexArg::All])?,
    };
    Ok(Some(vec![out, Value::Num(Matrix::col(order.iter().map(|&i| (i + 1) as f64).collect()))]))
}

fn issortedrows(a: &[Value]) -> Ret {
    let o = Opts::parse(a, &["ascend", "descend", "monotonic", "strictascend", "strictdescend", "strictmonotonic"], &["ComparisonMethod", "MissingPlacement"])?;
    let method = comparison_method(&o)?;
    let src = arg(&o.pos, 0, "issortedrows")?;
    let m = src.to_matrix()?;
    let dir = o.last_of(&["ascend", "descend", "monotonic", "strictascend", "strictdescend", "strictmonotonic"]).unwrap_or("ascend").to_string();
    let n = m.rows();
    let check = |desc: bool, strict: bool| -> R<bool> {
        let cols = row_columns(o.pos.get(1), m.cols(), desc, None)?;
        Ok((1..n).all(|i| {
            let c = cmp_rows(&m, &cols, i - 1, i, &method, false);
            c == Ordering::Less || (!strict && c == Ordering::Equal)
        }))
    };
    let r = match dir.as_str() {
        "ascend" => check(false, false)?,
        "descend" => check(true, false)?,
        "strictascend" => check(false, true)?,
        "strictdescend" => check(true, true)?,
        "monotonic" => check(false, false)? || check(true, false)?,
        _ => check(false, true)? || check(true, true)?,
    };
    one(Value::bool(r))
}

// ---- numerical derivatives --------------------------------------------------------------------

/// Spacing along one dimension: a scalar step or coordinates.
fn spacing(v: Option<&Value>, n: usize) -> R<Vec<f64>> {
    match v {
        None => Ok((0..n).map(|i| i as f64).collect()),
        Some(v) => {
            let m = v.to_matrix()?;
            if m.numel() == 1 {
                Ok((0..n).map(|i| i as f64 * m.data[0]).collect())
            } else if m.numel() == n {
                Ok(m.data)
            } else {
                rt("The spacing values must be a scalar or a vector with the same length as the corresponding dimension.")
            }
        }
    }
}

fn gradient_vec(f: &[f64], h: Option<f64>, x: &[f64]) -> Vec<f64> {
    let n = f.len();
    if n < 2 {
        return vec![0.0; n];
    }
    let mut g = vec![0.0; n];
    match h {
        Some(h) => {
            g[0] = (f[1] - f[0]) / h;
            g[n - 1] = (f[n - 1] - f[n - 2]) / h;
            for i in 1..n - 1 {
                g[i] = (f[i + 1] - f[i - 1]) / (2.0 * h);
            }
        }
        None => {
            g[0] = (f[1] - f[0]) / (x[1] - x[0]);
            g[n - 1] = (f[n - 1] - f[n - 2]) / (x[n - 1] - x[n - 2]);
            for i in 1..n - 1 {
                g[i] = (f[i + 1] - f[i - 1]) / (x[i + 1] - x[i - 1]);
            }
        }
    }
    g
}

/// Matrix dimension order of gradient/del2 outputs and spacing arguments: x (dim 2), y (dim 1),
/// then z, ...
fn xy_order(nd_: usize) -> Vec<usize> {
    let mut v = vec![2, 1];
    v.extend(3..=nd_);
    v
}

fn spacing_args(f: &Matrix, h: &[Value], dims: &[usize]) -> R<Vec<(Option<f64>, Vec<f64>)>> {
    dims.iter()
        .enumerate()
        .map(|(k, &d)| {
            let n = f.dims.get(d - 1).copied().unwrap_or(1);
            let v = if h.len() == 1 { h.first() } else { h.get(k) };
            let scalar = match v {
                None => Some(1.0),
                Some(v) if v.numel() == 1 => Some(v.scalar()?),
                _ => None,
            };
            Ok((scalar, spacing(v, n)?))
        })
        .collect()
}

fn gradient(a: &[Value], nargout: usize) -> Ret {
    let f = num_arg(a, 0, "gradient")?;
    let kind = float_kind(f.kind);
    let h = &a[1..];
    if f.is_vector() {
        let dim = default_dim(&f.dims);
        let sp = spacing_args(&f, &h[..h.len().min(1)], &[dim])?;
        let (s, x) = &sp[0];
        let g = map_dim(&f, dim, f.numel(), |v, _| gradient_vec(v, *s, x));
        return one(Value::Num(g.cast_to(kind)));
    }
    let dims = xy_order(f.dims.len());
    if h.len() > 1 && h.len() != dims.len() {
        return rt("The number of spacing values must equal the number of dimensions of the input.");
    }
    let sp = spacing_args(&f, h, &dims)?;
    let outs = nargout.max(1).min(dims.len());
    let mut res = Vec::with_capacity(outs);
    for (k, &d) in dims.iter().enumerate().take(outs) {
        let (s, x) = &sp[k];
        let n = f.dims.get(d - 1).copied().unwrap_or(1);
        res.push(Value::Num(map_dim(&f, d, n, |v, _| gradient_vec(v, *s, x)).cast_to(kind)));
    }
    Ok(Some(res))
}

/// Second differences along one vector with linear extrapolation at the ends.
fn second_diff(u: &[f64], x: &[f64]) -> Vec<f64> {
    let n = u.len();
    if n < 3 {
        return vec![0.0; n];
    }
    let mut d = vec![0.0; n];
    for i in 1..n - 1 {
        let (h0, h1) = (x[i] - x[i - 1], x[i + 1] - x[i]);
        d[i] = ((u[i + 1] - u[i]) / h1 - (u[i] - u[i - 1]) / h0) / ((h0 + h1) / 2.0);
    }
    if n == 3 {
        return vec![d[1]; 3];
    }
    let (x0, x1, x2) = (x[0], x[1], x[2]);
    d[0] = d[1] + (d[1] - d[2]) * (x1 - x0) / (x2 - x1);
    let (y0, y1, y2) = (x[n - 1], x[n - 2], x[n - 3]);
    d[n - 1] = d[n - 2] + (d[n - 2] - d[n - 3]) * (y0 - y1) / (y1 - y2);
    d
}

fn del2(a: &[Value]) -> R<Matrix> {
    let u = num_arg(a, 0, "del2")?;
    let kind = float_kind(u.kind);
    let h = &a[1..];
    let mut acc = Matrix::from_dims(u.dims.clone(), vec![0.0; u.numel()]);
    let (dims, sp) = if u.is_vector() {
        let d = default_dim(&u.dims);
        (vec![d], spacing_args(&u, &h[..h.len().min(1)], &[d])?)
    } else {
        let dims = xy_order(u.dims.len());
        if h.len() > 1 && h.len() != dims.len() {
            return rt("The number of spacing values must equal the number of dimensions of the input.");
        }
        let sp = spacing_args(&u, h, &dims)?;
        (dims, sp)
    };
    for (k, &d) in dims.iter().enumerate() {
        let n = u.dims.get(d - 1).copied().unwrap_or(1);
        let x = &sp[k].1;
        let dd = map_dim(&u, d, n, |v, _| second_diff(v, x));
        for (p, q) in acc.data.iter_mut().zip(&dd.data) {
            *p += q;
        }
    }
    let nd_ = u.dims.len().max(2) as f64;
    acc.data.iter_mut().for_each(|v| *v /= 2.0 * nd_);
    Ok(acc.cast_to(kind))
}
// ---- dispatch ----------------------------------------------------------------------------------

fn mat(m: Matrix) -> Ret {
    one(Value::Num(m))
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    let _ = &it;
    match name {
        "fft" | "ifft" => mat(fft1(name, a)?),
        "fft2" | "ifft2" | "fftn" | "ifftn" => mat(fft_nd(name, a)?),
        "fftshift" | "ifftshift" => fftshift(name, a),
        "fftw" => fftw(a),
        "nufft" => mat(nufft(a)?),
        "nufftn" => mat(nufftn(a)?),
        "filter" => filter(a, nargout),
        "conv2" => mat(conv2(a)?),
        "convn" => mat(convn(a)?),
        "filter2" => mat(filter2(a)?),
        "detrend" => mat(detrend(a)?),
        "cov" => mat(cov(a)?),
        "corrcoef" => corrcoef(a, nargout),
        "xcorr" | "xcov" => xcorr(name, a),
        "movsum" | "movprod" | "movmin" | "movmax" | "movmean" | "movmedian" | "movstd" | "movvar" | "movmad" => mat(moving(name, a)?),
        "prctile" | "quantile" => mat(prctile(name, a)?),
        "iqr" => mat(iqr(a)?),
        "bounds" => bounds(a),
        "maxk" | "mink" => maxk(name, a),
        "topkrows" => topkrows(a),
        "issortedrows" => issortedrows(a),
        "gradient" => gradient(a, nargout),
        "del2" => mat(del2(a)?),
        _ => rt(format!("'{name}' is not supported in this version.")),
    }
}
