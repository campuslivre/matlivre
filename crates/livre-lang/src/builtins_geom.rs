//! Computational geometry from MATLAB's `polyfun`: Delaunay triangulations in any dimension
//! (`delaunay`, `delaunayn`), convex hulls (`convhull`, `convhulln`), Voronoi diagrams
//! (`voronoi`, `voronoin`), point location (`dsearchn`, `tsearchn`) and scattered-data
//! interpolation (`griddata`, `griddatan`).
//!
//! The Delaunay triangulation is built incrementally (Bowyer–Watson) with a symbolic vertex at
//! infinity: every hull facet carries a "ghost" simplex joining it to that vertex, so points
//! outside the current hull need no enclosing super-simplex. The convex hull is read off the
//! ghosts. MATLAB uses Qhull, whose simplex order is an implementation detail; the facets and
//! orientations here follow MATLAB's conventions, but the order of the rows may differ.

use std::collections::HashMap;

use crate::builtins::{m_arg, one, Ret};
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &["delaunay", "delaunayn", "convhull", "convhulln", "voronoi", "voronoin", "dsearchn", "tsearchn", "griddata", "griddatan"];

const INF: usize = usize::MAX;

// ---- linear algebra on small dense matrices --------------------------------------------------

/// Determinant by Gaussian elimination with partial pivoting.
fn det(mut a: Vec<Vec<f64>>) -> f64 {
    let n = a.len();
    let mut d = 1.0;
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs())).unwrap();
        if a[p][c] == 0.0 {
            return 0.0;
        }
        if p != c {
            a.swap(p, c);
            d = -d;
        }
        d *= a[c][c];
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            let (top, bottom) = a.split_at_mut(r);
            for (x, &y) in bottom[0][c..].iter_mut().zip(&top[c][c..]) {
                *x -= f * y;
            }
        }
    }
    d
}

/// Solves `a x = b`; None when singular.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = a.len();
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[p][c] == 0.0 {
            return None;
        }
        a.swap(p, c);
        b.swap(p, c);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            let (top, bottom) = a.split_at_mut(r);
            for (x, &y) in bottom[0][c..].iter_mut().zip(&top[c][c..]) {
                *x -= f * y;
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|k| a[r][k] * x[k]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    Some(x)
}

pub(crate) fn sub(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b).map(|(x, y)| x - y).collect()
}

pub(crate) fn dist2(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}

/// Orientation of d+1 points in d dimensions: det of the edge vectors from the first.
fn orient(p: &[&[f64]]) -> f64 {
    det(p[1..].iter().map(|q| sub(q, p[0])).collect())
}

/// Circumcenter and squared radius of a simplex (None when degenerate).
pub(crate) fn circumsphere(p: &[&[f64]]) -> Option<(Vec<f64>, f64)> {
    let d = p[0].len();
    let a: Vec<Vec<f64>> = p[1..].iter().map(|q| sub(q, p[0]).iter().map(|v| 2.0 * v).collect()).collect();
    let b: Vec<f64> = p[1..].iter().map(|q| dist2(q, p[0])).collect();
    let rel = solve(a, b)?;
    let c: Vec<f64> = (0..d).map(|k| p[0][k] + rel[k]).collect();
    let r2 = rel.iter().map(|v| v * v).sum();
    Some((c, r2))
}

// ---- Delaunay --------------------------------------------------------------------------------

struct Simplex {
    v: Vec<usize>,
    alive: bool,
    sphere: Option<(Vec<f64>, f64)>,
}

/// An incremental Delaunay triangulation of distinct points.
pub(crate) struct Delaunay<'a> {
    pts: &'a [Vec<f64>],
    d: usize,
    simplices: Vec<Simplex>,
    facets: HashMap<Vec<usize>, Vec<usize>>,
    eps: f64,
}

fn facet_key(v: &[usize], skip: usize) -> Vec<usize> {
    let mut f: Vec<usize> = v.iter().enumerate().filter(|&(i, _)| i != skip).map(|(_, &x)| x).collect();
    f.sort_unstable();
    f
}

impl<'a> Delaunay<'a> {
    /// Triangulates `pts` (all distinct). Errors when they do not span the space.
    pub fn new(pts: &'a [Vec<f64>]) -> Option<Delaunay<'a>> {
        let d = pts.first()?.len();
        let scale = pts.iter().flatten().fold(0.0f64, |m, v| m.max(v.abs())).max(f64::MIN_POSITIVE);
        let mut t = Delaunay { pts, d, simplices: Vec::new(), facets: HashMap::new(), eps: 1e-12 * scale.powi(d as i32) };
        let first = t.initial()?;
        let mut first_sorted = first.clone();
        first_sorted.sort_unstable();
        for i in 0..pts.len() {
            if first_sorted.binary_search(&i).is_err() {
                t.insert(i);
            }
        }
        Some(t)
    }

    fn point(&self, i: usize) -> &[f64] {
        &self.pts[i]
    }

    /// d+1 affinely independent points, as a positively oriented simplex with its ghosts.
    fn initial(&mut self) -> Option<Vec<usize>> {
        let d = self.d;
        let mut chosen = vec![0usize];
        let mut basis: Vec<Vec<f64>> = Vec::new();
        let span = self.pts.iter().map(|p| dist2(p, &self.pts[0])).fold(0.0, f64::max).sqrt();
        for (i, p) in self.pts.iter().enumerate().skip(1) {
            if chosen.len() == d + 1 {
                break;
            }
            // Component of p - p0 orthogonal to the chosen directions.
            let mut r = sub(p, &self.pts[0]);
            for b in &basis {
                let dot: f64 = r.iter().zip(b).map(|(x, y)| x * y).sum();
                r.iter_mut().zip(b).for_each(|(x, y)| *x -= dot * y);
            }
            let norm = r.iter().map(|x| x * x).sum::<f64>().sqrt();
            if norm > 1e-10 * span {
                basis.push(r.iter().map(|x| x / norm).collect());
                chosen.push(i);
            }
        }
        if chosen.len() < d + 1 {
            return None;
        }
        let mut v = chosen.clone();
        if self.orient_of(&v) < 0.0 {
            v.swap(0, 1);
        }
        self.add(v.clone());
        for i in 0..=d {
            let mut g = v.clone();
            g[i] = INF;
            let (a, b) = if i == 0 { (1, 2) } else if i == 1 { (0, 2) } else { (0, 1) };
            g.swap(a, b);
            self.add(g);
        }
        Some(chosen)
    }

    fn orient_of(&self, v: &[usize]) -> f64 {
        let p: Vec<&[f64]> = v.iter().map(|&i| self.point(i)).collect();
        orient(&p)
    }

    fn add(&mut self, v: Vec<usize>) -> usize {
        let id = self.simplices.len();
        let sphere = if v.contains(&INF) {
            None
        } else {
            let p: Vec<&[f64]> = v.iter().map(|&i| self.point(i)).collect();
            circumsphere(&p)
        };
        for k in 0..v.len() {
            self.facets.entry(facet_key(&v, k)).or_default().push(id);
        }
        self.simplices.push(Simplex { v, alive: true, sphere });
        id
    }

    fn remove(&mut self, id: usize) {
        self.simplices[id].alive = false;
        let v = self.simplices[id].v.clone();
        for k in 0..v.len() {
            if let Some(list) = self.facets.get_mut(&facet_key(&v, k)) {
                list.retain(|&s| s != id);
            }
        }
    }

    fn neighbor(&self, id: usize, k: usize) -> Option<usize> {
        let key = facet_key(&self.simplices[id].v, k);
        self.facets.get(&key)?.iter().copied().find(|&s| s != id && self.simplices[s].alive)
    }

    fn in_sphere(&self, id: usize, p: &[f64]) -> bool {
        match &self.simplices[id].sphere {
            Some((c, r2)) => dist2(c, p) < r2 * (1.0 - 1e-12),
            None => false,
        }
    }

    fn conflict(&self, id: usize, p: usize) -> bool {
        let s = &self.simplices[id];
        match s.v.iter().position(|&x| x == INF) {
            None => self.in_sphere(id, self.point(p)),
            Some(k) => {
                let mut v = s.v.clone();
                v[k] = p;
                let o = self.orient_of(&v);
                if o > self.eps {
                    true
                } else if o >= -self.eps {
                    // On the hull facet's plane: in conflict when inside the facet, which is
                    // when the real simplex behind it is in conflict.
                    self.neighbor(id, k).map(|n| self.in_sphere(n, self.point(p))).unwrap_or(false)
                } else {
                    false
                }
            }
        }
    }

    fn insert(&mut self, p: usize) {
        let Some(start) = (0..self.simplices.len()).find(|&s| self.simplices[s].alive && self.conflict(s, p)) else { return };
        let mut cavity = vec![start];
        let mut seen = std::collections::HashSet::from([start]);
        let mut boundary = Vec::new();
        let mut i = 0;
        while i < cavity.len() {
            let s = cavity[i];
            i += 1;
            for k in 0..=self.d {
                match self.neighbor(s, k) {
                    Some(n) if seen.contains(&n) => {}
                    Some(n) if self.conflict(n, p) => {
                        seen.insert(n);
                        cavity.push(n);
                    }
                    _ => boundary.push((s, k)),
                }
            }
        }
        let new: Vec<Vec<usize>> = boundary
            .iter()
            .map(|&(s, k)| {
                let mut v = self.simplices[s].v.clone();
                v[k] = p;
                v
            })
            .collect();
        for s in cavity {
            self.remove(s);
        }
        for v in new {
            self.add(v);
        }
    }

    /// The finite simplices, positively oriented.
    pub fn simplices(&self) -> Vec<Vec<usize>> {
        self.simplices.iter().filter(|s| s.alive && !s.v.contains(&INF)).map(|s| s.v.clone()).collect()
    }

    /// Hull facets, oriented so that the right-hand normal points outward.
    pub fn hull(&self) -> Vec<Vec<usize>> {
        self.simplices
            .iter()
            .filter(|s| s.alive && s.v.contains(&INF))
            .map(|s| {
                // Replacing INF by an outside point gives a positive simplex; moving that
                // point to the end flips the sign for odd shifts, so fix the order up.
                let k = s.v.iter().position(|&x| x == INF).unwrap();
                let mut f: Vec<usize> = s.v.clone();
                f.remove(k);
                // Moving INF from position k to the end is (d - k) transpositions.
                if (self.d - k) % 2 == 1 {
                    f.swap(0, 1);
                }
                f
            })
            .collect()
    }
}

// ---- point sets ------------------------------------------------------------------------------

/// Distinct points and the first original index of each; emits MATLAB's duplicate warning.
pub(crate) fn unique_points(it: &mut Interpreter, pts: &[Vec<f64>]) -> (Vec<Vec<f64>>, Vec<usize>) {
    let mut seen: HashMap<Vec<u64>, usize> = HashMap::new();
    let mut out = Vec::new();
    let mut orig = Vec::new();
    for (i, p) in pts.iter().enumerate() {
        let key: Vec<u64> = p.iter().map(|v| if *v == 0.0 { 0 } else { v.to_bits() }).collect();
        if seen.contains_key(&key) {
            continue;
        }
        seen.insert(key, out.len());
        out.push(p.clone());
        orig.push(i);
    }
    if out.len() < pts.len() {
        it.emit(OutputKind::Warning("Duplicate data points have been detected and removed.\n Some point indices will not be referenced by the triangulation.".into()));
    }
    (out, orig)
}

/// Rows of an n-by-d matrix as points.
pub(crate) fn rows(m: &Matrix) -> Vec<Vec<f64>> {
    let (n, d) = (m.rows(), m.cols());
    (0..n).map(|i| (0..d).map(|k| m.data[k * n + i]).collect()).collect()
}

/// Points from separate coordinate vectors (`x, y[, z]`) or from one n-by-d matrix.
pub(crate) fn points_from(coords: &[Matrix]) -> R<Vec<Vec<f64>>> {
    if coords.len() == 1 {
        return Ok(rows(&coords[0]));
    }
    let n = coords[0].numel();
    if coords.iter().any(|c| c.numel() != n) {
        return rt("The input coordinates must be vectors of the same length.");
    }
    Ok((0..n).map(|i| coords.iter().map(|c| c.data[i]).collect()).collect())
}

pub(crate) fn index_matrix(rows_: &[Vec<usize>], cols: usize) -> Matrix {
    let n = rows_.len();
    let mut data = vec![0.0; n * cols];
    for (i, r) in rows_.iter().enumerate() {
        for (k, &v) in r.iter().enumerate() {
            data[k * n + i] = (v + 1) as f64;
        }
    }
    Matrix::new(n, cols, data)
}

/// Delaunay simplices of `pts` in original indices.
fn triangulate(it: &mut Interpreter, pts: &[Vec<f64>]) -> R<Vec<Vec<usize>>> {
    let (uniq, orig) = unique_points(it, pts);
    let Some(t) = Delaunay::new(&uniq) else {
        return rt("Error computing the Delaunay triangulation. The points may be collinear.");
    };
    Ok(t.simplices().into_iter().map(|s| s.into_iter().map(|i| orig[i]).collect()).collect())
}

pub(crate) fn simplex_volume(pts: &[Vec<f64>], s: &[usize]) -> f64 {
    let p: Vec<&[f64]> = s.iter().map(|&i| pts[i].as_slice()).collect();
    let d = p.len() - 1;
    orient(&p).abs() / (1..=d).map(|k| k as f64).product::<f64>()
}

/// 2-D hull by Andrew's monotone chain, keeping points on hull edges; counterclockwise,
/// starting from the lowest index on the hull, closed.
pub(crate) fn hull2(pts: &[Vec<f64>]) -> Option<Vec<usize>> {
    let mut idx: Vec<usize> = (0..pts.len()).collect();
    idx.sort_by(|&a, &b| pts[a][0].total_cmp(&pts[b][0]).then(pts[a][1].total_cmp(&pts[b][1])).then(a.cmp(&b)));
    idx.dedup_by(|a, b| pts[*a] == pts[*b]);
    if idx.len() < 3 {
        return None;
    }
    let cross = |o: usize, a: usize, b: usize| (pts[a][0] - pts[o][0]) * (pts[b][1] - pts[o][1]) - (pts[a][1] - pts[o][1]) * (pts[b][0] - pts[o][0]);
    let chain = |iter: &mut dyn Iterator<Item = usize>| {
        let mut h: Vec<usize> = Vec::new();
        for p in iter {
            while h.len() >= 2 && cross(h[h.len() - 2], h[h.len() - 1], p) < 0.0 {
                h.pop();
            }
            h.push(p);
        }
        h
    };
    let lower = chain(&mut idx.iter().copied());
    let upper = chain(&mut idx.iter().rev().copied());
    let mut h: Vec<usize> = lower[..lower.len() - 1].to_vec();
    h.extend_from_slice(&upper[..upper.len() - 1]);
    // All collinear: the chains fold back on themselves.
    let area: f64 = (0..h.len()).map(|k| cross(h[0], h[k], h[(k + 1) % h.len()])).sum();
    if area == 0.0 {
        return None;
    }
    let start = (0..h.len()).min_by_key(|&k| h[k]).unwrap();
    h.rotate_left(start);
    h.push(h[0]);
    Some(h)
}

pub(crate) fn polygon_area(pts: &[Vec<f64>], h: &[usize]) -> f64 {
    (0..h.len() - 1).map(|k| pts[h[k]][0] * pts[h[k + 1]][1] - pts[h[k + 1]][0] * pts[h[k]][1]).sum::<f64>().abs() / 2.0
}

fn num_args(a: &[Value], name: &str) -> R<(Vec<Matrix>, Vec<Value>)> {
    let n = a.iter().take_while(|v| !v.is_text()).count();
    Ok(((0..n).map(|i| m_arg(a, i, name)).collect::<R<_>>()?, a[n..].to_vec()))
}

fn convhull(a: &[Value], name: &str) -> Ret {
    let (m, _opts) = num_args(a, name)?;
    if m.is_empty() {
        return rt("Not enough input arguments.");
    }
    let pts = points_from(&m)?;
    let d = pts.first().map(|p| p.len()).unwrap_or(0);
    let (uniq, orig) = unique_points_quiet(&pts);
    if d == 2 && name == "convhull" {
        if uniq.len() < 3 {
            return rt("Error computing the convex hull. Not enough unique points specified.");
        }
        let Some(h) = hull2(&uniq) else { return rt("Error computing the convex hull. The points may be collinear.") };
        let area = polygon_area(&uniq, &h);
        let k: Vec<f64> = h.iter().map(|&i| (orig[i] + 1) as f64).collect();
        return Ok(Some(vec![Value::Num(Matrix::col(k)), Value::num(area)]));
    }
    if uniq.len() < d + 1 {
        return rt("Error computing the convex hull. Not enough unique points specified.");
    }
    let Some(t) = Delaunay::new(&uniq) else { return rt("Error computing the convex hull. The points may be collinear.") };
    let volume: f64 = t.simplices().iter().map(|s| simplex_volume(&uniq, s)).sum();
    let mut facets: Vec<Vec<usize>> = t.hull().into_iter().map(|f| f.into_iter().map(|i| orig[i]).collect()).collect();
    if d == 2 {
        // convhulln in 2-D: the edges as a counterclockwise chain.
        let h = hull2(&uniq).unwrap_or_default();
        facets = h.windows(2).map(|w| vec![orig[w[0]], orig[w[1]]]).collect();
    } else if name == "convhull" {
        // Outward normals, each facet starting from its smallest index, rows sorted.
        for f in facets.iter_mut() {
            let k = (0..f.len()).min_by_key(|&k| f[k]).unwrap();
            f.rotate_left(k);
        }
        facets.sort();
    } else {
        // convhulln keeps Qhull's inward orientation.
        for f in facets.iter_mut() {
            f.swap(0, 1);
            let k = (0..f.len()).min_by_key(|&k| f[k]).unwrap();
            let len = f.len();
            f.rotate_left((k + 1) % len);
        }
        facets.sort_by(|a, b| a[a.len() - 1].cmp(&b[b.len() - 1]).then(a.cmp(b)));
    }
    Ok(Some(vec![Value::Num(index_matrix(&facets, d)), Value::num(volume)]))
}

pub(crate) fn unique_points_quiet(pts: &[Vec<f64>]) -> (Vec<Vec<f64>>, Vec<usize>) {
    let mut seen: HashMap<Vec<u64>, usize> = HashMap::new();
    let mut out = Vec::new();
    let mut orig = Vec::new();
    for (i, p) in pts.iter().enumerate() {
        let key: Vec<u64> = p.iter().map(|v| if *v == 0.0 { 0 } else { v.to_bits() }).collect();
        if seen.insert(key, out.len()).is_none() {
            out.push(p.clone());
            orig.push(i);
        }
    }
    (out, orig)
}

fn delaunay(it: &mut Interpreter, a: &[Value], name: &str) -> Ret {
    let (m, _opts) = num_args(a, name)?;
    if m.is_empty() {
        return rt("Not enough input arguments.");
    }
    let pts = points_from(&m)?;
    let d = pts.first().map(|p| p.len()).unwrap_or(0);
    let tri = triangulate(it, &pts)?;
    one(Value::Num(index_matrix(&tri, d + 1)))
}

// ---- Voronoi ---------------------------------------------------------------------------------

/// Circumcenters of the Delaunay simplices.
pub(crate) fn centers(pts: &[Vec<f64>], tri: &[Vec<usize>]) -> Vec<Vec<f64>> {
    tri.iter()
        .map(|s| {
            let p: Vec<&[f64]> = s.iter().map(|&i| pts[i].as_slice()).collect();
            circumsphere(&p).map(|c| c.0).unwrap_or_else(|| vec![f64::NAN; pts[0].len()])
        })
        .collect()
}

fn voronoin(it: &mut Interpreter, a: &[Value]) -> Ret {
    let x = m_arg(a, 0, "voronoin")?;
    let pts = rows(&x);
    let d = x.cols();
    let (uniq, orig) = unique_points(it, &pts);
    let Some(t) = Delaunay::new(&uniq) else { return rt("Error computing the Voronoi diagram. The points may be collinear.") };
    let tri = t.simplices();
    let cc = centers(&uniq, &tri);
    let on_hull: std::collections::HashSet<usize> = t.hull().into_iter().flatten().collect();
    // V(1,:) is the point at infinity; vertex k + 1 is the circumcenter of simplex k.
    let mut vdata: Vec<Vec<f64>> = vec![vec![f64::INFINITY; d]];
    vdata.extend(cc.iter().cloned());
    let mut cells = Vec::with_capacity(pts.len());
    for u in 0..orig.len() {
        let fan: Vec<usize> = (0..tri.len()).filter(|&k| tri[k].contains(&u)).collect();
        let mut around: Vec<usize> = if d == 2 {
            // Walk the fan clockwise: triangle [u, a, b] (counterclockwise) is followed by
            // the one whose last vertex is a.
            let other = |k: usize| {
                let s = &tri[k];
                let i = s.iter().position(|&x| x == u).unwrap();
                (s[(i + 1) % 3], s[(i + 2) % 3])
            };
            let start = fan.iter().copied().find(|&k| !fan.iter().any(|&j| other(j).0 == other(k).1)).unwrap_or(fan[0]);
            let mut order = vec![start];
            while let Some(next) = fan.iter().copied().find(|&j| other(j).1 == other(*order.last().unwrap()).0) {
                if next == start || order.len() > fan.len() {
                    break;
                }
                order.push(next);
            }
            order
        } else {
            fan
        };
        // Row 1 of V is the point at infinity; simplex k's circumcenter is row k + 2.
        around.iter_mut().for_each(|k| *k += 2);
        if on_hull.contains(&u) {
            if d == 2 {
                around.push(1);
            } else {
                around.insert(0, 1);
            }
        }
        cells.push(around);
    }
    // Duplicate sites share the cell of their first occurrence.
    let mut by_point = vec![Vec::new(); pts.len()];
    let mut first_of: HashMap<Vec<u64>, usize> = HashMap::new();
    for (u, &o) in orig.iter().enumerate() {
        first_of.insert(pts[o].iter().map(|v| v.to_bits()).collect(), u);
    }
    for (i, p) in pts.iter().enumerate() {
        let key: Vec<u64> = p.iter().map(|v| v.to_bits()).collect();
        if let Some(&u) = first_of.get(&key) {
            by_point[i] = cells[u].clone();
        }
    }
    let n = vdata.len();
    let mut vm = vec![0.0; n * d];
    for (i, v) in vdata.iter().enumerate() {
        for k in 0..d {
            vm[k * n + i] = v[k];
        }
    }
    let c = CellArray::col(by_point.into_iter().map(|cell| Value::Num(Matrix::row(cell.into_iter().map(|k| k as f64).collect()))).collect());
    Ok(Some(vec![Value::Num(Matrix::new(n, d, vm)), Value::Cell(c)]))
}

fn voronoi(it: &mut Interpreter, a: &[Value]) -> Ret {
    let x = m_arg(a, 0, "voronoi")?;
    let y = m_arg(a, 1, "voronoi")?;
    let pts = points_from(&[x, y])?;
    let (uniq, _) = unique_points(it, &pts);
    let Some(t) = Delaunay::new(&uniq) else { return rt("Error computing the Delaunay triangulation. The points may be collinear.") };
    let tri = t.simplices();
    let cc = centers(&uniq, &tri);
    // Each interior Delaunay edge gives a finite Voronoi edge; each hull edge a ray, drawn to
    // a point well outside the data.
    let mut edges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (k, s) in tri.iter().enumerate() {
        for i in 0..3 {
            let (a, b) = (s[i], s[(i + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push(k);
        }
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in &uniq {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    let mut keys: Vec<_> = edges.keys().copied().collect();
    keys.sort();
    let (mut vx, mut vy) = (Vec::new(), Vec::new());
    for key in keys {
        let ts = &edges[&key];
        let (a, b) = key;
        let c0 = &cc[ts[0]];
        if ts.len() == 2 {
            let c1 = &cc[ts[1]];
            vx.extend([c0[0], c1[0]]);
            vy.extend([c0[1], c1[1]]);
        } else {
            // Outward normal of hull edge ab (the third vertex is on the inside).
            let s = &tri[ts[0]];
            let third = *s.iter().find(|&&v| v != a && v != b).unwrap();
            let (pa, pb, pc) = (&uniq[a], &uniq[b], &uniq[third]);
            let mut n = [pb[1] - pa[1], pa[0] - pb[0]];
            let mid = [(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0];
            if n[0] * (pc[0] - mid[0]) + n[1] * (pc[1] - mid[1]) > 0.0 {
                n = [-n[0], -n[1]];
            }
            let len = (n[0] * n[0] + n[1] * n[1]).sqrt();
            let far = 2.0 * span;
            vx.extend([c0[0], c0[0] + n[0] / len * far]);
            vy.extend([c0[1], c0[1] + n[1] / len * far]);
        }
    }
    let m = vx.len() / 2;
    Ok(Some(vec![Value::Num(Matrix::new(2, m, vx)), Value::Num(Matrix::new(2, m, vy))]))
}

// ---- point location --------------------------------------------------------------------------

/// Barycentric coordinates of `q` in simplex `s`.
pub(crate) fn barycentric(pts: &[Vec<f64>], s: &[usize], q: &[f64]) -> Option<Vec<f64>> {
    let d = q.len();
    let p0 = &pts[s[0]];
    let a: Vec<Vec<f64>> = (0..d).map(|r| (1..=d).map(|c| pts[s[c]][r] - p0[r]).collect()).collect();
    let b = sub(q, p0);
    let l = solve(a, b)?;
    let mut w = vec![1.0 - l.iter().sum::<f64>()];
    w.extend(l);
    Some(w)
}

/// The first simplex containing `q` (with a small tolerance) and its barycentric weights.
pub(crate) fn locate(pts: &[Vec<f64>], tri: &[Vec<usize>], q: &[f64]) -> Option<(usize, Vec<f64>)> {
    if q.iter().any(|v| v.is_nan()) {
        return None;
    }
    tri.iter().enumerate().find_map(|(k, s)| {
        let w = barycentric(pts, s, q)?;
        if w.iter().all(|&x| x >= -1e-12) { Some((k, w)) } else { None }
    })
}

pub(crate) fn nearest(pts: &[Vec<f64>], q: &[f64]) -> (usize, f64) {
    let mut best = (0, f64::INFINITY);
    for (i, p) in pts.iter().enumerate() {
        let d = dist2(p, q);
        if d < best.1 {
            best = (i, d);
        }
    }
    (best.0, best.1.sqrt())
}

fn dsearchn(a: &[Value]) -> Ret {
    let x = m_arg(a, 0, "dsearchn")?;
    let pts = rows(&x);
    // dsearchn(X, XI), dsearchn(X, T, XI), dsearchn(X, T, XI, outval).
    let (t, xi, outval) = match a.len() {
        2 => (None, m_arg(a, 1, "dsearchn")?, None),
        3 => (Some(m_arg(a, 1, "dsearchn")?), m_arg(a, 2, "dsearchn")?, None),
        _ => (Some(m_arg(a, 1, "dsearchn")?), m_arg(a, 2, "dsearchn")?, Some(m_arg(a, 3, "dsearchn")?.data[0])),
    };
    if xi.cols() != x.cols() {
        return rt("X and XI should have the same column dimension.");
    }
    let qs = rows(&xi);
    let tri: Option<Vec<Vec<usize>>> = match (&t, outval) {
        (Some(t), Some(_)) => Some(rows(t).into_iter().map(|r| r.into_iter().map(|v| v as usize - 1).collect()).collect()),
        _ => None,
    };
    let mut k = Vec::with_capacity(qs.len());
    let mut d = Vec::with_capacity(qs.len());
    for q in &qs {
        if let (Some(tri), Some(o)) = (&tri, outval) {
            if locate(&pts, tri, q).is_none() {
                k.push(o);
                d.push(o);
                continue;
            }
        }
        let (i, dist) = nearest(&pts, q);
        k.push((i + 1) as f64);
        d.push(dist);
    }
    Ok(Some(vec![Value::Num(Matrix::col(k)), Value::Num(Matrix::col(d))]))
}

fn tsearchn(a: &[Value]) -> Ret {
    let x = m_arg(a, 0, "tsearchn")?;
    let t = m_arg(a, 1, "tsearchn")?;
    let xi = m_arg(a, 2, "tsearchn")?;
    let pts = rows(&x);
    let tri: Vec<Vec<usize>> = rows(&t).into_iter().map(|r| r.into_iter().map(|v| v as usize - 1).collect()).collect();
    let qs = rows(&xi);
    let nv = t.cols();
    let n = qs.len();
    let mut idx = vec![f64::NAN; n];
    let mut bary = vec![f64::NAN; n * nv];
    for (i, q) in qs.iter().enumerate() {
        if let Some((k, w)) = locate(&pts, &tri, q) {
            idx[i] = (k + 1) as f64;
            for (j, v) in w.into_iter().enumerate() {
                bary[j * n + i] = v;
            }
        }
    }
    Ok(Some(vec![Value::Num(Matrix::col(idx)), Value::Num(Matrix::new(n, nv, bary))]))
}

// ---- scattered interpolation -----------------------------------------------------------------

/// A scattered-data interpolant over distinct points (shared by griddata and
/// scatteredInterpolant).
pub(crate) struct Scattered {
    pub pts: Vec<Vec<f64>>,
    pub tri: Vec<Vec<usize>>,
    /// Hull facets (for the nearest-facet linear extrapolation).
    pub hull: Vec<Vec<usize>>,
}

impl Scattered {
    pub fn new(pts: Vec<Vec<f64>>) -> R<Scattered> {
        let (tri, hull) = match Delaunay::new(&pts) {
            Some(t) => (t.simplices(), t.hull()),
            None => return rt("Error computing the Delaunay triangulation. The points may be collinear."),
        };
        Ok(Scattered { pts, tri, hull })
    }

    pub fn linear(&self, v: &[f64], q: &[f64]) -> Option<f64> {
        let (k, w) = locate(&self.pts, &self.tri, q)?;
        Some(self.tri[k].iter().zip(&w).map(|(&i, &wi)| wi * v[i]).sum())
    }

    /// Linear extrapolation outside the hull: the plane of the hull simplex nearest to `q`.
    /// (MATLAB estimates boundary gradients differently, so values outside the hull differ.)
    pub fn extrapolate_linear(&self, v: &[f64], q: &[f64]) -> f64 {
        if q.iter().any(|x| x.is_nan()) {
            return f64::NAN;
        }
        let hull: std::collections::HashSet<Vec<usize>> = self.hull.iter().map(|f| {
            let mut f = f.clone();
            f.sort_unstable();
            f
        }).collect();
        let mut best: Option<(f64, usize)> = None;
        for (k, s) in self.tri.iter().enumerate() {
            let on_hull = (0..s.len()).any(|skip| {
                let mut f: Vec<usize> = s.iter().enumerate().filter(|&(i, _)| i != skip).map(|(_, &x)| x).collect();
                f.sort_unstable();
                hull.contains(&f)
            });
            if !on_hull {
                continue;
            }
            let c: Vec<f64> = (0..q.len()).map(|d| s.iter().map(|&i| self.pts[i][d]).sum::<f64>() / s.len() as f64).collect();
            let dd = dist2(&c, q);
            if best.map(|b| dd < b.0).unwrap_or(true) {
                best = Some((dd, k));
            }
        }
        let Some((_, k)) = best else { return f64::NAN };
        match barycentric(&self.pts, &self.tri[k], q) {
            Some(w) => self.tri[k].iter().zip(&w).map(|(&i, &wi)| wi * v[i]).sum(),
            None => f64::NAN,
        }
    }

    pub fn nearest(&self, v: &[f64], q: &[f64]) -> f64 {
        if q.iter().any(|x| x.is_nan()) {
            return f64::NAN;
        }
        v[nearest(&self.pts, q).0]
    }

    /// Sibson's natural-neighbor interpolation in 2-D (NaN outside the hull).
    pub fn natural(&self, v: &[f64], q: &[f64]) -> Option<f64> {
        locate(&self.pts, &self.tri, q)?;
        if let Some(i) = self.pts.iter().position(|p| p[0] == q[0] && p[1] == q[1]) {
            return Some(v[i]);
        }
        let cc = |a: &[f64], b: &[f64], c: &[f64]| circumsphere(&[a, b, c]).map(|s| s.0);
        // The cavity: triangles whose circumcircle contains q.
        let cavity: Vec<usize> = (0..self.tri.len())
            .filter(|&k| {
                let s = &self.tri[k];
                circumsphere(&[&self.pts[s[0]], &self.pts[s[1]], &self.pts[s[2]]]).map(|(c, r2)| dist2(&c, q) < r2 * (1.0 + 1e-12)).unwrap_or(false)
            })
            .collect();
        let mut nbrs: Vec<usize> = cavity.iter().flat_map(|&k| self.tri[k].clone()).collect();
        nbrs.sort_unstable();
        nbrs.dedup();
        // Neighbors in angular order around q.
        nbrs.sort_by(|&a, &b| {
            let fa = (self.pts[a][1] - q[1]).atan2(self.pts[a][0] - q[0]);
            let fb = (self.pts[b][1] - q[1]).atan2(self.pts[b][0] - q[0]);
            fa.total_cmp(&fb)
        });
        let m = nbrs.len();
        let mut total = 0.0;
        let mut acc = 0.0;
        for j in 0..m {
            let (prev, cur, next) = (nbrs[(j + m - 1) % m], nbrs[j], nbrs[(j + 1) % m]);
            let p = &self.pts;
            let (Some(c_next), Some(c_prev)) = (cc(q, &p[cur], &p[next]), cc(q, &p[prev], &p[cur])) else { continue };
            // Old cavity triangles at `cur`, ordered angularly between next and prev.
            let mut olds: Vec<Vec<f64>> = cavity
                .iter()
                .filter(|&&k| self.tri[k].contains(&cur))
                .filter_map(|&k| {
                    let s = &self.tri[k];
                    circumsphere(&[&p[s[0]], &p[s[1]], &p[s[2]]]).map(|c| c.0)
                })
                .collect();
            let base = (c_next[1] - p[cur][1]).atan2(c_next[0] - p[cur][0]);
            let ang = |c: &[f64]| {
                let a = (c[1] - p[cur][1]).atan2(c[0] - p[cur][0]) - base;
                a.rem_euclid(2.0 * std::f64::consts::PI)
            };
            olds.sort_by(|a, b| ang(a).total_cmp(&ang(b)));
            let mut poly = vec![c_next.clone()];
            poly.extend(olds);
            poly.push(c_prev);
            let area = (0..poly.len()).map(|k| {
                let (a, b) = (&poly[k], &poly[(k + 1) % poly.len()]);
                a[0] * b[1] - b[0] * a[1]
            }).sum::<f64>().abs() / 2.0;
            total += area;
            acc += area * v[cur];
        }
        if total == 0.0 { None } else { Some(acc / total) }
    }
}

/// Biharmonic spline interpolation (griddata's 'v4' method), in 2-D.
fn v4(pts: &[Vec<f64>], v: &[f64], qs: &[Vec<f64>]) -> R<Vec<f64>> {
    let g = |r2: f64| if r2 == 0.0 { 0.0 } else { r2 * (0.5 * r2.ln() - 1.0) };
    let n = pts.len();
    let a: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| g(dist2(&pts[i], &pts[j]))).collect()).collect();
    let Some(w) = solve(a, v.to_vec()) else { return rt("Matrix is singular to working precision.") };
    Ok(qs.iter().map(|q| (0..n).map(|j| w[j] * g(dist2(q, &pts[j]))).sum()).collect())
}

fn griddata(it: &mut Interpreter, a: &[Value], name: &str) -> Ret {
    let (m, opts) = num_args(a, name)?;
    let method = match opts.first() {
        Some(o) => o.text()?.to_ascii_lowercase(),
        None => "linear".into(),
    };
    // griddata(x,y,v,xq,yq) / (x,y,z,v,xq,yq,zq); griddatan(X,v,Xq).
    let (coords, v, qcoords, mesh) = if name == "griddatan" {
        if m.len() < 3 {
            return rt("Not enough input arguments.");
        }
        (vec![m[0].clone()], m[1].clone(), vec![m[2].clone()], false)
    } else {
        match m.len() {
            5 => (m[0..2].to_vec(), m[2].clone(), m[3..5].to_vec(), true),
            7 => (m[0..3].to_vec(), m[3].clone(), m[4..7].to_vec(), true),
            _ => return rt("Wrong number of input arguments."),
        }
    };
    let pts = points_from(&coords)?;
    if pts.len() != v.numel() {
        return rt("The number of input points and values must match.");
    }
    let d = pts.first().map(|p| p.len()).unwrap_or(0);
    // Query points: row x with column y forms a grid (as meshgrid).
    let (qpts, qdims, grids) = if mesh {
        let same = qcoords.iter().all(|q| q.dims == qcoords[0].dims);
        if !same && d == 2 && qcoords[0].is_vector() && qcoords[1].is_vector() {
            let (xs, ys) = (&qcoords[0].data, &qcoords[1].data);
            let (nx, ny) = (xs.len(), ys.len());
            let mut pts = Vec::with_capacity(nx * ny);
            for &x in xs {
                for &y in ys {
                    pts.push(vec![x, y]);
                }
            }
            let gx = Matrix::new(ny, nx, pts.iter().map(|p| p[0]).collect());
            let gy = Matrix::new(ny, nx, pts.iter().map(|p| p[1]).collect());
            (pts, vec![ny, nx], Some((gx, gy)))
        } else if !same {
            return rt("The query coordinates must be the same size.");
        } else {
            (points_from(&qcoords)?, qcoords[0].dims.clone(), None)
        }
    } else {
        let q = &qcoords[0];
        (rows(q), vec![q.rows(), 1], None)
    };
    // Duplicate points are merged with the mean of their values.
    let (uniq, orig) = unique_points_quiet(&pts);
    let mut vals = vec![0.0; uniq.len()];
    let mut ims = vec![0.0; uniq.len()];
    let mut counts = vec![0.0; uniq.len()];
    let mut where_: HashMap<Vec<u64>, usize> = HashMap::new();
    for (u, &o) in orig.iter().enumerate() {
        where_.insert(pts[o].iter().map(|x| if *x == 0.0 { 0 } else { x.to_bits() }).collect(), u);
    }
    for (i, p) in pts.iter().enumerate() {
        let u = where_[&p.iter().map(|x| if *x == 0.0 { 0 } else { x.to_bits() }).collect::<Vec<u64>>()];
        vals[u] += v.data[i];
        ims[u] += v.im_at(i);
        counts[u] += 1.0;
    }
    if uniq.len() < pts.len() {
        it.emit(OutputKind::Warning("Duplicate data points have been detected and removed - corresponding values have been averaged.".into()));
    }
    for u in 0..uniq.len() {
        vals[u] /= counts[u];
        ims[u] /= counts[u];
    }
    let eval = |vals: &[f64]| -> R<Vec<f64>> {
        match method.as_str() {
            "v4" => {
                if d != 2 {
                    return rt("'V4' is only supported for 2-D interpolation.");
                }
                v4(&uniq, vals, &qpts)
            }
            "linear" | "nearest" | "natural" => {
                let s = Scattered::new(uniq.clone())?;
                Ok(qpts
                    .iter()
                    .map(|q| match method.as_str() {
                        "linear" => s.linear(vals, q).unwrap_or(f64::NAN),
                        "nearest" => s.nearest(vals, q),
                        _ => {
                            if d == 2 { s.natural(vals, q).unwrap_or(f64::NAN) } else { s.linear(vals, q).unwrap_or(f64::NAN) }
                        }
                    })
                    .collect())
            }
            "cubic" => rt("The 'cubic' method of griddata is not supported in this version."),
            _ => rt(format!("Invalid interpolation method '{method}'.")),
        }
    };
    let re = eval(&vals)?;
    let mut out = Matrix::from_dims(qdims, re);
    if v.is_complex() {
        out.im = Some(eval(&ims)?);
        out.narrow();
    }
    match grids {
        Some((gx, gy)) => Ok(Some(vec![Value::Num(gx), Value::Num(gy), Value::Num(out)])),
        None => one(Value::Num(out)),
    }
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return crate::polyfun_classes::call(it, name, a, nargout);
    }
    match name {
        "delaunay" | "delaunayn" => delaunay(it, a, name),
        "convhull" | "convhulln" => convhull(a, name),
        "voronoi" => voronoi(it, a),
        "voronoin" => voronoin(it, a),
        "dsearchn" => dsearchn(a),
        "tsearchn" => tsearchn(a),
        "griddata" | "griddatan" => {
            let r = griddata(it, a, name)?;
            // [Xq,Yq,Vq] = griddata(...) on a grid; one output is Vq.
            match r {
                Some(v) if v.len() == 3 && nargout < 2 => one(v[2].clone()),
                other => Ok(other),
            }
        }
        _ => crate::polyfun_classes::call(it, name, a, nargout),
    }
}
