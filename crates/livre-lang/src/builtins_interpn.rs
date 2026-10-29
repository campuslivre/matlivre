//! Gridded interpolation in any number of dimensions: the engine behind `interp2`, `interp3`,
//! `interpn` and `griddedInterpolant`.
//!
//! Linear and nearest interpolation work on the 2^N cell corners directly. `cubic` and
//! `spline` are tensor products of the 1-D methods: the values near the query point are reduced
//! one dimension at a time, last dimension first, each fiber being interpolated with the 1-D
//! method. `makima` is not linear in the data, so it is built as MATLAB builds it: a
//! tensor-product Hermite form from node slopes (see `Grid::makima`).

use crate::builtins::{m_arg, one, Ret};
use crate::builtins_polyfun::{Interp1, Method};
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::nd::{self, Dims};
use crate::value::*;

type R<T> = Result<T, LangError>;

/// Values on a full grid, with ascending grid vectors.
pub(crate) struct Grid {
    pub vecs: Vec<Vec<f64>>,
    /// Column-major values with dims `vecs[k].len()`; complex parts in `im`.
    pub re: Vec<f64>,
    pub im: Option<Vec<f64>>,
}

impl Grid {
    fn n(&self) -> usize {
        self.vecs.len()
    }

    fn strides(&self) -> Vec<usize> {
        let mut s = Vec::with_capacity(self.n());
        let mut acc = 1;
        for v in &self.vecs {
            s.push(acc);
            acc *= v.len();
        }
        s
    }

    fn outside(&self, q: &[f64]) -> bool {
        self.vecs.iter().zip(q).any(|(v, &x)| x < v[0] || x > v[v.len() - 1])
    }

    /// Value at `q` with `method`, extrapolating with the same method outside the grid.
    pub fn eval(&self, method: Method, q: &[f64], data: &[f64]) -> f64 {
        if q.iter().any(|x| x.is_nan()) {
            return f64::NAN;
        }
        match method {
            Method::Linear => self.linear(q, data),
            Method::Nearest | Method::Next | Method::Previous => self.stepwise(method, q, data),
            Method::Makima => self.makima(q, data),
            _ => self.tensor(method, q, data),
        }
    }

    /// The window of nodes `[i - before, i + after)` around the query's cell in each
    /// dimension, gathered column-major: (first indices, lengths, values).
    fn window(&self, before: usize, after: usize, q: &[f64], data: &[f64]) -> (Vec<usize>, Vec<usize>, Vec<f64>) {
        let strides = self.strides();
        let mut lo = Vec::with_capacity(self.n());
        let mut len = Vec::with_capacity(self.n());
        for (k, v) in self.vecs.iter().enumerate() {
            let n = v.len();
            let i = v[1..n - 1].partition_point(|&b| b <= q[k]);
            let a = i.saturating_sub(before);
            lo.push(a);
            len.push(i.saturating_add(after).min(n) - a);
        }
        let total: usize = len.iter().product();
        let mut buf = Vec::with_capacity(total);
        let mut idx = vec![0usize; self.n()];
        for _ in 0..total {
            buf.push(data[idx.iter().enumerate().map(|(k, &i)| (lo[k] + i) * strides[k]).sum::<usize>()]);
            for k in 0..idx.len() {
                idx[k] += 1;
                if idx[k] < len[k] {
                    break;
                }
                idx[k] = 0;
            }
        }
        (lo, len, buf)
    }

    /// N-D makima, as MATLAB computes it: a tensor-product cubic Hermite form. The first
    /// derivatives at the nodes are the 1-D makima slopes; a mixed derivative over a set of
    /// dimensions is the corresponding mixed divided difference of the surrounding cells
    /// (extended past the ends like makima's slopes), averaged with the product of each
    /// dimension's makima weights at the node (Akima's bivariate scheme, generalized).
    fn makima(&self, q: &[f64], data: &[f64]) -> f64 {
        let n = self.n();
        let (lo, len, buf) = self.window(3, 5, q, data);
        let values = Arr::new(len.clone(), buf);
        let xs: Vec<&[f64]> = (0..n).map(|k| &self.vecs[k][lo[k]..lo[k] + len[k]]).collect();
        // The query's cell (window-local) and Hermite basis per dimension.
        let mut cell = Vec::with_capacity(n);
        let mut basis = Vec::with_capacity(n);
        for k in 0..n {
            let v = &self.vecs[k];
            let i = v[1..v.len() - 1].partition_point(|&b| b <= q[k]);
            cell.push(i - lo[k]);
            let h = v[i + 1] - v[i];
            let t = (q[k] - v[i]) / h;
            let (t2, t3) = (t * t, t * t * t);
            basis.push(([2.0 * t3 - 3.0 * t2 + 1.0, -2.0 * t3 + 3.0 * t2], [(t3 - 2.0 * t2 + t) * h, (t3 - t2) * h]));
        }
        // Makima weights (left, right) of each corner node along each dimension.
        let corners: Vec<Vec<usize>> = (0usize..1 << n).map(|c| (0..n).map(|k| cell[k] + (c >> k & 1)).collect()).collect();
        let weights: Vec<Vec<(f64, f64)>> = corners
            .iter()
            .map(|node| {
                (0..n)
                    .map(|k| {
                        let fiber: Vec<f64> = (0..len[k]).map(|j| values.at_with(node, k, j)).collect();
                        let (w1, w2) = makima_weights(xs[k], &fiber, node[k]);
                        if w1 + w2 == 0.0 { (0.5, 0.5) } else { (w1 / (w1 + w2), w2 / (w1 + w2)) }
                    })
                    .collect()
            })
            .collect();
        let mut sum = 0.0;
        for mask in 0usize..1 << n {
            // Mixed divided differences over the dimensions in `mask`, extended by two cells
            // at each end of those dimensions.
            let mut diffs = values.clone();
            for k in (0..n).filter(|&k| mask >> k & 1 == 1) {
                diffs = diffs.divided_diff(k, xs[k]).extended(k);
            }
            for (c, node) in corners.iter().enumerate() {
                let mut w = 1.0;
                for (k, (h0, h1)) in basis.iter().enumerate() {
                    w *= if mask >> k & 1 == 1 { h1[c >> k & 1] } else { h0[c >> k & 1] };
                }
                if w == 0.0 {
                    continue;
                }
                // Average the 2^|mask| surrounding cells; cell node-1+s sits at node+1+s.
                let dims: Vec<usize> = (0..n).filter(|&k| mask >> k & 1 == 1).collect();
                let mut deriv = 0.0;
                for sides in 0usize..1 << dims.len() {
                    let mut idx = node.clone();
                    let mut wt = 1.0;
                    for (j, &k) in dims.iter().enumerate() {
                        let s = sides >> j & 1;
                        idx[k] = node[k] + 1 + s;
                        let (a, b) = weights[c][k];
                        wt *= if s == 0 { a } else { b };
                    }
                    deriv += wt * diffs.at(&idx);
                }
                sum += w * deriv;
            }
        }
        sum
    }

    fn linear(&self, q: &[f64], data: &[f64]) -> f64 {
        let strides = self.strides();
        let mut base = 0;
        let mut ts = Vec::with_capacity(self.n());
        let mut steps = Vec::with_capacity(self.n());
        for (k, v) in self.vecs.iter().enumerate() {
            let n = v.len();
            if n == 1 {
                ts.push(0.0);
                steps.push(0);
                continue;
            }
            let i = v[1..n - 1].partition_point(|&b| b <= q[k]);
            base += i * strides[k];
            ts.push((q[k] - v[i]) / (v[i + 1] - v[i]));
            steps.push(strides[k]);
        }
        // Successive linear interpolation, first dimension first.
        let n = self.n();
        let mut vals: Vec<f64> = (0..1usize << n)
            .map(|corner| {
                let off: usize = (0..n).filter(|&k| corner >> k & 1 == 1).map(|k| steps[k]).sum();
                data[base + off]
            })
            .collect();
        for t in ts.iter() {
            let half = vals.len() / 2;
            vals = (0..half).map(|j| {
                let (a, b) = (vals[2 * j], vals[2 * j + 1]);
                if *t == 1.0 { b } else { a + t * (b - a) }
            }).collect();
        }
        vals[0]
    }

    fn stepwise(&self, method: Method, q: &[f64], data: &[f64]) -> f64 {
        let strides = self.strides();
        let mut pos = 0;
        for (k, v) in self.vecs.iter().enumerate() {
            let n = v.len();
            let x = q[k];
            let i = match method {
                Method::Nearest => {
                    if n == 1 {
                        0
                    } else {
                        let i = v[1..n - 1].partition_point(|&b| b <= x);
                        if x - v[i] < v[i + 1] - x { i } else { i + 1 }
                    }
                }
                Method::Previous => {
                    let i = v.partition_point(|&b| b <= x);
                    if i == 0 {
                        return f64::NAN;
                    }
                    i - 1
                }
                _ => {
                    let i = v.partition_point(|&b| b < x);
                    if i == n {
                        return f64::NAN;
                    }
                    i
                }
            };
            pos += i * strides[k];
        }
        data[pos]
    }

    fn tensor(&self, method: Method, q: &[f64], data: &[f64]) -> f64 {
        let strides = self.strides();
        // The window of nodes each dimension's 1-D method needs around the query.
        let (before, after) = match method {
            Method::Cubic => (1, 3),
            Method::Pchip | Method::Makima => (3, 5),
            _ => (usize::MAX / 4, usize::MAX / 4),
        };
        let mut lo = Vec::with_capacity(self.n());
        let mut len = Vec::with_capacity(self.n());
        for (k, v) in self.vecs.iter().enumerate() {
            let n = v.len();
            let i = v[1..n - 1].partition_point(|&b| b <= q[k]);
            let a = i.saturating_sub(before);
            let b = (i.saturating_add(after)).min(n);
            lo.push(a);
            len.push(b - a);
        }
        // Gather the window with the dimension reduced first varying fastest.
        let order: Vec<usize> = (0..self.n()).rev().collect();
        let total: usize = len.iter().product();
        let mut buf = Vec::with_capacity(total);
        let mut idx = vec![0usize; self.n()];
        for _ in 0..total {
            let p: usize = idx.iter().enumerate().map(|(k, &i)| (lo[k] + i) * strides[k]).sum();
            buf.push(data[p]);
            for &k in &order {
                idx[k] += 1;
                if idx[k] < len[k] {
                    break;
                }
                idx[k] = 0;
            }
        }
        for &k in &order {
            let xs = &self.vecs[k][lo[k]..lo[k] + len[k]];
            buf = buf
                .chunks(len[k])
                .map(|fiber| {
                    let m = if method == Method::Cubic && !crate::builtins_polyfun::uniform(xs) { Method::Spline } else { method };
                    Interp1::new(xs.to_vec(), vec![fiber.to_vec()], m).eval(0, q[k], true)
                })
                .collect();
        }
        buf[0]
    }

    /// Values at the query points (one Vec per dimension), with `extrap` (None: NaN outside,
    /// Some(None): extrapolate with `extrap_method`, Some(Some(v)): the value `v`).
    pub fn eval_all(&self, method: Method, extrap: Extrap, pts: &[Vec<f64>]) -> (Vec<f64>, Option<Vec<f64>>) {
        let count = pts.first().map(|p| p.len()).unwrap_or(0);
        let one = |data: &[f64]| -> Vec<f64> {
            (0..count)
                .map(|i| {
                    let q: Vec<f64> = pts.iter().map(|p| p[i]).collect();
                    if self.outside(&q) {
                        match extrap {
                            Extrap::Value(v) => return v,
                            Extrap::Method(m) => return self.eval(m, &q, data),
                        }
                    }
                    self.eval(method, &q, data)
                })
                .collect()
        };
        (one(&self.re), self.im.as_ref().map(|im| one(im)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Extrap {
    Value(f64),
    Method(Method),
}

pub(crate) fn parse_method(s: &str) -> R<Method> {
    match s.to_ascii_lowercase().trim_start_matches('*') {
        "linear" => Ok(Method::Linear),
        "nearest" => Ok(Method::Nearest),
        "next" => Ok(Method::Next),
        "previous" => Ok(Method::Previous),
        "cubic" => Ok(Method::Cubic),
        "pchip" => Ok(Method::Pchip),
        "makima" => Ok(Method::Makima),
        "spline" => Ok(Method::Spline),
        _ => rt("Invalid interpolation method."),
    }
}

fn method_name(m: Method) -> &'static str {
    match m {
        Method::Linear => "linear",
        Method::Nearest => "nearest",
        Method::Next => "next",
        Method::Previous => "previous",
        Method::Cubic => "cubic",
        Method::Pchip => "pchip",
        Method::Makima => "makima",
        Method::Spline => "spline",
    }
}


/// Builds a grid from grid vectors (descending ones are flipped) and values whose dims follow
/// the vectors.
pub(crate) fn make_grid(vecs: Vec<Vec<f64>>, v: &Matrix) -> R<Grid> {
    let n = vecs.len();
    let mut dims: Vec<usize> = v.dims.clone();
    dims.resize(n.max(dims.len()), 1);
    for (k, g) in vecs.iter().enumerate() {
        if g.len() != dims[k] {
            return rt(format!("Sample points vector corresponding to grid dimension {} must contain {} elements.", k + 1, dims[k]));
        }
    }
    if dims.len() > n && dims[n..].iter().any(|&d| d != 1) {
        return rt("The grid vectors do not define a grid of points that match the given values.");
    }
    if vecs.iter().any(|g| g.len() < 2) {
        return rt("Interpolation requires at least two sample points for each grid dimension.");
    }
    if vecs.iter().any(|g| g.iter().any(|x| !x.is_finite())) {
        return rt("The sample points must be finite.");
    }
    let mut re = v.data.clone();
    let mut im = v.im.clone();
    let mut out = Vec::with_capacity(n);
    let mut stride = 1usize;
    for g in vecs {
        let len = g.len();
        let desc = g.windows(2).all(|w| w[1] < w[0]);
        if !desc && !g.windows(2).all(|w| w[1] > w[0]) {
            return rt("Sample points must be strictly monotonic increasing.");
        }
        if desc {
            // Reverse the values along this dimension.
            let flip = |d: &mut Vec<f64>| {
                let copy = d.clone();
                for (p, val) in d.iter_mut().enumerate() {
                    let i = (p / stride) % len;
                    *val = copy[p - i * stride + (len - 1 - i) * stride];
                }
            };
            flip(&mut re);
            if let Some(im) = im.as_mut() {
                flip(im);
            }
            out.push(g.into_iter().rev().collect());
        } else {
            out.push(g);
        }
        stride *= len;
    }
    Ok(Grid { vecs: out, re, im })
}

/// Grid vector `k` from a vector or from a full `ndgrid`-style coordinate array.
fn grid_vector(m: &Matrix, k: usize) -> Vec<f64> {
    if m.is_vector() || m.is_scalar() {
        return m.data.clone();
    }
    let stride: usize = m.dims[..k.min(m.dims.len())].iter().product();
    (0..m.dims.get(k).copied().unwrap_or(1)).map(|i| m.data[i * stride]).collect()
}

/// Swaps the first two dimensions (meshgrid order to ndgrid order).
fn swap12(m: &Matrix) -> Matrix {
    let mut order: Vec<usize> = (0..m.dims.len().max(2)).collect();
    order.swap(0, 1);
    let (dims, data) = nd::permute(&m.dims, &m.data, &order).expect("valid permutation");
    let im = m.im.as_ref().map(|im| nd::permute(&m.dims, im, &order).expect("valid permutation").1);
    Matrix { dims, data, im, kind: m.kind }
}

/// Method name and extrapolation value after the numeric arguments.
fn parse_tail(a: &[Value]) -> R<(Method, Option<f64>)> {
    let mut method = Method::Linear;
    let mut extrap = None;
    for v in a {
        if v.is_text() {
            method = parse_method(&v.text()?).or_else(|_| rt("The interpolation method must be 'linear', 'nearest', 'next', 'previous', 'pchip', 'cubic', 'spline', or 'makima'."))?;
        } else {
            extrap = Some(v.scalar()?);
        }
    }
    Ok((method, extrap))
}

/// Refinement grid `1 : 2^-k : n`.
fn refine(n: usize, k: f64) -> Vec<f64> {
    let step = 2f64.powf(-k);
    let count = ((n as f64 - 1.0) / step).round() as usize + 1;
    (0..count).map(|i| 1.0 + i as f64 * step).collect()
}

fn ones_to(n: usize) -> Vec<f64> {
    (1..=n).map(|i| i as f64).collect()
}

/// All combinations of the query vectors, with the result dims; `mesh` puts the second
/// coordinate first (meshgrid order: y along the rows).
fn combos(qs: &[Matrix], mesh: bool) -> (Vec<Vec<f64>>, Dims) {
    let lens: Vec<usize> = qs.iter().map(|q| q.numel()).collect();
    let mut order: Vec<usize> = (0..qs.len()).collect();
    if mesh && qs.len() >= 2 {
        order.swap(0, 1);
    }
    let dims: Dims = order.iter().map(|&k| lens[k]).collect();
    let total: usize = lens.iter().product();
    let mut pts = vec![Vec::with_capacity(total); qs.len()];
    for p in 0..total {
        let mut r = p;
        for &k in &order {
            pts[k].push(qs[k].data[r % lens[k]]);
            r /= lens[k];
        }
    }
    (pts, dims)
}

/// Query points from coordinate arrays: equal sizes give points; vectors of different shapes
/// span a grid.
fn queries(qs: &[Matrix], mesh: bool) -> R<(Vec<Vec<f64>>, Dims)> {
    if qs.iter().all(|q| q.dims == qs[0].dims) {
        return Ok((qs.iter().map(|q| q.data.clone()).collect(), qs[0].dims.clone()));
    }
    let vectors = qs.iter().all(|q| q.is_vector() || q.is_scalar());
    // Two row vectors (or two columns) of different lengths are not a grid.
    let mixed = qs.iter().any(|q| q.dims[0] == 1 && !q.is_scalar()) && qs.iter().any(|q| q.dims[0] != 1);
    if !vectors || (qs.len() == 2 && !mixed) {
        return rt("Query coordinates input arrays must have the same size.");
    }
    Ok(combos(qs, mesh))
}

/// The method actually used on `grid`, warning as MATLAB does when it falls back.
pub(crate) fn effective_method(it: &mut Interpreter, grid: &Grid, method: Method) -> Method {
    let revert = |it: &mut Interpreter, why: &str| {
        it.emit(OutputKind::Warning(format!("{why}\nReverting to the default 'linear' method because this condition is not met.")));
        Method::Linear
    };
    match method {
        Method::Pchip if grid.n() > 1 => revert(it, "The 'pchip' method is only supported in 1D."),
        Method::Cubic if grid.vecs.iter().any(|v| v.len() < 3) => revert(it, "The 'cubic' method requires at least 3 points in each dimension."),
        Method::Cubic if !grid.vecs.iter().all(|v| crate::builtins_polyfun::uniform(v)) => {
            it.emit(OutputKind::Warning(
                "The 'cubic' method requires the grid to have a uniform spacing.\nSwitching the method from 'cubic' to 'spline' because this condition is not met.".into(),
            ));
            Method::Spline
        }
        m => m,
    }
}

/// Applies the method fallbacks, evaluates and shapes the result.
fn finish(it: &mut Interpreter, grid: Grid, method: Method, extrapval: Option<f64>, pts: Vec<Vec<f64>>, mut dims: Dims, single: bool) -> Ret {
    let method = effective_method(it, &grid, method);
    let extrap = match extrapval {
        Some(v) => Extrap::Value(v),
        None if method.extrapolates() => Extrap::Method(method),
        None => Extrap::Value(f64::NAN),
    };
    let (re, im) = grid.eval_all(method, extrap, &pts);
    if dims.len() < 2 {
        dims.push(1);
    }
    let mut out = Matrix::from_dims(dims, re);
    if let Some(im) = im {
        out.im = Some(im);
        out.narrow();
    }
    one(Value::Num(if single { out.cast_to(Kind::Single) } else { out }))
}

fn nums(a: &[Value], name: &str) -> R<Vec<Matrix>> {
    (0..a.len()).map(|i| m_arg(a, i, name)).collect()
}

/// interp2 and interp3 use meshgrid conventions, with x along the columns of V. The grid is
/// kept in (x, y, z) order, so V's first two dimensions are swapped.
fn interp_mesh(it: &mut Interpreter, a: &[Value], name: &str, n: usize) -> Ret {
    let n_num = a.iter().take_while(|v| !v.is_text()).count();
    let (num, tail) = a.split_at(n_num);
    let m = nums(num, name)?;
    let (method, mut extrap) = parse_tail(tail)?;
    if m.is_empty() {
        return rt("Not enough input arguments.");
    }
    // Sizes of V in (x, y, z) order.
    let lens_of = |v: &Matrix| -> Vec<usize> { (0..n).map(|i| v.dims.get([1, 0, 2][i]).copied().unwrap_or(1)).collect() };
    let c = m.len();
    let (vecs, v, qs, refined) = if c == 1 || (c == 2 && m[1].is_scalar()) {
        let k = m.get(1).map(|k| k.data[0]).unwrap_or(1.0);
        let lens = lens_of(&m[0]);
        (lens.iter().map(|&l| ones_to(l)).collect::<Vec<_>>(), m[0].clone(), lens.iter().map(|&l| Matrix::row(refine(l, k))).collect::<Vec<_>>(), true)
    } else if c == n + 1 || c == n + 2 {
        if c == n + 2 {
            extrap = Some(m[n + 1].data[0]);
        }
        (lens_of(&m[0]).iter().map(|&l| ones_to(l)).collect(), m[0].clone(), m[1..=n].to_vec(), false)
    } else if c == 2 * n + 1 || c == 2 * n + 2 {
        if c == 2 * n + 2 {
            extrap = Some(m[2 * n + 1].data[0]);
        }
        // Full coordinate arrays are in meshgrid order: x varies along dimension 2.
        ((0..n).map(|k| grid_vector(&m[k], [1, 0, 2][k])).collect(), m[n].clone(), m[n + 1..=2 * n].to_vec(), false)
    } else {
        return rt("Wrong number of input arguments.");
    };
    let single = m.iter().any(|x| x.kind == Kind::Single);
    let grid = make_grid(vecs, &swap12(&v))?;
    let (pts, dims) = if refined { combos(&qs, true) } else { queries(&qs, true)? };
    finish(it, grid, method, extrap, pts, dims, single)
}

fn interpn(it: &mut Interpreter, a: &[Value]) -> Ret {
    let n_num = a.iter().take_while(|v| !v.is_text()).count();
    let (num, tail) = a.split_at(n_num);
    let m = nums(num, "interpn")?;
    let (method, mut extrap) = parse_tail(tail)?;
    if m.is_empty() {
        return rt("Not enough input arguments.");
    }
    let c = m.len();
    // The implicit-grid forms take V first, whose dimensionality fixes N (a vector is 1-D).
    let n0 = if m[0].is_vector() { 1 } else { m[0].dims.len() };
    let lens0: Vec<usize> = if n0 == 1 { vec![m[0].numel()] } else { m[0].dims.clone() };
    let (vecs, v, qs, refined) = if c == 1 || (c == 2 && m[1].is_scalar()) {
        let k = m.get(1).map(|k| k.data[0]).unwrap_or(1.0);
        (lens0.iter().map(|&l| ones_to(l)).collect::<Vec<_>>(), m[0].clone(), lens0.iter().map(|&l| Matrix::col(refine(l, k))).collect::<Vec<_>>(), true)
    } else if n0 > 1 && (c == n0 + 1 || c == n0 + 2) {
        if c == n0 + 2 {
            extrap = Some(m[n0 + 1].data[0]);
        }
        (lens0.iter().map(|&l| ones_to(l)).collect(), m[0].clone(), m[1..=n0].to_vec(), false)
    } else if n0 == 1 && c == 2 && !m[1].is_scalar() {
        (vec![ones_to(m[0].numel())], m[0].clone(), vec![m[1].clone()], false)
    } else {
        let n = (c - 1) / 2;
        if n == 0 {
            return rt("Wrong number of input arguments.");
        }
        if c == 2 * n + 2 {
            extrap = Some(m[2 * n + 1].data[0]);
        }
        ((0..n).map(|k| grid_vector(&m[k], k)).collect(), m[n].clone(), m[n + 1..=2 * n].to_vec(), false)
    };
    let single = m.iter().any(|x| x.kind == Kind::Single);
    let one_d = vecs.len() == 1;
    let values = if one_d { Matrix::col(v.data.clone()) } else { v.clone() };
    let values = Matrix { im: v.im.clone(), ..values };
    let grid = make_grid(vecs, &values)?;
    let (pts, dims) = if refined {
        let (p, d) = combos(&qs, false);
        // A row vector V refines to a row.
        (p, if one_d && v.dims[0] == 1 { vec![1, d[0]] } else { d })
    } else {
        queries(&qs, false)?
    };
    finish(it, grid, method, extrap, pts, dims, single)
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    match name {
        "interp2" => interp_mesh(it, a, name, 2),
        "interp3" => interp_mesh(it, a, name, 3),
        "interpn" => interpn(it, a),
        _ => Ok(None),
    }
}

/// Makima's weights `(w1, w2)` of the left and right differences at node `k` of a fiber.
fn makima_weights(x: &[f64], y: &[f64], k: usize) -> (f64, f64) {
    let n = x.len();
    if n == 2 {
        return (1.0, 1.0);
    }
    let del: Vec<f64> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / (x[i + 1] - x[i])).collect();
    let e = extend_slopes(&del);
    let (a, b, c, d) = (e[k], e[k + 1], e[k + 2], e[k + 3]);
    ((d - c).abs() + (d + c).abs() / 2.0, (b - a).abs() + (b + a).abs() / 2.0)
}

/// Two extra values at each end, continued linearly as makima does with its slopes.
fn extend_slopes(d: &[f64]) -> Vec<f64> {
    let m = d.len();
    let (first, second) = (d[0], if m > 1 { d[1] } else { d[0] });
    let (last, before) = (d[m - 1], if m > 1 { d[m - 2] } else { d[m - 1] });
    let d0 = 2.0 * first - second;
    let dn = 2.0 * last - before;
    let mut e = Vec::with_capacity(m + 4);
    e.push(2.0 * d0 - first);
    e.push(d0);
    e.extend_from_slice(d);
    e.push(dn);
    e.push(2.0 * dn - last);
    e
}

/// A small column-major N-D array for the makima window.
#[derive(Clone)]
struct Arr {
    dims: Vec<usize>,
    data: Vec<f64>,
}

impl Arr {
    fn new(dims: Vec<usize>, data: Vec<f64>) -> Arr {
        Arr { dims, data }
    }

    fn pos(&self, idx: &[usize]) -> usize {
        let mut p = 0;
        let mut stride = 1;
        for (k, &i) in idx.iter().enumerate() {
            p += i * stride;
            stride *= self.dims[k];
        }
        p
    }

    fn at(&self, idx: &[usize]) -> f64 {
        self.data[self.pos(idx)]
    }

    /// Value at `node` with coordinate `k` replaced by `j`.
    fn at_with(&self, node: &[usize], k: usize, j: usize) -> f64 {
        let mut idx = node.to_vec();
        idx[k] = j;
        self.at(&idx)
    }

    /// Applies `f` to every fiber along dimension `k`, producing fibers of length `out_len`.
    fn map_fibers(&self, k: usize, out_len: usize, f: impl Fn(&[f64]) -> Vec<f64>) -> Arr {
        let stride: usize = self.dims[..k].iter().product();
        let len = self.dims[k];
        let outer: usize = self.dims[k + 1..].iter().product();
        let mut dims = self.dims.clone();
        dims[k] = out_len;
        let mut data = vec![0.0; stride * out_len * outer];
        for o in 0..outer {
            for i in 0..stride {
                let fiber: Vec<f64> = (0..len).map(|j| self.data[o * stride * len + j * stride + i]).collect();
                for (j, v) in f(&fiber).into_iter().enumerate() {
                    data[o * stride * out_len + j * stride + i] = v;
                }
            }
        }
        Arr { dims, data }
    }

    fn divided_diff(&self, k: usize, x: &[f64]) -> Arr {
        self.map_fibers(k, self.dims[k] - 1, |f| (0..f.len() - 1).map(|j| (f[j + 1] - f[j]) / (x[j + 1] - x[j])).collect())
    }

    fn extended(&self, k: usize) -> Arr {
        self.map_fibers(k, self.dims[k] + 4, extend_slopes)
    }
}
