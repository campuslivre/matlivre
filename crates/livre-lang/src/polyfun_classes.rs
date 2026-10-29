//! The object-based parts of `polyfun`: `griddedInterpolant`, `scatteredInterpolant`,
//! `triangulation`, `delaunayTriangulation` and `alphaShape`.
//!
//! Each class is an ordinary value class whose definition is MATLAB source (generated below):
//! the properties are real properties, so display, `properties`, dot access and assignment
//! behave as for user classes, and every method is a one-line MATLAB function forwarding to the
//! hidden builtin `matlivre_polyfun_method`, which does the work here. Constructors are builtins
//! that fill the properties directly.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::builtins::{m_arg, one, Ret};
use crate::builtins_geom::{self as geom, Delaunay, Scattered};
use crate::builtins_interpn::{self as gridded, Extrap};
use crate::builtins_polyfun::Method;
use crate::class::{ClassDef, Object};
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &["griddedInterpolant", "scatteredInterpolant", "triangulation", "delaunayTriangulation", "alphaShape", "boundary", "matlivre_polyfun_method"];

const TRI_METHODS: &[&str] = &[
    "barycentricToCartesian", "cartesianToBarycentric", "circumcenter", "edgeAttachments", "edges", "faceNormal", "featureEdges", "freeBoundary", "incenter",
    "isConnected", "nearestNeighbor", "neighbors", "pointLocation", "size", "vertexAttachments", "vertexNormal",
];
const DT_METHODS: &[&str] = &["convexHull", "voronoiDiagram", "isInterior"];
const ALPHA_METHODS: &[&str] = &[
    "alphaSpectrum", "criticalAlpha", "numRegions", "inShape", "alphaTriangulation", "boundaryFacets", "perimeter", "area", "surfaceArea", "volume", "nearestNeighbor",
];

fn props_of(class: &str) -> &'static [&'static str] {
    match class {
        "griddedInterpolant" => &["GridVectors", "Values", "Method", "ExtrapolationMethod"],
        "scatteredInterpolant" => &["Points", "Values", "Method", "ExtrapolationMethod"],
        "triangulation" => &["Points", "ConnectivityList"],
        "delaunayTriangulation" => &["Points", "ConnectivityList", "Constraints"],
        _ => &["Points", "Alpha", "HoleThreshold", "RegionThreshold"],
    }
}

fn methods_of(class: &str) -> Vec<&'static str> {
    match class {
        "triangulation" => TRI_METHODS.to_vec(),
        "delaunayTriangulation" => TRI_METHODS.iter().chain(DT_METHODS).copied().collect(),
        "alphaShape" => ALPHA_METHODS.to_vec(),
        _ => Vec::new(),
    }
}

/// The class definition, generated as MATLAB source and parsed once per thread.
fn class_def(class: &str) -> Rc<ClassDef> {
    thread_local! {
        static DEFS: RefCell<HashMap<String, Rc<ClassDef>>> = RefCell::new(HashMap::new());
    }
    DEFS.with(|defs| {
        if let Some(d) = defs.borrow().get(class) {
            return d.clone();
        }
        let mut src = format!("classdef {class}\n  properties\n");
        for p in props_of(class) {
            src.push_str(&format!("    {p}\n"));
        }
        src.push_str("  end\n  methods\n");
        for m in methods_of(class) {
            src.push_str(&format!(
                "    function varargout = {m}(obj, varargin)\n      varargout = matlivre_polyfun_method('{class}.{m}', max(nargout, 1), obj, varargin{{:}});\n    end\n"
            ));
        }
        src.push_str("  end\nend\n");
        let ast = crate::class::parse_classdef(&src).expect("built-in class source parses");
        let def = Rc::new(ClassDef::build(ast, &[]));
        defs.borrow_mut().insert(class.to_string(), def.clone());
        def
    })
}

fn make(class: &str, values: Vec<Value>) -> Value {
    Value::Object(Object { class: class_def(class), props: Rc::new(RefCell::new(values)) })
}

fn prop(o: &Object, name: &str) -> Value {
    o.get(name).unwrap_or_else(Value::empty)
}

fn prop_m(o: &Object, name: &str) -> R<Matrix> {
    prop(o, name).to_matrix()
}

fn prop_text(o: &Object, name: &str) -> R<String> {
    prop(o, name).text()
}

fn col(v: Vec<f64>) -> Value {
    Value::Num(Matrix::col(v))
}

fn logical_col(v: Vec<bool>) -> Value {
    Value::Num(Matrix::col(v.into_iter().map(|b| if b { 1.0 } else { 0.0 }).collect()).with_kind(Kind::Logical))
}

fn points_matrix(pts: &[Vec<f64>]) -> Matrix {
    let n = pts.len();
    let d = pts.first().map(|p| p.len()).unwrap_or(0);
    let mut data = vec![0.0; n * d];
    for (i, p) in pts.iter().enumerate() {
        for k in 0..d {
            data[k * n + i] = p[k];
        }
    }
    Matrix::new(n, d, data)
}

fn index_rows(m: &Matrix) -> Vec<Vec<usize>> {
    geom::rows(m).into_iter().map(|r| r.into_iter().map(|v| v as usize - 1).collect()).collect()
}

fn index_matrix(rows: &[Vec<usize>], cols: usize) -> Matrix {
    geom::index_matrix(rows, cols)
}

// ---- griddedInterpolant ----------------------------------------------------------------------

fn method_text(m: Method) -> &'static str {
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

fn grid_method(s: &str) -> R<Method> {
    gridded::parse_method(s).or_else(|_| rt("The interpolation method must be 'linear', 'nearest', 'next', 'previous', 'pchip', 'cubic', 'spline', or 'makima'."))
}

fn new_gridded(a: &[Value]) -> Ret {
    let n_num = a.iter().take_while(|v| !v.is_text()).count();
    let (num, tail) = a.split_at(n_num);
    let method = match tail.first() {
        Some(m) => grid_method(&m.text()?)?,
        None => Method::Linear,
    };
    let extrap = match tail.get(1) {
        Some(e) => {
            let t = e.text()?;
            if t != "none" {
                grid_method(&t)?;
            }
            t
        }
        None => method_text(method).to_string(),
    };
    let (vecs, values): (Vec<Vec<f64>>, Matrix) = match num {
        [] => return rt("Not enough input arguments."),
        [v] => {
            let v = v.to_matrix()?;
            let dims: Vec<usize> = if v.is_vector() { vec![v.numel()] } else { v.dims.clone() };
            (dims.iter().map(|&d| (1..=d).map(|i| i as f64).collect()).collect(), v)
        }
        [Value::Cell(c), v] => (c.data.iter().map(|g| g.to_matrix().map(|m| m.data)).collect::<R<_>>()?, v.to_matrix()?),
        _ => {
            let k = num.len() - 1;
            let v = num[k].to_matrix()?;
            let mut vecs = Vec::new();
            for (i, g) in num[..k].iter().enumerate() {
                let g = g.to_matrix()?;
                // Full ndgrid arrays reduce to their grid vectors.
                if g.is_vector() || k == 1 {
                    vecs.push(g.data);
                } else {
                    let stride: usize = g.dims[..i].iter().product();
                    vecs.push((0..g.dims[i]).map(|j| g.data[j * stride]).collect());
                }
            }
            (vecs, v)
        }
    };
    for g in &vecs {
        if g.windows(2).any(|w| w[1] == w[0]) {
            return rt("Sample points must be unique.");
        }
        if g.windows(2).any(|w| w[1] < w[0]) {
            return rt("Sample points must be sorted in ascending order.");
        }
    }
    // Values of a 1-D interpolant may be a row or a column.
    let check = if vecs.len() == 1 { Matrix::col(values.data.clone()) } else { values.clone() };
    let check = Matrix { im: values.im.clone(), ..check };
    let grid = gridded::make_grid(vecs, &check)?;
    let gv = CellArray::row(grid.vecs.iter().map(|g| Value::Num(Matrix::row(g.clone()))).collect());
    one(make("griddedInterpolant", vec![Value::Cell(gv), Value::Num(values), Value::chars(method_text(method)), Value::chars(&extrap)]))
}

fn eval_gridded(it: &mut Interpreter, o: &Object, args: &[Value]) -> Ret {
    let Value::Cell(gv) = prop(o, "GridVectors") else { return rt("Invalid griddedInterpolant.") };
    let vecs: Vec<Vec<f64>> = gv.data.iter().map(|g| g.to_matrix().map(|m| m.data)).collect::<R<_>>()?;
    let values = prop_m(o, "Values")?;
    let n = vecs.len();
    let v = if n == 1 { Matrix { dims: vec![values.numel(), 1], ..values.clone() } } else { values.clone() };
    let grid = gridded::make_grid(vecs, &v)?;
    let method = gridded::effective_method(it, &grid, grid_method(&prop_text(o, "Method")?)?);
    let extrap = match prop_text(o, "ExtrapolationMethod")?.as_str() {
        "none" => Extrap::Value(f64::NAN),
        m => Extrap::Method(grid_method(m)?),
    };
    let (pts, dims): (Vec<Vec<f64>>, Vec<usize>) = match args {
        [Value::Cell(c)] => {
            let qs: Vec<Vec<f64>> = c.data.iter().map(|g| g.to_matrix().map(|m| m.data)).collect::<R<_>>()?;
            let lens: Vec<usize> = qs.iter().map(|q| q.len()).collect();
            let total: usize = lens.iter().product();
            let mut pts = vec![Vec::with_capacity(total); n];
            for p in 0..total {
                let mut r = p;
                for k in 0..n {
                    pts[k].push(qs[k][r % lens[k]]);
                    r /= lens[k];
                }
            }
            (pts, if lens.len() == 1 { vec![lens[0], 1] } else { lens })
        }
        [q] if n == 1 => {
            let q = q.to_matrix()?;
            (vec![q.data.clone()], q.dims.clone())
        }
        [q] => {
            let q = q.to_matrix()?;
            if q.cols() != n {
                return rt(format!("Query point dimension does not match the dimension of the interpolant (expected {n} columns)."));
            }
            let pts = geom::rows(&q);
            ((0..n).map(|k| pts.iter().map(|p| p[k]).collect()).collect(), vec![q.rows(), 1])
        }
        qs if qs.len() == n => {
            let qs: Vec<Matrix> = qs.iter().map(|q| q.to_matrix()).collect::<R<_>>()?;
            if qs.iter().any(|q| q.dims != qs[0].dims) {
                return rt("Query coordinates input arrays must have the same size.");
            }
            (qs.iter().map(|q| q.data.clone()).collect(), qs[0].dims.clone())
        }
        _ => return rt("Invalid number of input arguments for the griddedInterpolant."),
    };
    let (re, im) = grid.eval_all(method, extrap, &pts);
    let mut out = Matrix::from_dims(dims, re);
    if let Some(im) = im {
        out.im = Some(im);
        out.narrow();
    }
    one(Value::Num(if values.kind == Kind::Single { out.cast_to(Kind::Single) } else { out }))
}

// ---- scatteredInterpolant --------------------------------------------------------------------

fn new_scattered(a: &[Value]) -> Ret {
    let n_num = a.iter().take_while(|v| !v.is_text()).count();
    let (num, tail) = a.split_at(n_num);
    let method = tail.first().map(|m| m.text()).transpose()?.unwrap_or_else(|| "linear".into());
    let extrap = tail.get(1).map(|m| m.text()).transpose()?.unwrap_or_else(|| if method == "nearest" { "nearest".into() } else { "linear".into() });
    for (m, allowed) in [(&method, &["linear", "nearest", "natural"][..]), (&extrap, &["linear", "nearest", "none"][..])] {
        if !allowed.contains(&m.as_str()) {
            return rt(format!("Invalid interpolation method '{m}'."));
        }
    }
    let ms: Vec<Matrix> = num.iter().map(|v| v.to_matrix()).collect::<R<_>>()?;
    let (pts, v) = match ms.len() {
        0 => (Matrix::empty(), Matrix::empty()),
        2 => (ms[0].clone(), ms[1].clone()),
        3 | 4 => {
            let k = ms.len() - 1;
            (points_matrix(&geom::points_from(&ms[..k])?), ms[k].clone())
        }
        _ => return rt("Wrong number of input arguments."),
    };
    if v.numel() != pts.rows() && !(pts.is_empty() && v.is_empty()) {
        return rt("The number of data point locations should equal the number of data point values.");
    }
    let values = Matrix { dims: vec![v.numel(), 1], ..v };
    one(make("scatteredInterpolant", vec![Value::Num(pts), Value::Num(values), Value::chars(&method), Value::chars(&extrap)]))
}

fn eval_scattered(it: &mut Interpreter, o: &Object, args: &[Value]) -> Ret {
    let pts = geom::rows(&prop_m(o, "Points")?);
    let values = prop_m(o, "Values")?;
    let method = prop_text(o, "Method")?;
    let extrap = prop_text(o, "ExtrapolationMethod")?;
    let d = pts.first().map(|p| p.len()).unwrap_or(0);
    let (qs, dims): (Vec<Vec<f64>>, Vec<usize>) = match args {
        [Value::Cell(c)] => {
            let gs: Vec<Vec<f64>> = c.data.iter().map(|g| g.to_matrix().map(|m| m.data)).collect::<R<_>>()?;
            let lens: Vec<usize> = gs.iter().map(|g| g.len()).collect();
            let total: usize = lens.iter().product();
            let mut q = Vec::with_capacity(total);
            for p in 0..total {
                let mut r = p;
                q.push(gs.iter().zip(&lens).map(|(g, &l)| {
                    let v = g[r % l];
                    r /= l;
                    v
                }).collect());
            }
            (q, lens)
        }
        [q] => {
            let q = q.to_matrix()?;
            (geom::rows(&q), vec![q.rows(), 1])
        }
        qs => {
            let qs: Vec<Matrix> = qs.iter().map(|q| q.to_matrix()).collect::<R<_>>()?;
            (geom::points_from(&qs)?, qs[0].dims.clone())
        }
    };
    if qs.iter().any(|q| q.len() != d) {
        return rt("The dimension of the query points does not match the dimension of the interpolant.");
    }
    // Duplicate sample points are merged, averaging their values.
    let (uniq, _) = geom::unique_points_quiet(&pts);
    let mut sums: HashMap<Vec<u64>, (f64, f64, f64)> = HashMap::new();
    for (i, p) in pts.iter().enumerate() {
        let e = sums.entry(p.iter().map(|x| x.to_bits()).collect()).or_insert((0.0, 0.0, 0.0));
        e.0 += values.data[i];
        e.1 += values.im_at(i);
        e.2 += 1.0;
    }
    if uniq.len() < pts.len() {
        it.emit(OutputKind::Warning("Duplicate data points have been detected and removed - corresponding values have been averaged.".into()));
    }
    let avg = |u: &Vec<f64>, im: bool| {
        let e = sums[&u.iter().map(|x| x.to_bits()).collect::<Vec<u64>>()];
        (if im { e.1 } else { e.0 }) / e.2
    };
    let s = Scattered::new(uniq.clone())?;
    let eval = |im: bool| -> Vec<f64> {
        let v: Vec<f64> = uniq.iter().map(|u| avg(u, im)).collect();
        qs.iter()
            .map(|q| {
                let inside = match method.as_str() {
                    "linear" => s.linear(&v, q),
                    "nearest" => Some(s.nearest(&v, q)),
                    _ if d == 2 => s.natural(&v, q),
                    _ => s.linear(&v, q),
                };
                match (inside, extrap.as_str()) {
                    (Some(x), _) => x,
                    (None, "nearest") => s.nearest(&v, q),
                    (None, "linear") => s.extrapolate_linear(&v, q),
                    _ => f64::NAN,
                }
            })
            .collect()
    };
    let mut out = Matrix::from_dims(dims, eval(false));
    if values.is_complex() {
        out.im = Some(eval(true));
        out.narrow();
    }
    one(Value::Num(out))
}

/// `F(args)` for the interpolant classes (MATLAB's parenthesis call on the object).
pub(crate) fn call_object(it: &mut Interpreter, o: &Object, args: &[Value]) -> Option<Ret> {
    match o.class.name.as_str() {
        "griddedInterpolant" => Some(eval_gridded(it, o, args)),
        "scatteredInterpolant" => Some(eval_scattered(it, o, args)),
        _ => None,
    }
}

// ---- triangulation ---------------------------------------------------------------------------

struct Tri {
    pts: Vec<Vec<f64>>,
    t: Vec<Vec<usize>>,
}

impl Tri {
    fn of(o: &Object) -> R<Tri> {
        Ok(Tri { pts: geom::rows(&prop_m(o, "Points")?), t: index_rows(&prop_m(o, "ConnectivityList")?) })
    }

    fn dim(&self) -> usize {
        self.pts.first().map(|p| p.len()).unwrap_or(0)
    }

    /// Triangles (or tetrahedra) as directed facets: map from sorted facet to simplices.
    fn facet_map(&self) -> HashMap<Vec<usize>, Vec<usize>> {
        let mut m: HashMap<Vec<usize>, Vec<usize>> = HashMap::new();
        for (k, s) in self.t.iter().enumerate() {
            for skip in 0..s.len() {
                let mut f: Vec<usize> = s.iter().enumerate().filter(|&(i, _)| i != skip).map(|(_, &v)| v).collect();
                f.sort_unstable();
                m.entry(f).or_default().push(k);
            }
        }
        m
    }

    fn edges(&self) -> Vec<(usize, usize)> {
        let mut e: Vec<(usize, usize)> = Vec::new();
        for s in &self.t {
            for i in 0..s.len() {
                for j in i + 1..s.len() {
                    e.push((s[i].min(s[j]), s[i].max(s[j])));
                }
            }
        }
        e.sort_unstable();
        e.dedup();
        e
    }

    /// Boundary facets, oriented as in their simplex (outward for consistently oriented
    /// meshes), chained into loops for triangles.
    fn free_boundary(&self) -> Vec<Vec<usize>> {
        let fm = self.facet_map();
        let simplex = self.t.first().map(|s| s.len()).unwrap_or(0);
        let mut out = Vec::new();
        for s in &self.t {
            for skip in 0..s.len() {
                let mut key: Vec<usize> = s.iter().enumerate().filter(|&(i, _)| i != skip).map(|(_, &v)| v).collect();
                key.sort_unstable();
                if fm[&key].len() != 1 {
                    continue;
                }
                // The facet opposite vertex `skip`, in the simplex's cyclic order.
                let f: Vec<usize> = if simplex == 3 {
                    vec![s[(skip + 1) % 3], s[(skip + 2) % 3]]
                } else {
                    // Tetrahedron: opposite faces with outward orientation.
                    let faces = [[1, 3, 2], [0, 2, 3], [0, 3, 1], [0, 1, 2]];
                    faces[skip].iter().map(|&i| s[i]).collect()
                };
                out.push(f);
            }
        }
        if simplex == 3 {
            // Chain the edges into loops.
            // At a vertex shared by several loops, MATLAB continues along the edge that turns
            // most clockwise.
            let heading = |e: &Vec<usize>| {
                let (a, b) = (&self.pts[e[0]], &self.pts[e[1]]);
                (b[1] - a[1]).atan2(b[0] - a[0])
            };
            let turn = |from: &Vec<usize>, to: &Vec<usize>| {
                let t = heading(to) - heading(from);
                let pi = std::f64::consts::PI;
                if t > pi { t - 2.0 * pi } else if t <= -pi { t + 2.0 * pi } else { t }
            };
            let mut chained = Vec::with_capacity(out.len());
            let mut used = vec![false; out.len()];
            for start in 0..out.len() {
                if used[start] {
                    continue;
                }
                let mut cur = start;
                loop {
                    used[cur] = true;
                    chained.push(out[cur].clone());
                    let end = out[cur][1];
                    let next = (0..out.len()).filter(|&k| !used[k] && out[k][0] == end).min_by(|&j, &k| turn(&out[cur], &out[j]).total_cmp(&turn(&out[cur], &out[k])));
                    match next {
                        Some(k) => cur = k,
                        None => break,
                    }
                }
            }
            return chained;
        }
        out
    }

    fn face_normals(&self) -> Vec<[f64; 3]> {
        self.t
            .iter()
            .map(|s| {
                let p = |i: usize| -> [f64; 3] {
                    let q = &self.pts[s[i]];
                    [q[0], q[1], q.get(2).copied().unwrap_or(0.0)]
                };
                let (a, b, c) = (p(0), p(1), p(2));
                let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                [n[0] / len, n[1] / len, n[2] / len]
            })
            .collect()
    }

    /// Simplices around vertex v; for triangles, counterclockwise from the boundary (or from
    /// the lowest index for an interior vertex).
    fn attachments(&self, v: usize) -> Vec<usize> {
        let fan: Vec<usize> = (0..self.t.len()).filter(|&k| self.t[k].contains(&v)).collect();
        if self.t.first().map(|s| s.len()) != Some(3) || fan.is_empty() {
            return fan;
        }
        let other = |k: usize| {
            let s = &self.t[k];
            let i = s.iter().position(|&x| x == v).unwrap();
            (s[(i + 1) % 3], s[(i + 2) % 3])
        };
        // Counterclockwise: [v, a, b] is followed by the triangle whose `a` is b.
        let start = fan.iter().copied().find(|&k| !fan.iter().any(|&j| other(j).1 == other(k).0)).unwrap_or(fan[0]);
        let mut order = vec![start];
        while let Some(next) = fan.iter().copied().find(|&j| other(j).0 == other(*order.last().unwrap()).1 && !order.contains(&j)) {
            order.push(next);
        }
        for k in fan {
            if !order.contains(&k) {
                order.push(k);
            }
        }
        order
    }

    fn locate(&self, q: &[f64]) -> Option<(usize, Vec<f64>)> {
        geom::locate(&self.pts, &self.t, q)
    }
}

/// Query points from `(P)` or `(x, y[, z])` arguments.
fn query_points(args: &[Value], d: usize) -> R<Vec<Vec<f64>>> {
    let ms: Vec<Matrix> = args.iter().map(|v| v.to_matrix()).collect::<R<_>>()?;
    let pts = if ms.len() == 1 { geom::rows(&ms[0]) } else { geom::points_from(&ms)? };
    if pts.iter().any(|p| p.len() != d) {
        return rt("The dimension of the query points does not match the dimension of the triangulation.");
    }
    Ok(pts)
}

fn ids_arg(a: Option<&Value>, n: usize) -> R<Vec<usize>> {
    match a {
        Some(v) => Ok(v.to_matrix()?.data.iter().map(|&x| x as usize - 1).collect()),
        None => Ok((0..n).collect()),
    }
}

fn new_triangulation(it: &mut Interpreter, a: &[Value], delaunay: bool) -> Ret {
    let ms: Vec<Matrix> = (0..a.len()).map(|i| m_arg(a, i, "triangulation")).collect::<R<_>>()?;
    if delaunay {
        let (coords, constraints) = match ms.len() {
            1 => (ms[..1].to_vec(), None),
            2 if ms[0].cols() >= 2 && ms[0].cols() <= 3 && ms[1].cols() == 2 && ms[0].rows() != ms[1].rows() => (ms[..1].to_vec(), Some(())),
            2 | 3 => (ms.clone(), None),
            4 => (ms[..3].to_vec(), Some(())),
            _ => return rt("Wrong number of input arguments."),
        };
        if constraints.is_some() {
            return rt("Constrained Delaunay triangulations are not supported in this version.");
        }
        let pts = geom::points_from(&coords)?;
        let (uniq, _) = geom::unique_points_quiet(&pts);
        if uniq.len() < pts.len() {
            it.emit(OutputKind::Warning(
                "Duplicate data points have been detected and removed.\n The Triangulation indices are defined with respect to the unique set of points in delaunayTriangulation.".into(),
            ));
        }
        let d = uniq.first().map(|p| p.len()).unwrap_or(0);
        let tri = match Delaunay::new(&uniq) {
            Some(t) => t.simplices(),
            None => Vec::new(),
        };
        return one(make("delaunayTriangulation", vec![Value::Num(points_matrix(&uniq)), Value::Num(index_matrix(&tri, d + 1)), Value::empty()]));
    }
    let (t, pts) = match ms.len() {
        2 => (ms[0].clone(), ms[1].clone()),
        3 | 4 => {
            if ms[1..].iter().any(|m| !(m.is_2d() && m.dims[1] == 1)) {
                return rt("The input points must be specified in column-vector format.");
            }
            (ms[0].clone(), points_matrix(&geom::points_from(&ms[1..])?))
        }
        _ => return rt("Wrong number of input arguments."),
    };
    if pts.rows() < 3 {
        return rt("At least three input points must be provided to define a triangular mesh.");
    }
    if t.data.iter().any(|&v| v < 1.0 || v > pts.rows() as f64 || v.fract() != 0.0) {
        return rt("The triangulation connectivity is invalid: index out of range of the points.");
    }
    one(make("triangulation", vec![Value::Num(pts), Value::Num(t)]))
}

fn tri_method(m: &str, o: &Object, a: &[Value], nargout: usize) -> Ret {
    let tri = Tri::of(o)?;
    let d = tri.dim();
    let nt = tri.t.len();
    let nv = tri.t.first().map(|s| s.len()).unwrap_or(d + 1);
    Ok(Some(match m {
        "size" => {
            let s = [nt as f64, nv as f64];
            if nargout >= 2 { vec![Value::num(s[0]), Value::num(s[1])] } else { vec![Value::Num(Matrix::row(s.to_vec()))] }
        }
        "edges" => {
            let e = tri.edges();
            vec![Value::Num(index_matrix(&e.iter().map(|&(a, b)| vec![a, b]).collect::<Vec<_>>(), 2))]
        }
        "freeBoundary" => {
            let f = tri.free_boundary();
            let cols = f.first().map(|r| r.len()).unwrap_or(nv - 1);
            if nargout >= 2 {
                // Compact the vertex numbering to the boundary points, in order of appearance.
                let mut map = HashMap::new();
                let mut used = Vec::new();
                let rows: Vec<Vec<usize>> = f
                    .iter()
                    .map(|r| {
                        r.iter()
                            .map(|&v| {
                                *map.entry(v).or_insert_with(|| {
                                    used.push(v);
                                    used.len() - 1
                                })
                            })
                            .collect()
                    })
                    .collect();
                let pts: Vec<Vec<f64>> = used.iter().map(|&v| tri.pts[v].clone()).collect();
                vec![Value::Num(index_matrix(&rows, cols)), Value::Num(points_matrix(&pts))]
            } else {
                vec![Value::Num(index_matrix(&f, cols))]
            }
        }
        "neighbors" => {
            let fm = tri.facet_map();
            let ids = ids_arg(a.first(), nt)?;
            let n = ids.len();
            let mut data = vec![f64::NAN; n * nv];
            for (r, &k) in ids.iter().enumerate() {
                let s = &tri.t[k];
                for skip in 0..nv {
                    let mut f: Vec<usize> = s.iter().enumerate().filter(|&(i, _)| i != skip).map(|(_, &v)| v).collect();
                    f.sort_unstable();
                    if let Some(&other) = fm[&f].iter().find(|&&j| j != k) {
                        data[skip * n + r] = (other + 1) as f64;
                    }
                }
            }
            vec![Value::Num(Matrix::new(n, nv, data))]
        }
        "vertexAttachments" => {
            let ids = ids_arg(a.first(), tri.pts.len())?;
            let cells = ids.iter().map(|&v| Value::Num(Matrix::row(tri.attachments(v).into_iter().map(|k| (k + 1) as f64).collect()))).collect();
            vec![Value::Cell(CellArray::col(cells))]
        }
        "edgeAttachments" => {
            let edges: Vec<(usize, usize)> = match a {
                [e] => geom::rows(&e.to_matrix()?).into_iter().map(|r| (r[0] as usize - 1, r[1] as usize - 1)).collect(),
                [x, y] => x.to_matrix()?.data.iter().zip(&y.to_matrix()?.data).map(|(&p, &q)| (p as usize - 1, q as usize - 1)).collect(),
                _ => return rt("Wrong number of input arguments."),
            };
            let cells = edges
                .iter()
                .map(|&(p, q)| {
                    let has = |k: usize| tri.t[k].contains(&p) && tri.t[k].contains(&q);
                    let directed = |k: usize, from: usize, to: usize| {
                        let s = &tri.t[k];
                        (0..s.len()).any(|i| s[i] == from && s[(i + 1) % s.len()] == to)
                    };
                    let mut ks: Vec<usize> = (0..nt).filter(|&k| has(k)).collect();
                    if nv == 3 {
                        // The triangle on the right of p->q (containing q->p) comes first.
                        ks.sort_by_key(|&k| !directed(k, q, p));
                    }
                    Value::Num(Matrix::row(ks.into_iter().map(|k| (k + 1) as f64).collect()))
                })
                .collect();
            vec![Value::Cell(CellArray::col(cells))]
        }
        "pointLocation" => {
            let qs = query_points(a, d)?;
            let n = qs.len();
            let mut ti = vec![f64::NAN; n];
            let mut b = vec![f64::NAN; n * nv];
            for (i, q) in qs.iter().enumerate() {
                if let Some((k, w)) = tri.locate(q) {
                    ti[i] = (k + 1) as f64;
                    for (j, x) in w.into_iter().enumerate() {
                        b[j * n + i] = x;
                    }
                }
            }
            vec![col(ti), Value::Num(Matrix::new(n, nv, b))]
        }
        "nearestNeighbor" => {
            let qs = query_points(a, d)?;
            let (k, dist): (Vec<f64>, Vec<f64>) = qs.iter().map(|q| {
                let (i, dd) = geom::nearest(&tri.pts, q);
                ((i + 1) as f64, dd)
            }).unzip();
            vec![col(k), col(dist)]
        }
        "circumcenter" | "incenter" => {
            let ids = ids_arg(a.first(), nt)?;
            let n = ids.len();
            let mut c = vec![0.0; n * d];
            let mut r = vec![0.0; n];
            for (row, &k) in ids.iter().enumerate() {
                let s = &tri.t[k];
                let p: Vec<&[f64]> = s.iter().map(|&i| tri.pts[i].as_slice()).collect();
                let (center, radius) = if m == "circumcenter" {
                    match geom::circumsphere(&p) {
                        Some((cc, r2)) => (cc, r2.sqrt()),
                        None => (vec![f64::NAN; d], f64::NAN),
                    }
                } else {
                    incircle(&p)
                };
                for k2 in 0..d {
                    c[k2 * n + row] = center[k2];
                }
                r[row] = radius;
            }
            vec![Value::Num(Matrix::new(n, d, c)), col(r)]
        }
        "isConnected" => {
            let pairs: Vec<(usize, usize)> = match a {
                [e] => geom::rows(&e.to_matrix()?).into_iter().map(|r| (r[0] as usize - 1, r[1] as usize - 1)).collect(),
                [x, y] => x.to_matrix()?.data.iter().zip(&y.to_matrix()?.data).map(|(&p, &q)| (p as usize - 1, q as usize - 1)).collect(),
                _ => return rt("Wrong number of input arguments."),
            };
            let e: HashSet<(usize, usize)> = tri.edges().into_iter().collect();
            vec![logical_col(pairs.iter().map(|&(p, q)| e.contains(&(p.min(q), p.max(q)))).collect())]
        }
        "barycentricToCartesian" => {
            let ids = ids_arg(a.first(), nt)?;
            let b = geom::rows(&crate::builtins::arg(a, 1, m)?.to_matrix()?);
            let n = ids.len();
            let mut out = vec![0.0; n * d];
            for (row, &k) in ids.iter().enumerate() {
                for k2 in 0..d {
                    out[k2 * n + row] = tri.t[k].iter().zip(&b[row]).map(|(&i, &w)| w * tri.pts[i][k2]).sum();
                }
            }
            vec![Value::Num(Matrix::new(n, d, out))]
        }
        "cartesianToBarycentric" => {
            let ids = ids_arg(a.first(), nt)?;
            let q = geom::rows(&crate::builtins::arg(a, 1, m)?.to_matrix()?);
            let n = ids.len();
            let mut out = vec![f64::NAN; n * nv];
            for (row, &k) in ids.iter().enumerate() {
                if let Some(w) = geom::barycentric(&tri.pts, &tri.t[k], &q[row]) {
                    for (j, x) in w.into_iter().enumerate() {
                        out[j * n + row] = x;
                    }
                }
            }
            vec![Value::Num(Matrix::new(n, nv, out))]
        }
        "faceNormal" => {
            if nv != 3 {
                return rt("The triangulation must be a 2-D triangulation or a 3-D surface triangulation.");
            }
            let ids = ids_arg(a.first(), nt)?;
            let normals = tri.face_normals();
            let n = ids.len();
            let mut out = vec![0.0; n * 3];
            for (row, &k) in ids.iter().enumerate() {
                for c in 0..3 {
                    out[c * n + row] = normals[k][c];
                }
            }
            vec![Value::Num(Matrix::new(n, 3, out))]
        }
        "vertexNormal" => {
            if nv != 3 || d != 3 {
                return rt("The triangulation must be a 3-D surface triangulation.");
            }
            let ids = ids_arg(a.first(), tri.pts.len())?;
            let normals = tri.face_normals();
            let n = ids.len();
            let mut out = vec![0.0; n * 3];
            for (row, &v) in ids.iter().enumerate() {
                let mut s = [0.0; 3];
                for (k, t) in tri.t.iter().enumerate() {
                    if t.contains(&v) {
                        for c in 0..3 {
                            s[c] += normals[k][c];
                        }
                    }
                }
                let len = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
                for c in 0..3 {
                    out[c * n + row] = s[c] / len;
                }
            }
            vec![Value::Num(Matrix::new(n, 3, out))]
        }
        "featureEdges" => {
            if nv != 3 {
                return rt("The triangulation must be a 2-D triangulation or a 3-D surface triangulation.");
            }
            let angle = crate::builtins::arg(a, 0, m)?.scalar()?;
            let normals = tri.face_normals();
            let mut by_edge: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
            for (k, s) in tri.t.iter().enumerate() {
                for i in 0..3 {
                    let (p, q) = (s[i], s[(i + 1) % 3]);
                    by_edge.entry((p.min(q), p.max(q))).or_default().push(k);
                }
            }
            let mut e: Vec<(usize, usize)> = by_edge
                .iter()
                .filter(|(_, ks)| {
                    ks.len() != 2 || {
                        let (a, b) = (normals[ks[0]], normals[ks[1]]);
                        let dot = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
                        dot.acos() > angle
                    }
                })
                .map(|(&e, _)| e)
                .collect();
            e.sort_unstable();
            vec![Value::Num(index_matrix(&e.iter().map(|&(a, b)| vec![a, b]).collect::<Vec<_>>(), 2))]
        }
        "convexHull" => {
            let pts = &tri.pts;
            if d == 2 {
                let h = geom::hull2(pts).unwrap_or_default();
                let area = if h.is_empty() { 0.0 } else { geom::polygon_area(pts, &h) };
                vec![col(h.iter().map(|&i| (i + 1) as f64).collect()), Value::num(area)]
            } else {
                let Some(t) = Delaunay::new(pts) else { return rt("The triangulation is empty.") };
                let vol: f64 = t.simplices().iter().map(|s| geom::simplex_volume(pts, s)).sum();
                let mut f = t.hull();
                for r in f.iter_mut() {
                    let k = (0..r.len()).min_by_key(|&k| r[k]).unwrap();
                    r.rotate_left(k);
                }
                f.sort();
                vec![Value::Num(index_matrix(&f, d)), Value::num(vol)]
            }
        }
        "voronoiDiagram" => {
            let cc = geom::centers(&tri.pts, &tri.t);
            let mut v = vec![vec![f64::INFINITY; d]];
            v.extend(cc);
            let fm = tri.facet_map();
            let on_hull: HashSet<usize> = fm.iter().filter(|(_, ks)| ks.len() == 1).flat_map(|(f, _)| f.clone()).collect();
            let cells = (0..tri.pts.len())
                .map(|p| {
                    let mut ids: Vec<f64> = Vec::new();
                    if on_hull.contains(&p) {
                        ids.push(1.0);
                    }
                    let mut att = tri.attachments(p);
                    if !on_hull.contains(&p) && nv == 3 {
                        let k = (0..att.len()).min_by_key(|&k| att[k]).unwrap_or(0);
                        att.rotate_left(k);
                    }
                    ids.extend(att.into_iter().map(|k| (k + 2) as f64));
                    Value::Num(Matrix::row(ids))
                })
                .collect();
            vec![Value::Num(points_matrix(&v)), Value::Cell(CellArray::col(cells))]
        }
        "isInterior" => return rt("isInterior requires a constrained Delaunay triangulation, which is not supported in this version."),
        _ => return rt(format!("Undefined function '{m}' for input arguments of type '{}'.", o.class.name)),
    }))
}

/// Incenter and inradius of a triangle (or the insphere of a tetrahedron).
fn incircle(p: &[&[f64]]) -> (Vec<f64>, f64) {
    let d = p[0].len();
    let n = p.len();
    // Weights: the measure of the facet opposite each vertex.
    let facet = |skip: usize| -> f64 {
        let q: Vec<&[f64]> = (0..n).filter(|&i| i != skip).map(|i| p[i]).collect();
        if q.len() == 2 {
            geom::dist2(q[0], q[1]).sqrt()
        } else {
            let a = geom::sub(q[1], q[0]);
            let b = geom::sub(q[2], q[0]);
            let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
            (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() / 2.0
        }
    };
    let w: Vec<f64> = (0..n).map(facet).collect();
    let total: f64 = w.iter().sum();
    let center: Vec<f64> = (0..d).map(|k| (0..n).map(|i| w[i] * p[i][k]).sum::<f64>() / total).collect();
    let volume = {
        let idx: Vec<usize> = (0..n).collect();
        let pts: Vec<Vec<f64>> = p.iter().map(|q| q.to_vec()).collect();
        geom::simplex_volume(&pts, &idx)
    };
    (center, d as f64 * volume / total)
}

// ---- alphaShape ------------------------------------------------------------------------------

/// The Delaunay triangulation of an alpha shape's points with each simplex's circumradius.
struct Alpha {
    pts: Vec<Vec<f64>>,
    tri: Vec<Vec<usize>>,
    radius: Vec<f64>,
}

impl Alpha {
    fn new(pts: Vec<Vec<f64>>) -> Alpha {
        let tri = Delaunay::new(&pts).map(|t| t.simplices()).unwrap_or_default();
        let radius = tri
            .iter()
            .map(|s| {
                let p: Vec<&[f64]> = s.iter().map(|&i| pts[i].as_slice()).collect();
                geom::circumsphere(&p).map(|c| c.1.sqrt()).unwrap_or(f64::INFINITY)
            })
            .collect();
        Alpha { pts, tri, radius }
    }

    fn d(&self) -> usize {
        self.pts.first().map(|p| p.len()).unwrap_or(2)
    }

    fn measure(&self, k: usize) -> f64 {
        geom::simplex_volume(&self.pts, &self.tri[k])
    }

    fn spectrum(&self) -> Vec<f64> {
        let mut r: Vec<f64> = self.radius.iter().copied().filter(|r| r.is_finite()).collect();
        r.sort_by(|a, b| b.total_cmp(a));
        r.dedup();
        r
    }

    /// Connected components of a set of simplices, through shared facets or, for the
    /// regions of the shape (as MATLAB counts them), through any shared vertex.
    fn components(&self, set: &[usize], by_vertex: bool) -> Vec<Vec<usize>> {
        let keys = |s: &Vec<usize>| -> Vec<Vec<usize>> {
            if by_vertex {
                return s.iter().map(|&v| vec![v]).collect();
            }
            (0..s.len())
                .map(|skip| {
                    let mut f: Vec<usize> = s.iter().enumerate().filter(|&(i, _)| i != skip).map(|(_, &v)| v).collect();
                    f.sort_unstable();
                    f
                })
                .collect()
        };
        let mut by_facet: HashMap<Vec<usize>, Vec<usize>> = HashMap::new();
        for &k in set {
            for f in keys(&self.tri[k]) {
                by_facet.entry(f).or_default().push(k);
            }
        }
        let mut comp: HashMap<usize, usize> = HashMap::new();
        let mut out: Vec<Vec<usize>> = Vec::new();
        for &k in set {
            if comp.contains_key(&k) {
                continue;
            }
            let id = out.len();
            let mut stack = vec![k];
            let mut members = Vec::new();
            comp.insert(k, id);
            while let Some(c) = stack.pop() {
                members.push(c);
                for f in keys(&self.tri[c]) {
                    for &n in &by_facet[&f] {
                        if let std::collections::hash_map::Entry::Vacant(e) = comp.entry(n) {
                            e.insert(id);
                            stack.push(n);
                        }
                    }
                }
            }
            members.sort_unstable();
            out.push(members);
        }
        out
    }

    /// The regions of the shape at `alpha`, after filling small holes and dropping small
    /// regions, largest first.
    fn regions(&self, alpha: f64, hole: f64, region: f64) -> Vec<Vec<usize>> {
        // Radii equal to alpha up to rounding count as inside (alpha usually comes from the
        // spectrum, computed with different rounding).
        let mut inside: Vec<bool> = self.radius.iter().map(|&r| r <= alpha * (1.0 + 1e-12)).collect();
        if hole > 0.0 {
            // Holes: groups of excluded simplices that do not reach the convex hull.
            let outside: Vec<usize> = (0..self.tri.len()).filter(|&k| !inside[k]).collect();
            let mut facet_count: HashMap<Vec<usize>, usize> = HashMap::new();
            for s in &self.tri {
                for skip in 0..s.len() {
                    let mut f: Vec<usize> = s.iter().enumerate().filter(|&(i, _)| i != skip).map(|(_, &v)| v).collect();
                    f.sort_unstable();
                    *facet_count.entry(f).or_default() += 1;
                }
            }
            for c in self.components(&outside, false) {
                let touches_hull = c.iter().any(|&k| {
                    let s = &self.tri[k];
                    (0..s.len()).any(|skip| {
                        let mut f: Vec<usize> = s.iter().enumerate().filter(|&(i, _)| i != skip).map(|(_, &v)| v).collect();
                        f.sort_unstable();
                        facet_count[&f] == 1
                    })
                });
                let size: f64 = c.iter().map(|&k| self.measure(k)).sum();
                if !touches_hull && size <= hole {
                    for k in c {
                        inside[k] = true;
                    }
                }
            }
        }
        let set: Vec<usize> = (0..self.tri.len()).filter(|&k| inside[k]).collect();
        let mut regions: Vec<(f64, Vec<usize>)> = self
            .components(&set, true)
            .into_iter()
            .map(|c| (c.iter().map(|&k| self.measure(k)).sum(), c))
            .filter(|(m, _)| *m > region)
            .collect();
        regions.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1[0].cmp(&b.1[0])));
        regions.into_iter().map(|(_, c)| c).collect()
    }

    fn critical_all_points(&self) -> f64 {
        let mut best = vec![f64::INFINITY; self.pts.len()];
        for (k, s) in self.tri.iter().enumerate() {
            for &v in s {
                best[v] = best[v].min(self.radius[k]);
            }
        }
        best.into_iter().filter(|r| r.is_finite()).fold(0.0, f64::max)
    }

    fn critical_one_region(&self) -> f64 {
        let all = self.critical_all_points();
        let mut spec = self.spectrum();
        spec.reverse();
        spec.into_iter().filter(|&a| a >= all).find(|&a| self.regions(a, 0.0, 0.0).len() == 1).unwrap_or(all)
    }

    /// Boundary facets of a set of simplices, oriented outward; edges chained into loops,
    /// each starting from its lowest-numbered vertex.
    fn boundary(&self, set: &[usize]) -> Vec<Vec<usize>> {
        let t = Tri { pts: self.pts.clone(), t: set.iter().map(|&k| self.tri[k].clone()).collect() };
        let f = t.free_boundary();
        if self.d() != 2 {
            return f;
        }
        let mut out = Vec::with_capacity(f.len());
        let mut start = 0;
        for i in 0..f.len() {
            if i + 1 == f.len() || f[i][1] != f[i + 1][0] {
                let mut lp = f[start..=i].to_vec();
                if let Some(m) = (0..lp.len()).min_by_key(|&k| lp[k][0]) {
                    lp.rotate_left(m);
                }
                out.extend(lp);
                start = i + 1;
            }
        }
        out
    }
}

struct Shape {
    a: Alpha,
    alpha: f64,
    hole: f64,
    region: f64,
}

impl Shape {
    fn of(o: &Object) -> R<Shape> {
        let pts = geom::rows(&prop_m(o, "Points")?);
        Ok(Shape {
            a: Alpha::new(pts),
            alpha: prop(o, "Alpha").scalar()?,
            hole: prop(o, "HoleThreshold").scalar()?,
            region: prop(o, "RegionThreshold").scalar()?,
        })
    }

    fn regions(&self) -> Vec<Vec<usize>> {
        self.a.regions(self.alpha, self.hole, self.region)
    }

    /// The simplices of all regions, or of the requested ones (1-based IDs).
    fn selected(&self, ids: Option<&Value>) -> R<Vec<usize>> {
        let regions = self.regions();
        let mut out: Vec<usize> = match ids {
            Some(v) => {
                let mut out = Vec::new();
                for &r in &v.to_matrix()?.data {
                    let r = r as usize;
                    if r < 1 || r > regions.len() {
                        return rt("RegionID must be a positive integer less than or equal to the number of regions.");
                    }
                    out.extend(regions[r - 1].iter().copied());
                }
                out
            }
            None => regions.into_iter().flatten().collect(),
        };
        out.sort_unstable();
        Ok(out)
    }
}

fn new_alpha(a: &[Value]) -> Ret {
    let n_num = a.iter().take_while(|v| !v.is_text()).count();
    let ms: Vec<Matrix> = (0..n_num).map(|i| m_arg(a, i, "alphaShape")).collect::<R<_>>()?;
    let mut hole = 0.0;
    let mut region = 0.0;
    let mut i = n_num;
    while i + 1 < a.len() + 1 && i < a.len() {
        let name = a[i].text()?;
        let v = crate::builtins::arg(a, i + 1, "alphaShape")?.scalar()?;
        match name.to_ascii_lowercase().as_str() {
            "holethreshold" => hole = v,
            "regionthreshold" => region = v,
            _ => return rt(format!("Invalid parameter name '{name}'.")),
        }
        i += 2;
    }
    // (P), (P, a), (x, y), (x, y, a), (x, y, z), (x, y, z, a).
    let (coords, alpha) = match ms.len() {
        1 => (ms[..1].to_vec(), None),
        2 if ms[1].is_scalar() && ms[0].cols() > 1 => (ms[..1].to_vec(), Some(ms[1].data[0])),
        2 => (ms.clone(), None),
        3 if ms[2].is_scalar() && ms[0].numel() > 1 => (ms[..2].to_vec(), Some(ms[2].data[0])),
        3 => (ms.clone(), None),
        4 => (ms[..3].to_vec(), Some(ms[3].data[0])),
        _ => return rt("Wrong number of input arguments."),
    };
    let pts = geom::points_from(&coords)?;
    let alpha = match alpha {
        Some(a) => a,
        None => Alpha::new(pts.clone()).critical_all_points(),
    };
    one(make("alphaShape", vec![Value::Num(points_matrix(&pts)), Value::num(alpha), Value::num(hole), Value::num(region)]))
}

/// Query points, region IDs and the shape of the result.
type Queries = (Vec<Vec<f64>>, Option<Value>, Vec<usize>);

/// Queries as (shp, P) or (shp, x, y[, z]), optionally followed by region IDs.
fn alpha_queries(a: &[Value], d: usize) -> R<Queries> {
    let ms: Vec<Matrix> = a.iter().map(|v| v.to_matrix()).collect::<R<_>>()?;
    let coordinate_form = ms.len() >= d && ms[..d].iter().all(|m| m.dims == ms[0].dims);
    let (coords, ids) = if !coordinate_form {
        (ms[..1].to_vec(), a.get(1).cloned())
    } else {
        (ms[..d.min(ms.len())].to_vec(), a.get(d).cloned())
    };
    let dims = coords.first().map(|c| if coords.len() == 1 { vec![c.rows(), 1] } else { c.dims.clone() }).unwrap_or_default();
    let pts = if coords.len() == 1 { geom::rows(&coords[0]) } else { geom::points_from(&coords)? };
    Ok((pts, ids, dims))
}

fn alpha_method(m: &str, o: &Object, a: &[Value], nargout: usize) -> Ret {
    let s = Shape::of(o)?;
    let d = s.a.d();
    let measure = |set: &[usize]| -> f64 { set.iter().map(|&k| s.a.measure(k)).sum() };
    Ok(Some(match m {
        "alphaSpectrum" => vec![col(s.a.spectrum())],
        "criticalAlpha" => {
            let kind = crate::builtins::arg(a, 0, m)?.text()?;
            vec![Value::num(match kind.as_str() {
                "all-points" => s.a.critical_all_points(),
                "one-region" => s.a.critical_one_region(),
                _ => return rt("Type must be 'all-points' or 'one-region'."),
            })]
        }
        "numRegions" => vec![Value::num(s.regions().len() as f64)],
        "area" | "volume" => {
            if (m == "area") != (d == 2) {
                return rt(if m == "area" { "Use volume for 3-D alpha shapes." } else { "Use area for 2-D alpha shapes." });
            }
            let regions = s.regions();
            match a.first() {
                Some(ids) => {
                    let ids = ids.to_matrix()?;
                    let mut out = Vec::new();
                    for &r in &ids.data {
                        let r = r as usize;
                        if r < 1 || r > regions.len() {
                            return rt("RegionID must be a positive integer less than or equal to the number of regions.");
                        }
                        out.push(measure(&regions[r - 1]));
                    }
                    vec![Value::Num(Matrix::from_dims(ids.dims.clone(), out))]
                }
                None => vec![Value::num(measure(&regions.concat()))],
            }
        }
        "perimeter" | "surfaceArea" => {
            if (m == "perimeter") != (d == 2) {
                return rt(if m == "perimeter" { "Use surfaceArea for 3-D alpha shapes." } else { "Use perimeter for 2-D alpha shapes." });
            }
            let regions = s.regions();
            let size_of = |set: &[usize]| -> f64 {
                s.a.boundary(set)
                    .iter()
                    .map(|f| {
                        let p: Vec<&[f64]> = f.iter().map(|&i| s.a.pts[i].as_slice()).collect();
                        if p.len() == 2 {
                            geom::dist2(p[0], p[1]).sqrt()
                        } else {
                            let u = geom::sub(p[1], p[0]);
                            let v = geom::sub(p[2], p[0]);
                            let c = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                            (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() / 2.0
                        }
                    })
                    .sum()
            };
            match a.first() {
                Some(ids) => {
                    let ids = ids.to_matrix()?;
                    let mut out = Vec::new();
                    for &r in &ids.data {
                        let r = r as usize;
                        if r < 1 || r > regions.len() {
                            return rt("RegionID must be a positive integer less than or equal to the number of regions.");
                        }
                        out.push(size_of(&regions[r - 1]));
                    }
                    vec![Value::Num(Matrix::from_dims(ids.dims.clone(), out))]
                }
                None => vec![Value::num(regions.iter().map(|r| size_of(r)).sum())],
            }
        }
        "inShape" => {
            let (qs, ids, dims) = alpha_queries(a, d)?;
            let set = s.selected(ids.as_ref())?;
            let simplices: Vec<Vec<usize>> = set.iter().map(|&k| s.a.tri[k].clone()).collect();
            let r: Vec<f64> = qs.iter().map(|q| if geom::locate(&s.a.pts, &simplices, q).is_some() { 1.0 } else { 0.0 }).collect();
            vec![Value::Num(Matrix::from_dims(dims, r).with_kind(Kind::Logical))]
        }
        "alphaTriangulation" | "boundaryFacets" => {
            let set = s.selected(a.first())?;
            let rows: Vec<Vec<usize>> = if m == "alphaTriangulation" { set.iter().map(|&k| s.a.tri[k].clone()).collect() } else { s.a.boundary(&set) };
            let cols = if m == "alphaTriangulation" { d + 1 } else { d };
            if nargout >= 2 {
                let mut map = HashMap::new();
                let mut used = Vec::new();
                let compact: Vec<Vec<usize>> = rows
                    .iter()
                    .map(|r| {
                        r.iter()
                            .map(|&v| {
                                *map.entry(v).or_insert_with(|| {
                                    used.push(v);
                                    used.len() - 1
                                })
                            })
                            .collect()
                    })
                    .collect();
                // Triangulation points keep their original relative order; boundary points
                // are numbered along the boundary.
                let mut order: Vec<usize> = used.clone();
                if m == "alphaTriangulation" {
                    order.sort_unstable();
                }
                let renum: HashMap<usize, usize> = order.iter().enumerate().map(|(i, &v)| (v, i)).collect();
                let compact: Vec<Vec<usize>> = compact.iter().map(|r| r.iter().map(|&i| renum[&used[i]]).collect()).collect();
                let pts: Vec<Vec<f64>> = order.iter().map(|&v| s.a.pts[v].clone()).collect();
                vec![Value::Num(index_matrix(&compact, cols)), Value::Num(points_matrix(&pts))]
            } else {
                vec![Value::Num(index_matrix(&rows, cols))]
            }
        }
        "nearestNeighbor" => {
            let (qs, ids, dims) = alpha_queries(a, d)?;
            let set = s.selected(ids.as_ref())?;
            let mut on: Vec<usize> = s.a.boundary(&set).into_iter().flatten().collect();
            on.sort_unstable();
            on.dedup();
            // The nearest boundary vertex; MATLAB resolves ties toward the later point.
            let (k, dist): (Vec<f64>, Vec<f64>) = qs
                .iter()
                .map(|q| {
                    let mut best = (f64::NAN, f64::INFINITY);
                    for &i in &on {
                        let dd = geom::dist2(&s.a.pts[i], q);
                        if dd <= best.1 {
                            best = ((i + 1) as f64, dd);
                        }
                    }
                    (best.0, best.1.sqrt())
                })
                .unzip();
            vec![Value::Num(Matrix::from_dims(dims.clone(), k)), Value::Num(Matrix::from_dims(dims, dist))]
        }
        _ => return rt(format!("Undefined function '{m}' for input arguments of type 'alphaShape'.")),
    }))
}

/// `boundary(x, y[, z][, s])` and `boundary(P[, s])`: an alpha shape that envelops all points,
/// from the convex hull (s = 0) to the tightest single region (s = 1).
fn boundary(a: &[Value]) -> Ret {
    let ms: Vec<Matrix> = (0..a.len()).map(|i| m_arg(a, i, "boundary")).collect::<R<_>>()?;
    let (coords, shrink) = match ms.len() {
        1 => (ms.clone(), 0.5),
        2 if ms[1].is_scalar() && ms[0].cols() > 1 => (ms[..1].to_vec(), ms[1].data[0]),
        2 | 3 if ms.len() == 2 || !ms[2].is_scalar() || ms[0].is_scalar() => (ms.clone(), 0.5),
        3 => (ms[..2].to_vec(), ms[2].data[0]),
        4 => (ms[..3].to_vec(), ms[3].data[0]),
        _ => return rt("Wrong number of input arguments."),
    };
    if !(0.0..=1.0).contains(&shrink) {
        return rt("Shrink factor must be a scalar between 0 and 1.");
    }
    let pts = geom::points_from(&coords)?;
    let (uniq, orig) = geom::unique_points_quiet(&pts);
    let shape = Alpha::new(uniq);
    let d = shape.d();
    let one_region = shape.critical_one_region();
    // The shrink factor picks one of the alphas from the convex hull (largest) down to the
    // tightest single region, in equal steps.
    let spec: Vec<f64> = shape.spectrum().into_iter().filter(|&r| r >= one_region).collect();
    let n = spec.len().max(1);
    let idx = ((shrink * n as f64).floor() as usize).min(n - 1);
    let alpha = spec.get(idx).copied().unwrap_or(f64::INFINITY);
    let set = shape.regions(alpha, 0.0, 0.0).concat();
    let facets = shape.boundary(&set);
    let size: f64 = set.iter().map(|&k| shape.measure(k)).sum();
    let k: Matrix = if d == 2 {
        // Closed loop starting at the lowest index.
        let mut chain: Vec<usize> = facets.iter().map(|f| f[0]).collect();
        if let Some(start) = (0..chain.len()).min_by_key(|&i| chain[i]) {
            chain.rotate_left(start);
        }
        if let Some(&f) = chain.first() {
            chain.push(f);
        }
        Matrix::col(chain.into_iter().map(|i| (orig[i] + 1) as f64).collect())
    } else {
        index_matrix(&facets.iter().map(|f| f.iter().map(|&i| orig[i]).collect()).collect::<Vec<_>>(), 3)
    };
    Ok(Some(vec![Value::Num(k), Value::num(size)]))
}

// ---- dispatch --------------------------------------------------------------------------------

/// `matlivre_polyfun_method('Class.method', nargout, obj, args...)`: returns the outputs as a
/// cell row (the class methods assign it to varargout).
fn method_call(a: &[Value]) -> Ret {
    let which = crate::builtins::arg(a, 0, "matlivre_polyfun_method")?.text()?;
    let nargout = crate::builtins::arg(a, 1, "matlivre_polyfun_method")?.scalar()? as usize;
    let Some(Value::Object(o)) = a.get(2) else { return rt("Invalid object.") };
    let (class, m) = which.split_once('.').unwrap_or(("", ""));
    let rest = &a[3..];
    let out = match class {
        "alphaShape" => alpha_method(m, o, rest, nargout)?,
        _ => tri_method(m, o, rest, nargout)?,
    }
    .unwrap_or_default();
    one(Value::Cell(CellArray::row(out)))
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    match name {
        "griddedInterpolant" => new_gridded(a),
        "scatteredInterpolant" => new_scattered(a),
        "triangulation" => new_triangulation(it, a, false),
        "delaunayTriangulation" => new_triangulation(it, a, true),
        "alphaShape" => new_alpha(a),
        "boundary" => boundary(a),
        "matlivre_polyfun_method" => method_call(a),
        _ => Ok(None),
    }
}
