//! MATLAB's `toolbox/matlab/polyfun` outside geometry: polynomial arithmetic (`conv`, `deconv`,
//! `polyder`, `polyint`, `polyvalm`), piecewise polynomials (`mkpp`, `unmkpp`, `ppval`) and the
//! 1-D interpolants built on them (`spline`, `pchip`, `makima`), `interp1` with all of its
//! methods, `interpft`, gridded interpolation (`interp2`, `interp3`, `interpn`) and the polygon
//! functions `polyarea`, `inpolygon` and `rectint`.
//!
//! Every 1-D method is a cubic (or lower) Hermite form defined by node slopes, so `spline`,
//! `pchip` and `makima` only differ in how they choose the slopes.

use crate::builtins::{m_arg, one, Ret};
use crate::cplx::C;
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::nd::{self, Dims};
use crate::ops;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "conv", "deconv", "polyder", "polyint", "polyvalm", "mkpp", "unmkpp", "ppval", "spline", "pchip", "makima", "interp1", "interpft", "interp2",
    "interp3", "interpn", "polyarea", "inpolygon", "rectint",
];

// ---- small helpers ---------------------------------------------------------------------------

fn complex_of(m: &Matrix) -> Vec<C> {
    (0..m.numel()).map(|i| C::new(m.data[i], m.im_at(i))).collect()
}

fn from_complex(dims: Dims, z: &[C]) -> Matrix {
    Matrix::complex(dims, z.iter().map(|c| c.re).collect(), z.iter().map(|c| c.im).collect())
}

fn is_single(m: &Matrix) -> bool {
    m.kind == Kind::Single
}

/// Rounds to single when any input was single (MATLAB's float result rule).
fn float_result(m: Matrix, single: bool) -> Matrix {
    if single { m.cast_to(Kind::Single) } else { m }
}

fn is_column(m: &Matrix) -> bool {
    m.is_2d() && m.dims[1] == 1 && m.dims[0] > 1
}

/// `v` as a row or column vector of the given orientation.
fn oriented(data: Vec<C>, column: bool) -> Matrix {
    let n = data.len();
    let dims = if column { vec![n, 1] } else { vec![1, n] };
    from_complex(dims, &data)
}

fn conv_c(a: &[C], b: &[C]) -> Vec<C> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut out = vec![C::new(0.0, 0.0); a.len() + b.len() - 1];
    for (i, &x) in a.iter().enumerate() {
        for (j, &y) in b.iter().enumerate() {
            out[i + j] = out[i + j].add(x.mul(y));
        }
    }
    out
}

/// Drops leading zero coefficients, keeping at least one.
fn strip_leading(mut p: Vec<C>) -> Vec<C> {
    let k = p.iter().position(|c| c.re != 0.0 || c.im != 0.0).unwrap_or(p.len().saturating_sub(1));
    p.drain(..k.min(p.len()));
    if p.is_empty() {
        p.push(C::new(0.0, 0.0));
    }
    p
}

/// `a + b` for coefficient vectors aligned at their ends.
fn poly_add(a: &[C], b: &[C], sign: f64) -> Vec<C> {
    let n = a.len().max(b.len());
    let mut out = vec![C::new(0.0, 0.0); n];
    for (i, &x) in a.iter().enumerate() {
        out[n - a.len() + i] = x;
    }
    for (i, &y) in b.iter().enumerate() {
        let k = n - b.len() + i;
        out[k] = out[k].add(C::new(sign * y.re, sign * y.im));
    }
    out
}

fn derivative(p: &[C]) -> Vec<C> {
    let n = p.len();
    if n < 2 {
        return vec![C::new(0.0, 0.0)];
    }
    (0..n - 1).map(|i| C::new(p[i].re * (n - 1 - i) as f64, p[i].im * (n - 1 - i) as f64)).collect()
}

fn poly_row(z: &[C]) -> Matrix {
    oriented(z.to_vec(), false)
}

fn vector_check(m: &Matrix) -> bool {
    m.is_2d() && (m.dims[0] == 1 || m.dims[1] == 1) && !m.is_empty()
}

// ---- polynomials -----------------------------------------------------------------------------

fn conv(a: &[Value]) -> Ret {
    let u = m_arg(a, 0, "conv")?;
    let v = m_arg(a, 1, "conv")?;
    if !vector_check(&u) || !vector_check(&v) {
        return rt("A and B must be vectors.");
    }
    let shape = match a.get(2) {
        Some(s) => s.text()?.to_ascii_lowercase(),
        None => "full".into(),
    };
    let full = conv_c(&complex_of(&u), &complex_of(&v));
    let (nu, nv) = (u.numel(), v.numel());
    let part = match shape.as_str() {
        "full" => full,
        "same" => full[nv / 2..nv / 2 + nu].to_vec(),
        "valid" => {
            if nu >= nv {
                full[nv - 1..nu].to_vec()
            } else {
                Vec::new()
            }
        }
        _ => return rt("SHAPE must be 'full', 'same', or 'valid'."),
    };
    let column = if nu > 1 { is_column(&u) } else { is_column(&v) };
    one(Value::Num(float_result(oriented(part, column), is_single(&u) || is_single(&v))))
}

fn deconv(a: &[Value]) -> Ret {
    let u = m_arg(a, 0, "deconv")?;
    let v = m_arg(a, 1, "deconv")?;
    if !vector_check(&u) || !vector_check(&v) {
        return rt("A and B must be vectors.");
    }
    let (uc, vc) = (complex_of(&u), complex_of(&v));
    if vc[0].re == 0.0 && vc[0].im == 0.0 {
        return rt("First coefficient of A must be non-zero when the deconvolution method is \"long-division\".");
    }
    let single = is_single(&u) || is_single(&v);
    let column = is_column(&u);
    if vc.len() > uc.len() {
        let q = float_result(Matrix::scalar(0.0), single);
        return Ok(Some(vec![Value::Num(q), Value::Num(float_result(u.clone(), single))]));
    }
    let nq = uc.len() - vc.len() + 1;
    let mut q = vec![C::new(0.0, 0.0); nq];
    for k in 0..nq {
        let mut acc = uc[k];
        for j in 1..vc.len().min(k + 1) {
            acc = acc.sub(vc[j].mul(q[k - j]));
        }
        q[k] = acc.div(vc[0]);
    }
    let back = conv_c(&vc, &q);
    let r: Vec<C> = uc.iter().zip(&back).map(|(x, y)| x.sub(*y)).collect();
    let mut rm = from_complex(u.dims.clone(), &r);
    rm.narrow();
    Ok(Some(vec![Value::Num(float_result(oriented(q, column), single)), Value::Num(float_result(rm, single))]))
}

fn polyder(a: &[Value], nargout: usize) -> Ret {
    let p = complex_of(&m_arg(a, 0, "polyder")?);
    if a.len() == 1 {
        return one(Value::Num(poly_row(&strip_leading(derivative(&strip_leading(p))))));
    }
    let b = p;
    let q = complex_of(&m_arg(a, 1, "polyder")?);
    // MATLAB's two-argument form: d(b*a)/dx with one output, (a*b' - b*a')/a^2 with two.
    if nargout < 2 {
        let s = poly_add(&conv_c(&derivative(&b), &q), &conv_c(&b, &derivative(&q)), 1.0);
        return one(Value::Num(poly_row(&strip_leading(s))));
    }
    let num = poly_add(&conv_c(&q, &derivative(&b)), &conv_c(&b, &derivative(&q)), -1.0);
    let den = conv_c(&q, &q);
    Ok(Some(vec![Value::Num(poly_row(&strip_leading(num))), Value::Num(poly_row(&strip_leading(den)))]))
}

fn polyint(a: &[Value]) -> Ret {
    let p = m_arg(a, 0, "polyint")?;
    let k = match a.get(1) {
        Some(v) => complex_of(&v.to_matrix()?),
        None => vec![C::new(0.0, 0.0)],
    };
    if p.numel() > 1 && !(p.is_2d() && p.dims[0] == 1) {
        return rt("Dimensions of arrays being concatenated are not consistent.");
    }
    let pc = complex_of(&p);
    let n = pc.len();
    let mut out: Vec<C> = pc.iter().enumerate().map(|(i, c)| C::new(c.re / (n - i) as f64, c.im / (n - i) as f64)).collect();
    out.extend(k);
    one(Value::Num(float_result(poly_row(&out), is_single(&p))))
}

fn polyvalm(a: &[Value]) -> Ret {
    let p = m_arg(a, 0, "polyvalm")?;
    let x = m_arg(a, 1, "polyvalm")?;
    if !x.is_2d() || x.dims[0] != x.dims[1] {
        return rt("Matrix must be square.");
    }
    let n = x.dims[0];
    let eye = ops::identity(n);
    let mut y = Matrix::zeros(n, n);
    for i in 0..p.numel() {
        let c = Matrix::complex(vec![1, 1], vec![p.data[i]], vec![p.im_at(i)]);
        let prod = if i == 0 { y } else { ops::matmul_any(&y, &x)? };
        y = ops::num_binary(crate::ast::BinOp::Add, &prod, &ops::num_binary(crate::ast::BinOp::Mul, &c, &eye)?)?;
    }
    y.narrow();
    one(Value::Num(y))
}

// ---- piecewise polynomials -------------------------------------------------------------------

/// A piecewise polynomial: `coefs` has `pieces * d` rows of `order` coefficients (row
/// `piece * d + component`, highest power first), as MATLAB's pp-form stores them.
#[derive(Clone, Debug)]
pub(crate) struct Pp {
    pub breaks: Vec<f64>,
    pub coefs: Vec<Vec<f64>>,
    pub dim: Vec<usize>,
    pub order: usize,
}

impl Pp {
    fn d(&self) -> usize {
        self.dim.iter().product()
    }

    fn pieces(&self) -> usize {
        self.breaks.len() - 1
    }

    /// Piece containing `x` (the end pieces extend to ±Inf).
    fn piece(&self, x: f64) -> usize {
        let l = self.pieces();
        if l <= 1 {
            return 0;
        }
        // Breaks 2..l, the interior ones.
        self.breaks[1..l].partition_point(|&b| b <= x)
    }

    /// Value of component `c` at `x`.
    pub fn eval(&self, c: usize, x: f64) -> f64 {
        if x.is_nan() {
            return f64::NAN;
        }
        let k = self.piece(x);
        let t = x - self.breaks[k];
        let row = &self.coefs[k * self.d() + c];
        row[1..].iter().fold(row[0], |acc, &a| acc * t + a)
    }

    pub fn to_value(&self, orient: bool) -> Value {
        let mut s = StructArray::scalar();
        s.set("form", Value::chars("pp"));
        s.set("breaks", Value::Num(Matrix::row(self.breaks.clone())));
        let rows = self.coefs.len();
        let mut data = vec![0.0; rows * self.order];
        for (r, row) in self.coefs.iter().enumerate() {
            for (c, &v) in row.iter().enumerate() {
                data[c * rows + r] = v;
            }
        }
        s.set("coefs", Value::Num(Matrix::new(rows, self.order, data)));
        s.set("pieces", Value::num(self.pieces() as f64));
        s.set("order", Value::num(self.order as f64));
        s.set("dim", Value::Num(Matrix::row(self.dim.iter().map(|&d| d as f64).collect())));
        if orient {
            s.set("orient", Value::chars("first"));
        }
        Value::Struct(s)
    }

    fn from_value(v: &Value) -> R<Pp> {
        let Value::Struct(s) = v else { return rt("PP must be a piecewise polynomial structure.") };
        let get = |f: &str| -> R<Matrix> {
            match s.get(f) {
                Some(v) => v.to_matrix(),
                None => rt("PP must be a piecewise polynomial structure."),
            }
        };
        if s.get("form").map(|f| f.text().ok()) != Some(Some("pp".into())) {
            return rt("PP must be a piecewise polynomial structure.");
        }
        let breaks = get("breaks")?.data;
        let coefs = get("coefs")?;
        let order = get("order")?.data.first().copied().unwrap_or(0.0) as usize;
        let dim: Vec<usize> = get("dim")?.data.iter().map(|&d| d as usize).collect();
        let rows = coefs.numel() / order.max(1);
        let coefs = (0..rows).map(|r| (0..order).map(|c| coefs.data[c * rows + r]).collect()).collect();
        Ok(Pp { breaks, coefs, dim, order })
    }

    /// Evaluates at every point of `xx`, shaping the result as `ppval` does.
    fn eval_at(&self, xx: &Matrix) -> Matrix {
        let d = self.d();
        let mut data = Vec::with_capacity(d * xx.numel());
        for &x in &xx.data {
            for c in 0..d {
                data.push(self.eval(c, x));
            }
        }
        let mut dims = if self.dim.len() == 1 && self.dim[0] == 1 { Vec::new() } else { self.dim.clone() };
        if xx.is_vector() && !dims.is_empty() {
            dims.push(xx.numel());
        } else if dims.is_empty() {
            dims = xx.dims.clone();
        } else {
            dims.extend(xx.dims.iter().copied());
        }
        if dims.len() == 1 {
            dims.push(1);
        }
        Matrix::from_dims(dims, data)
    }
}

/// Coefficients of the cubic Hermite piece on `[x0, x0 + h]` with end values and slopes.
fn hermite(h: f64, y0: f64, y1: f64, s0: f64, s1: f64) -> Vec<f64> {
    let del = (y1 - y0) / h;
    vec![(s0 - 2.0 * del + s1) / (h * h), (3.0 * del - 2.0 * s0 - s1) / h, s0, y0]
}

/// Cubic Hermite pp from node values `ys[c][i]` and slopes `ss[c][i]`.
fn hermite_pp(x: &[f64], ys: &[Vec<f64>], ss: &[Vec<f64>], dim: Vec<usize>) -> Pp {
    let mut coefs = Vec::new();
    for k in 0..x.len() - 1 {
        let h = x[k + 1] - x[k];
        for c in 0..ys.len() {
            coefs.push(hermite(h, ys[c][k], ys[c][k + 1], ss[c][k], ss[c][k + 1]));
        }
    }
    Pp { breaks: x.to_vec(), coefs, dim, order: 4 }
}

fn slopes_of(x: &[f64], y: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let h: Vec<f64> = x.windows(2).map(|w| w[1] - w[0]).collect();
    let del = y.windows(2).zip(&h).map(|(w, &hk)| (w[1] - w[0]) / hk).collect();
    (h, del)
}

/// Solves a tridiagonal system (sub, diag, sup) in place by elimination without pivoting.
fn tridiag(sub: &[f64], diag: &mut [f64], sup: &[f64], rhs: &mut [f64]) {
    let n = diag.len();
    for i in 1..n {
        let m = sub[i] / diag[i - 1];
        diag[i] -= m * sup[i - 1];
        rhs[i] -= m * rhs[i - 1];
    }
    rhs[n - 1] /= diag[n - 1];
    for i in (0..n - 1).rev() {
        rhs[i] = (rhs[i] - sup[i] * rhs[i + 1]) / diag[i];
    }
}

/// Node slopes of the cubic spline: not-a-knot ends, or clamped when `ends` gives them.
fn spline_slopes(x: &[f64], y: &[f64], ends: Option<(f64, f64)>) -> Vec<f64> {
    let n = x.len();
    let (h, del) = slopes_of(x, y);
    let mut sub = vec![0.0; n];
    let mut diag = vec![0.0; n];
    let mut sup = vec![0.0; n];
    let mut rhs = vec![0.0; n];
    for k in 1..n - 1 {
        sub[k] = h[k];
        diag[k] = 2.0 * (h[k - 1] + h[k]);
        sup[k] = h[k - 1];
        rhs[k] = 3.0 * (h[k] * del[k - 1] + h[k - 1] * del[k]);
    }
    match ends {
        Some((a, b)) => {
            diag[0] = 1.0;
            rhs[0] = a;
            diag[n - 1] = 1.0;
            rhs[n - 1] = b;
        }
        None => {
            diag[0] = h[1];
            sup[0] = h[0] + h[1];
            rhs[0] = ((h[0] + 2.0 * (h[0] + h[1])) * h[1] * del[0] + h[0] * h[0] * del[1]) / (h[0] + h[1]);
            let (a, b) = (h[n - 2], h[n - 3]);
            sub[n - 1] = a + b;
            diag[n - 1] = b;
            rhs[n - 1] = (a * a * del[n - 3] + (2.0 * (b + a) + a) * b * del[n - 2]) / (b + a);
        }
    }
    tridiag(&sub, &mut diag, &sup, &mut rhs);
    rhs
}

/// The interpolating spline of `ys` (components × nodes; `n + 2` columns for clamped ends).
fn spline_pp(x: &[f64], ys: &[Vec<f64>], dim: Vec<usize>) -> Pp {
    let n = x.len();
    let clamped = ys[0].len() == n + 2;
    if !clamped && n == 2 {
        let coefs = ys.iter().map(|y| vec![(y[1] - y[0]) / (x[1] - x[0]), y[0]]).collect();
        return Pp { breaks: x.to_vec(), coefs, dim, order: 2 };
    }
    if !clamped && n == 3 {
        // Not-a-knot with three points is the interpolating parabola, as one piece.
        let coefs = ys
            .iter()
            .map(|y| {
                let (h, del) = slopes_of(x, y);
                let c2 = (del[1] - del[0]) / (h[0] + h[1]);
                vec![c2, del[0] - c2 * h[0], y[0]]
            })
            .collect();
        return Pp { breaks: vec![x[0], x[2]], coefs, dim, order: 3 };
    }
    let mut vals = Vec::new();
    let mut ss = Vec::new();
    for y in ys {
        if clamped {
            let inner = y[1..=n].to_vec();
            ss.push(spline_slopes(x, &inner, Some((y[0], y[n + 1]))));
            vals.push(inner);
        } else {
            ss.push(spline_slopes(x, y, None));
            vals.push(y.clone());
        }
    }
    hermite_pp(x, &vals, &ss, dim)
}

fn sign(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Shape-preserving end slope of `pchip` from the two nearest intervals.
fn pchip_end(h0: f64, h1: f64, d0: f64, d1: f64) -> f64 {
    let s = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
    if sign(s) != sign(d0) {
        0.0
    } else if sign(d0) != sign(d1) && s.abs() > (3.0 * d0).abs() {
        3.0 * d0
    } else {
        s
    }
}

fn pchip_slopes(x: &[f64], y: &[f64]) -> Vec<f64> {
    let n = x.len();
    let (h, del) = slopes_of(x, y);
    if n == 2 {
        return vec![del[0], del[0]];
    }
    let mut s = vec![0.0; n];
    for k in 1..n - 1 {
        if sign(del[k - 1]) * sign(del[k]) > 0.0 {
            let w1 = 2.0 * h[k] + h[k - 1];
            let w2 = h[k] + 2.0 * h[k - 1];
            s[k] = (w1 + w2) / (w1 / del[k - 1] + w2 / del[k]);
        }
    }
    s[0] = pchip_end(h[0], h[1], del[0], del[1]);
    s[n - 1] = pchip_end(h[n - 2], h[n - 3], del[n - 2], del[n - 3]);
    s
}

pub(crate) fn makima_slopes(x: &[f64], y: &[f64]) -> Vec<f64> {
    let n = x.len();
    let (_, del) = slopes_of(x, y);
    if n == 2 {
        return vec![del[0], del[0]];
    }
    // Two extra slopes at each end, extrapolated linearly.
    let mut e = Vec::with_capacity(n + 3);
    let d0 = 2.0 * del[0] - del[1];
    e.push(2.0 * d0 - del[0]);
    e.push(d0);
    e.extend(&del);
    let dn = 2.0 * del[n - 2] - del[n - 3];
    e.push(dn);
    e.push(2.0 * dn - del[n - 2]);
    // Node k uses e[k..k+4] = delta_{k-2} .. delta_{k+1}.
    (0..n)
        .map(|k| {
            let (a, b, c, d) = (e[k], e[k + 1], e[k + 2], e[k + 3]);
            let w1 = (d - c).abs() + (d + c).abs() / 2.0;
            let w2 = (b - a).abs() + (b + a).abs() / 2.0;
            if w1 + w2 == 0.0 {
                (b + c) / 2.0
            } else {
                (w1 * b + w2 * c) / (w1 + w2)
            }
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Method {
    Linear,
    Nearest,
    Next,
    Previous,
    Pchip,
    Cubic,
    Makima,
    Spline,
}

impl Method {
    fn parse(s: &str) -> Option<Method> {
        let s = s.trim_start_matches('*').to_ascii_lowercase();
        Some(match s.as_str() {
            "linear" => Method::Linear,
            "nearest" => Method::Nearest,
            "next" => Method::Next,
            "previous" => Method::Previous,
            "pchip" => Method::Pchip,
            "cubic" | "v5cubic" => Method::Cubic,
            "makima" => Method::Makima,
            "spline" => Method::Spline,
            _ => return None,
        })
    }

    pub(crate) fn extrapolates(self) -> bool {
        matches!(self, Method::Pchip | Method::Makima | Method::Spline)
    }
}

/// pp of one of the cubic methods for several components.
fn cubic_pp(method: Method, x: &[f64], ys: &[Vec<f64>], dim: Vec<usize>) -> Pp {
    match method {
        Method::Spline => spline_pp(x, ys, dim),
        Method::Pchip => hermite_pp(x, ys, &ys.iter().map(|y| pchip_slopes(x, y)).collect::<Vec<_>>(), dim),
        _ => hermite_pp(x, ys, &ys.iter().map(|y| makima_slopes(x, y)).collect::<Vec<_>>(), dim),
    }
}

/// Validates and sorts sample points as `spline`, `pchip` and `makima` do; returns the sorted
/// sites and, per component, the values in the same order.
fn chckxy(x: &Matrix, y: &Matrix, allow_ends: bool) -> R<(Vec<f64>, Vec<Vec<f64>>, Vec<usize>)> {
    let n = x.numel();
    if n < 2 {
        return rt("The first two inputs must have at least two elements.");
    }
    // y's last dimension runs along x; the others form the value dims.
    let (dim, m) = if y.is_vector() || y.is_scalar() {
        (vec![1], y.numel())
    } else {
        let last = *y.dims.last().unwrap();
        (y.dims[..y.dims.len() - 1].to_vec(), last)
    };
    if m != n && !(allow_ends && m == n + 2) {
        return rt(format!("The number of sample points X, {n}, is incompatible with the number of values Y, {m}."));
    }
    let d: usize = dim.iter().product();
    let mut ys: Vec<Vec<f64>> = (0..d).map(|c| (0..m).map(|i| y.data[i * d + c]).collect()).collect();
    let mut xs = x.data.clone();
    let off = if m == n + 2 { 1 } else { 0 };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| xs[a].total_cmp(&xs[b]));
    if order.iter().enumerate().any(|(i, &o)| i != o) {
        xs = order.iter().map(|&o| x.data[o]).collect();
        for yc in ys.iter_mut() {
            let mut out = yc.clone();
            for (i, &o) in order.iter().enumerate() {
                out[i + off] = yc[o + off];
            }
            *yc = out;
        }
    }
    if xs.windows(2).any(|w| w[0] == w[1]) {
        return rt("The first input must contain unique values.");
    }
    Ok((xs, ys, dim))
}

/// `spline`, `pchip` and `makima`: the pp form with two arguments, its values with three.
fn cubic_fn(it: &mut Interpreter, name: &str, a: &[Value]) -> Ret {
    let x = m_arg(a, 0, name)?;
    let y = m_arg(a, 1, name)?;
    let method = match name {
        "spline" => Method::Spline,
        "pchip" => Method::Pchip,
        _ => Method::Makima,
    };
    let single = is_single(&x) || is_single(&y);
    // NaN data points are dropped (spline warns about it).
    let (x, y) = if y.is_vector() && x.numel() == y.numel() && (x.data.iter().any(|v| v.is_nan()) || y.data.iter().any(|v| v.is_nan())) {
        let keep: Vec<usize> = (0..x.numel()).filter(|&i| !x.data[i].is_nan() && !y.data[i].is_nan()).collect();
        it.emit(OutputKind::Warning("All NaN entries in the first and second input were ignored.".into()));
        (Matrix::row(keep.iter().map(|&i| x.data[i]).collect()), y.gather(&keep, vec![1, keep.len()]))
    } else {
        (x, y)
    };
    let complex = y.is_complex();
    let (xs, ys, dim) = chckxy(&x, &y, method == Method::Spline)?;
    let ims = if complex {
        let yi = Matrix::from_dims(y.dims.clone(), y.im.clone().unwrap());
        Some(chckxy(&x, &yi, method == Method::Spline)?.1)
    } else {
        None
    };
    let pp = cubic_pp(method, &xs, &ys, dim.clone());
    let Some(xx) = a.get(2) else {
        return one(pp.to_value(false));
    };
    let xx = xx.to_matrix()?;
    let mut out = pp.eval_at(&xx);
    if let Some(ims) = ims {
        let pi = cubic_pp(method, &xs, &ims, dim);
        out.im = Some(pi.eval_at(&xx).data);
        out.narrow();
    }
    one(Value::Num(float_result(out, single || is_single(&xx))))
}

fn mkpp(a: &[Value]) -> Ret {
    let breaks = m_arg(a, 0, "mkpp")?.data;
    let coefs = m_arg(a, 1, "mkpp")?;
    let dim: Vec<usize> = match a.get(2) {
        Some(d) => d.to_matrix()?.data.iter().map(|&v| v as usize).collect(),
        None => vec![1],
    };
    let l = breaks.len().saturating_sub(1);
    let dp: usize = dim.iter().product();
    let total = coefs.numel();
    if l == 0 || dp == 0 || total % (dp * l) != 0 || total == 0 {
        let size = format!("[{}]", dim.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(" "));
        return rt(format!(
            "The requested number of polynomial pieces, {l}, is incompatible with the proposed size, {size}, of a coefficient and the number, {total}, of scalar coefficients provided."
        ));
    }
    let k = total / (dp * l);
    let rows = dp * l;
    let pp = Pp { breaks, coefs: (0..rows).map(|r| (0..k).map(|c| coefs.data[c * rows + r]).collect()).collect(), dim, order: k };
    one(pp.to_value(false))
}

fn unmkpp(a: &[Value]) -> Ret {
    let Some(Value::Struct(s)) = a.first() else { return rt("PP must be a piecewise polynomial structure.") };
    let f = |n: &str| s.get(n).cloned().unwrap_or_else(Value::empty);
    Ok(Some(vec![f("breaks"), f("coefs"), f("pieces"), f("order"), f("dim")]))
}

fn ppval(a: &[Value]) -> Ret {
    let pp = Pp::from_value(crate::builtins::arg(a, 0, "ppval")?)?;
    let xx = m_arg(a, 1, "ppval")?;
    if xx.is_empty() {
        return one(Value::Num(Matrix::from_dims(xx.dims.clone(), Vec::new())));
    }
    one(Value::Num(float_result(pp.eval_at(&xx), is_single(&xx))))
}

// ---- interp1 ---------------------------------------------------------------------------------

/// Whether the sites are equally spaced (to rounding), as the cubic convolution needs.
pub(crate) fn uniform(x: &[f64]) -> bool {
    let h = (x[x.len() - 1] - x[0]) / (x.len() - 1) as f64;
    let tol = 4.0 * f64::EPSILON * x[0].abs().max(x[x.len() - 1].abs()).max(1.0) * x.len() as f64;
    x.windows(2).all(|w| ((w[1] - w[0]) - h).abs() <= tol.max(h.abs() * 1e-12))
}

/// Keys' cubic convolution kernel (a = -1/2) weights for a fractional offset `t` in [0, 1).
fn keys_weights(t: f64) -> [f64; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [(-t3 + 2.0 * t2 - t) / 2.0, (3.0 * t3 - 5.0 * t2 + 2.0) / 2.0, (-3.0 * t3 + 4.0 * t2 + t) / 2.0, (t3 - t2) / 2.0]
}

/// Cubic convolution on equally spaced sites, with the boundary extension of Keys (1981).
fn cubic_conv(x: &[f64], y: &[f64], q: f64) -> f64 {
    let n = x.len();
    let h = (x[n - 1] - x[0]) / (n - 1) as f64;
    let s = (q - x[0]) / h;
    let k = (s.floor() as isize).clamp(0, n as isize - 2) as usize;
    let t = s - k as f64;
    let at = |i: isize| -> f64 {
        if i < 0 {
            3.0 * y[0] - 3.0 * y[1] + y[2]
        } else if i as usize >= n {
            3.0 * y[n - 1] - 3.0 * y[n - 2] + y[n - 3]
        } else {
            y[i as usize]
        }
    };
    let w = keys_weights(t);
    let k = k as isize;
    w[0] * at(k - 1) + w[1] * at(k) + w[2] * at(k + 1) + w[3] * at(k + 2)
}

/// A 1-D interpolant of several value columns on sorted, distinct sites.
pub(crate) struct Interp1 {
    pub x: Vec<f64>,
    pub cols: Vec<Vec<f64>>,
    pub method: Method,
    pps: Vec<Pp>,
}

impl Interp1 {
    pub fn new(x: Vec<f64>, cols: Vec<Vec<f64>>, method: Method) -> Interp1 {
        let pps = if matches!(method, Method::Pchip | Method::Makima | Method::Spline) {
            cols.iter().map(|c| cubic_pp(method, &x, std::slice::from_ref(c), vec![1])).collect()
        } else {
            Vec::new()
        };
        Interp1 { x, cols, method, pps }
    }

    /// Value of column `c` at `q`; `extrap` says whether points outside the sites are
    /// extrapolated (otherwise they are NaN, left to the caller).
    pub fn eval(&self, c: usize, q: f64, extrap: bool) -> f64 {
        let x = &self.x;
        let y = &self.cols[c];
        let n = x.len();
        if q.is_nan() {
            return f64::NAN;
        }
        let outside = q < x[0] || q > x[n - 1];
        if outside && !extrap {
            return f64::NAN;
        }
        match self.method {
            Method::Linear => {
                let k = x[1..n - 1].partition_point(|&v| v <= q);
                let (x0, x1) = (x[k], x[k + 1]);
                let t = (q - x0) / (x1 - x0);
                if q == x1 {
                    y[k + 1]
                } else {
                    y[k] + t * (y[k + 1] - y[k])
                }
            }
            Method::Nearest => {
                let k = x[1..n - 1].partition_point(|&v| v <= q);
                if q - x[k] < x[k + 1] - q { y[k] } else { y[k + 1] }
            }
            Method::Previous => {
                if q < x[0] {
                    return f64::NAN;
                }
                let k = x.partition_point(|&v| v <= q);
                y[k - 1]
            }
            Method::Next => {
                if q > x[n - 1] {
                    return f64::NAN;
                }
                let k = x.partition_point(|&v| v < q);
                y[k]
            }
            Method::Cubic => {
                if outside {
                    return f64::NAN;
                }
                cubic_conv(x, y, q)
            }
            _ => self.pps[c].eval(0, q),
        }
    }
}

fn float_check(m: &Matrix, msg: &str) -> R<()> {
    if m.kind.is_float() { Ok(()) } else { rt(msg.to_string()) }
}

fn interp1(it: &mut Interpreter, a: &[Value]) -> Ret {
    // interp1(v, xq, ...) or interp1(x, v, xq, ...); interp1(x, v, method, 'pp').
    let numeric = |i: usize| matches!(a.get(i), Some(v) if !v.is_text());
    if a.len() == 4 && a[2].is_text() && a[3].is_text() {
        if !a[3].text()?.eq_ignore_ascii_case("pp") {
            return rt("Invalid character vector argument. Did you mean 'pp' or 'extrap'?");
        }
        let x = m_arg(a, 0, "interp1")?;
        let v = m_arg(a, 1, "interp1")?;
        let method = Method::parse(&a[2].text()?).ok_or_else(|| LangError::new("Invalid interpolation method.", 0))?;
        let (xs, ys, _) = chckxy(&x, &v, false)?;
        let pp = match method {
            Method::Linear => Pp { breaks: xs.clone(), coefs: (0..xs.len() - 1).map(|k| vec![(ys[0][k + 1] - ys[0][k]) / (xs[k + 1] - xs[k]), ys[0][k]]).collect(), dim: vec![1], order: 2 },
            Method::Spline | Method::Pchip | Method::Makima => cubic_pp(method, &xs, &ys, vec![1]),
            _ => return rt("Invalid interpolation method."),
        };
        return one(pp.to_value(true));
    }
    let (x, v, xq, rest) = if numeric(2) {
        (Some(m_arg(a, 0, "interp1")?), m_arg(a, 1, "interp1")?, crate::builtins::arg(a, 2, "interp1")?.clone(), &a[3..])
    } else {
        (None, m_arg(a, 0, "interp1")?, crate::builtins::arg(a, 1, "interp1")?.clone(), &a[2..])
    };
    if a.len() == 3 && a[2].is_text() && a[2].text()?.eq_ignore_ascii_case("pp") {
        return rt("Use 4 inputs for PP=INTERP1(X,V,METHOD,'pp').");
    }
    let Value::Num(xq) = xq else {
        return rt("Inputs must be floats, namely single or double.");
    };
    let mut method = Method::Linear;
    let mut extrap: Option<Option<f64>> = None; // Some(None) = 'extrap', Some(Some(v)) = value
    let mut method_given = false;
    for (i, r) in rest.iter().enumerate() {
        if r.is_text() {
            let t = r.text()?;
            if t.eq_ignore_ascii_case("extrap") {
                if !method_given {
                    return rt("An interpolation method must be specified with 'extrap'.");
                }
                extrap = Some(None);
            } else if i == 0 {
                method = Method::parse(&t).ok_or_else(|| LangError::new("Invalid interpolation method.", 0))?;
                method_given = true;
            } else if t.eq_ignore_ascii_case("pp") {
                return rt("Wrong number of input arguments.");
            } else {
                return rt("Invalid character vector argument. Did you mean 'pp' or 'extrap'?");
            }
        } else {
            let m = r.to_matrix()?;
            if m.numel() != 1 || i == 0 {
                return rt("Wrong number of input arguments.");
            }
            extrap = Some(Some(m.data[0]));
        }
    }
    if let Some(x) = &x {
        if !x.kind.is_float() {
            return rt("X must be a vector of type double or single.");
        }
    }
    float_check(&v, "Values V must be of type double or single.")?;
    if !xq.kind.is_float() && xq.kind != Kind::Logical {
        return rt("Inputs must be floats, namely single or double.");
    }
    let single = is_single(&v) || is_single(&xq) || x.as_ref().map(is_single).unwrap_or(false);
    // Values: a vector, or columns along the first dimension.
    let vec_v = v.is_vector() || v.is_scalar();
    let n = if vec_v { v.numel() } else { v.dims[0] };
    let ncols = if vec_v { 1 } else { v.numel() / n };
    let xs = match &x {
        Some(x) => {
            if vec_v && x.numel() != n {
                return rt("X and V must be of the same length.");
            }
            if !vec_v && x.numel() != n {
                return rt("LENGTH(X) and SIZE(V,1) must be the same.");
            }
            x.data.clone()
        }
        None => (1..=n).map(|i| i as f64).collect(),
    };
    if n < 2 {
        return rt("Interpolation requires at least two sample points for each grid dimension.");
    }
    if xs.iter().any(|v| !v.is_finite()) {
        return rt("The sample points must be finite.");
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&p, &q| xs[p].total_cmp(&xs[q]));
    let xsorted: Vec<f64> = order.iter().map(|&o| xs[o]).collect();
    if xsorted.windows(2).any(|w| w[0] == w[1]) {
        return rt("Sample points must be unique.");
    }
    if method == Method::Cubic {
        if n < 3 {
            it.emit(OutputKind::Warning("The 'cubic' method requires at least 3 points in each dimension.\nReverting to the default 'linear' method because this condition is not met.".into()));
            method = Method::Linear;
        } else if !uniform(&xsorted) {
            it.emit(OutputKind::Warning("The 'cubic' method requires the grid to have a uniform spacing.\nSwitching the method from 'cubic' to 'spline' because this condition is not met.".into()));
            method = Method::Spline;
        } else if extrap == Some(None) {
            it.emit(OutputKind::Warning("The 'cubic' and 'v5cubic' methods do not support extrapolation.".into()));
            extrap = None;
        }
    }
    let column = |c: usize, im: bool| -> Vec<f64> {
        order.iter().map(|&o| if im { v.im_at(c * n + o) } else { v.data[c * n + o] }).collect()
    };
    let re = Interp1::new(xsorted.clone(), (0..ncols).map(|c| column(c, false)).collect(), method);
    let imi = v.is_complex().then(|| Interp1::new(xsorted.clone(), (0..ncols).map(|c| column(c, true)).collect(), method));
    let do_extrap = match extrap {
        Some(None) => true,
        Some(Some(_)) => false,
        None => method.extrapolates(),
    };
    let fill = match extrap {
        Some(Some(f)) => f,
        _ => f64::NAN,
    };
    let (lo, hi) = (xsorted[0], xsorted[n - 1]);
    let value = |ip: &Interp1, c: usize, q: f64| -> f64 {
        if !do_extrap && (q < lo || q > hi) {
            return if q.is_nan() { f64::NAN } else { fill };
        }
        ip.eval(c, q, do_extrap)
    };
    let nq = xq.numel();
    let mut re_out = Vec::with_capacity(nq * ncols);
    let mut im_out = Vec::with_capacity(nq * ncols);
    for c in 0..ncols {
        for &q in &xq.data {
            re_out.push(value(&re, c, q));
            if let Some(ip) = &imi {
                im_out.push(value(ip, c, q));
            }
        }
    }
    let dims = if vec_v {
        xq.dims.clone()
    } else {
        let mut d = if xq.is_vector() || xq.is_scalar() { vec![nq] } else { xq.dims.clone() };
        d.extend_from_slice(&v.dims[1..]);
        d
    };
    let mut out = Matrix::from_dims(dims, re_out);
    if imi.is_some() {
        out.im = Some(im_out);
        out.narrow();
    }
    one(Value::Num(float_result(out, single)))
}

// ---- interpft --------------------------------------------------------------------------------

/// Discrete Fourier transform (sign -1) or its unscaled inverse (sign +1).
fn dft(x: &[C], sign: f64) -> Vec<C> {
    let n = x.len();
    (0..n)
        .map(|k| {
            let mut acc = C::new(0.0, 0.0);
            for (j, &v) in x.iter().enumerate() {
                let ang = sign * 2.0 * std::f64::consts::PI * ((k * j) % n) as f64 / n as f64;
                acc = acc.add(v.mul(C::new(ang.cos(), ang.sin())));
            }
            acc
        })
        .collect()
}

fn interpft_slice(x: &[C], n: f64) -> Vec<C> {
    let m = x.len();
    let (incr, ny) = if n > m as f64 { (1, n as usize) } else { let i = (m as f64 / n).floor() as usize + 1; (i, (i as f64 * n) as usize) };
    let a = dft(x, -1.0);
    let nyq = m.div_ceil(2) + usize::from(m % 2 == 0);
    let nyq = nyq.min(m);
    let mut b = vec![C::new(0.0, 0.0); ny.max(m)];
    b[..nyq].copy_from_slice(&a[..nyq]);
    let tail = m - nyq;
    let base = b.len() - tail;
    b[base..].copy_from_slice(&a[nyq..]);
    if m % 2 == 0 && ny > m {
        let half = C::new(b[nyq - 1].re / 2.0, b[nyq - 1].im / 2.0);
        b[nyq - 1] = half;
        b[nyq - 1 + ny - m] = half;
    }
    let y = dft(&b, 1.0);
    let scale = 1.0 / m as f64;
    y.iter().step_by(incr).take(ny.div_ceil(incr)).map(|c| C::new(c.re * scale, c.im * scale)).collect()
}

fn interpft(a: &[Value]) -> Ret {
    let x = m_arg(a, 0, "interpft")?;
    let n = crate::builtins::arg(a, 1, "interpft")?.scalar()?;
    let dim = if a.len() > 2 { crate::builtins::dim_arg(a, 2, &x)? } else { crate::builtins::default_dim(&x.dims) };
    let mut dims = x.dims.clone();
    while dims.len() < dim {
        dims.push(1);
    }
    if n == 0.0 || x.is_empty() {
        return one(Value::Num(Matrix::empty()));
    }
    if n < 0.0 {
        dims[dim - 1] = 0;
        return one(Value::Num(Matrix::from_dims(dims, Vec::new())));
    }
    let (_, sl) = crate::builtins::slices(&x.dims, dim);
    let mut results = Vec::with_capacity(sl.len());
    for idx in &sl {
        let z: Vec<C> = idx.iter().map(|&i| C::new(x.data[i], x.im_at(i))).collect();
        results.push(interpft_slice(&z, n));
    }
    let len = results.first().map(|r| r.len()).unwrap_or(0);
    let mut out_dims = dims.clone();
    out_dims[dim - 1] = len;
    let total = nd::numel(&out_dims);
    let mut re = vec![0.0; total];
    let mut im = vec![0.0; total];
    let stride: usize = out_dims[..dim - 1].iter().product();
    for (s, r) in results.iter().enumerate() {
        let (inner, outer) = (s % stride, s / stride);
        for (j, c) in r.iter().enumerate() {
            let p = outer * stride * len + j * stride + inner;
            re[p] = c.re;
            im[p] = c.im;
        }
    }
    let out = if x.is_complex() { Matrix::complex(out_dims, re, im) } else { Matrix::from_dims(out_dims, re) };
    one(Value::Num(float_result(out, is_single(&x))))
}

// ---- polygons --------------------------------------------------------------------------------

/// Shoelace area of each polygon along `dim` (the vertices of one polygon run along it).
fn polyarea(a: &[Value]) -> Ret {
    let x = m_arg(a, 0, "polyarea")?;
    let y = m_arg(a, 1, "polyarea")?;
    if x.dims != y.dims {
        return rt("X and Y must be the same size.");
    }
    let single = is_single(&x) || is_single(&y);
    if x.is_empty() {
        return one(Value::Num(float_result(Matrix::scalar(0.0), single)));
    }
    let dim = if a.len() > 2 { crate::builtins::dim_arg(a, 2, &x)? } else { crate::builtins::default_dim(&x.dims) };
    let (dims, sl) = crate::builtins::slices(&x.dims, dim);
    let areas = sl
        .iter()
        .map(|idx| {
            let n = idx.len();
            let s: f64 = (0..n).map(|k| {
                let (i, j) = (idx[k], idx[(k + 1) % n]);
                x.data[i] * y.data[j] - x.data[j] * y.data[i]
            }).sum();
            s.abs() / 2.0
        })
        .collect();
    one(Value::Num(float_result(Matrix::from_dims(dims, areas), single)))
}

/// Point-in-polygon by nonzero winding number over all edges; NaN separates loops (each
/// closed), so a hole must run opposite to its outer boundary.
fn inpolygon(a: &[Value]) -> Ret {
    let xq = m_arg(a, 0, "inpolygon")?;
    let yq = m_arg(a, 1, "inpolygon")?;
    let xv = m_arg(a, 2, "inpolygon")?;
    let yv = m_arg(a, 3, "inpolygon")?;
    if xq.numel() != yq.numel() || xv.numel() != yv.numel() {
        return rt("Arrays have incompatible sizes for this operation.");
    }
    // Edges of every loop, closing each one.
    let mut edges = Vec::new();
    let mut start = 0;
    let n = xv.numel();
    for k in 0..=n {
        if k == n || xv.data[k].is_nan() || yv.data[k].is_nan() {
            if k > start {
                for i in start..k {
                    let j = if i + 1 < k { i + 1 } else { start };
                    edges.push(((xv.data[i], yv.data[i]), (xv.data[j], yv.data[j])));
                }
            }
            start = k + 1;
        }
    }
    let scale = xv.data.iter().chain(&yv.data).filter(|v| v.is_finite()).fold(0.0f64, |m, v| m.max(v.abs())).max(1.0);
    let tol = scale * scale * f64::EPSILON * 4.0;
    let mut inside = Vec::with_capacity(xq.numel());
    let mut on = Vec::with_capacity(xq.numel());
    for i in 0..xq.numel() {
        let (px, py) = (xq.data[i], yq.data[i]);
        let mut winding = 0i32;
        let mut boundary = false;
        for &((x1, y1), (x2, y2)) in &edges {
            let cross = (x2 - x1) * (py - y1) - (y2 - y1) * (px - x1);
            if cross.abs() <= tol && px >= x1.min(x2) && px <= x1.max(x2) && py >= y1.min(y2) && py <= y1.max(y2) {
                boundary = true;
            }
            if y1 <= py && py < y2 && cross > 0.0 {
                winding += 1;
            } else if y2 <= py && py < y1 && cross < 0.0 {
                winding -= 1;
            }
        }
        on.push(if boundary { 1.0 } else { 0.0 });
        inside.push(if boundary || winding != 0 { 1.0 } else { 0.0 });
    }
    let as_logical = |v: Vec<f64>| Value::Num(Matrix::from_dims(xq.dims.clone(), v).with_kind(Kind::Logical));
    Ok(Some(vec![as_logical(inside), as_logical(on)]))
}

/// Overlap areas of rectangles `[x y w h]` (rows of A against rows of B).
fn rectint(a: &[Value]) -> Ret {
    let ra = m_arg(a, 0, "rectint")?;
    let rb = m_arg(a, 1, "rectint")?;
    for r in [&ra, &rb] {
        if r.cols() < 4 {
            return rt(format!("Index in position 2 exceeds array bounds. Index must not exceed {}.", r.cols()));
        }
    }
    let (na, nb) = (ra.rows(), rb.rows());
    let mut out = vec![0.0; na * nb];
    for i in 0..na {
        for j in 0..nb {
            let w = (ra.get(i, 0) + ra.get(i, 2)).min(rb.get(j, 0) + rb.get(j, 2)) - ra.get(i, 0).max(rb.get(j, 0));
            let h = (ra.get(i, 1) + ra.get(i, 3)).min(rb.get(j, 1) + rb.get(j, 3)) - ra.get(i, 1).max(rb.get(j, 1));
            out[j * na + i] = w.max(0.0) * h.max(0.0);
        }
    }
    one(Value::Num(Matrix::new(na, nb, out)))
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return crate::builtins_geom::call(it, name, a, nargout);
    }
    match name {
        "conv" => conv(a),
        "deconv" => deconv(a),
        "polyder" => polyder(a, nargout),
        "polyint" => polyint(a),
        "polyvalm" => polyvalm(a),
        "mkpp" => mkpp(a),
        "unmkpp" => unmkpp(a),
        "ppval" => ppval(a),
        "spline" | "pchip" | "makima" => cubic_fn(it, name, a),
        "interp1" => interp1(it, a),
        "interpft" => interpft(a),
        "interp2" | "interp3" | "interpn" => crate::builtins_interpn::call(it, name, a, nargout),
        "polyarea" => polyarea(a),
        "inpolygon" => inpolygon(a),
        "rectint" => rectint(a),
        _ => Ok(None),
    }
}
