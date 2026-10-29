//! `polyshape` and `nsidedpoly`: 2-D polygons with the `Vertices`, `NumRegions` and `NumHoles`
//! properties and the common measuring and transforming methods.
//!
//! Like the other built-in classes, `polyshape` is an ordinary value class whose definition is
//! MATLAB source generated here; each method forwards to the hidden builtin
//! `matlivre_polyshape_method`. Regions are separated by NaN rows in `Vertices`; solid
//! boundaries are stored clockwise, as MATLAB stores them. Boolean operations (union,
//! intersect, ...) and hole detection are not covered yet.

use std::cell::RefCell;
use std::rc::Rc;

use crate::builtins::{one, Ret};
use crate::class::{ClassDef, Object};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &["polyshape", "nsidedpoly", "matlivre_polyshape_method"];

const METHODS: &[&str] = &["area", "perimeter", "centroid", "translate", "rotate", "scale", "numsides", "isinterior", "boundingbox", "plot", "regions"];

fn class_def() -> Rc<ClassDef> {
    thread_local! {
        static DEF: RefCell<Option<Rc<ClassDef>>> = const { RefCell::new(None) };
    }
    DEF.with(|d| {
        if let Some(def) = d.borrow().as_ref() {
            return def.clone();
        }
        let mut src = String::from("classdef polyshape\n  properties\n    Vertices\n    NumRegions\n    NumHoles\n  end\n  methods\n");
        for m in METHODS {
            src.push_str(&format!(
                "    function varargout = {m}(obj, varargin)\n      varargout = matlivre_polyshape_method('{m}', max(nargout, 1), obj, varargin{{:}});\n    end\n"
            ));
        }
        src.push_str("  end\nend\n");
        let ast = crate::class::parse_classdef(&src).expect("built-in class source parses");
        let def = Rc::new(ClassDef::build(ast, &[]));
        *d.borrow_mut() = Some(def.clone());
        def
    })
}

/// One boundary as (x, y) points, not closed (the last point is not the first repeated).
type Ring = Vec<(f64, f64)>;

fn signed_area(r: &Ring) -> f64 {
    let n = r.len();
    (0..n).map(|i| {
        let (a, b) = (r[i], r[(i + 1) % n]);
        a.0 * b.1 - b.0 * a.1
    }).sum::<f64>()
        / 2.0
}

/// The polyshape object for these boundaries: each made clockwise (reversed in place when
/// needed, keeping its first vertex), separated by NaN rows.
fn make(mut rings: Vec<Ring>) -> Value {
    rings.retain(|r| r.len() >= 3);
    for r in &mut rings {
        if signed_area(r) > 0.0 {
            r[1..].reverse();
        }
    }
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for (k, r) in rings.iter().enumerate() {
        if k > 0 {
            xs.push(f64::NAN);
            ys.push(f64::NAN);
        }
        for &(x, y) in r {
            xs.push(x);
            ys.push(y);
        }
    }
    let n = xs.len();
    let mut data = xs;
    data.extend(ys);
    let verts = if n == 0 { Matrix::new(0, 2, Vec::new()) } else { Matrix::new(n, 2, data) };
    let props = vec![Value::Num(verts), Value::num(rings.len() as f64), Value::num(0.0)];
    Value::Object(Object { class: class_def(), props: Rc::new(RefCell::new(props)) })
}

/// Boundaries from x and y vectors, NaN-separated, with consecutive duplicates and a closing
/// repeat of the first point removed.
fn rings_from(x: &[f64], y: &[f64]) -> Vec<Ring> {
    let mut rings = vec![Vec::new()];
    for (&a, &b) in x.iter().zip(y) {
        if a.is_nan() || b.is_nan() {
            rings.push(Vec::new());
            continue;
        }
        let r = rings.last_mut().unwrap();
        if r.last() != Some(&(a, b)) {
            r.push((a, b));
        }
    }
    for r in &mut rings {
        if r.len() > 1 && r.first() == r.last() {
            r.pop();
        }
    }
    rings
}

fn rings_of(o: &Object) -> R<Vec<Ring>> {
    let v = o.get("Vertices").unwrap_or_else(Value::empty).to_matrix()?;
    if v.is_empty() {
        return Ok(Vec::new());
    }
    let n = v.rows();
    Ok(rings_from(&v.data[..n], &v.data[n..]).into_iter().filter(|r| !r.is_empty()).collect())
}

fn sind(d: f64) -> f64 {
    if d % 180.0 == 0.0 { 0.0 } else { crate::builtins::sind_cosd(d).0 }
}

fn cosd(d: f64) -> f64 {
    if (d - 90.0) % 180.0 == 0.0 { 0.0 } else { crate::builtins::sind_cosd(d).1 }
}

/// Name-value options after the positional arguments.
fn options(a: &[Value]) -> R<Vec<(String, Value)>> {
    if a.len() % 2 != 0 {
        return rt("Name-value arguments must come in pairs.");
    }
    a.chunks(2).map(|c| Ok((c[0].text()?.to_lowercase(), c[1].clone()))).collect()
}

fn nsidedpoly(a: &[Value]) -> Ret {
    let n = a.first().map(|v| v.scalar()).transpose()?.unwrap_or(f64::NAN);
    if !(n >= 3.0) || n.fract() != 0.0 {
        return rt("Number of sides must be a positive integer larger than or equal to 3.");
    }
    let mut center = (0.0, 0.0);
    let mut radius = None;
    let mut side = None;
    for (k, v) in options(&a[1..])? {
        match k.as_str() {
            "center" => {
                let m = v.to_matrix()?;
                if m.numel() != 2 {
                    return rt("Center must be a two-element vector.");
                }
                center = (m.data[0], m.data[1]);
            }
            "radius" => radius = Some(v.scalar()?),
            "sidelength" => side = Some(v.scalar()?),
            other => return rt(format!("'{other}' is not a recognized parameter.")),
        }
    }
    let step = 360.0 / n;
    // Circumradius: given, from the side length, or 1.
    let r = radius.unwrap_or_else(|| side.map_or(1.0, |s| s / (2.0 * sind(180.0 / n))));
    // Clockwise from the vertex just left of straight down, so the bottom side is horizontal.
    let start = 270.0 - 180.0 / n;
    let ring: Ring = (0..n as usize).map(|k| {
        let t = start - step * k as f64;
        (center.0 + r * cosd(t), center.1 + r * sind(t))
    }).collect();
    one(make(vec![ring]))
}

fn constructor(a: &[Value]) -> Ret {
    match a.len() {
        0 => one(make(Vec::new())),
        _ => {
            let (x, y) = match a.get(1) {
                Some(v) if !v.is_text() => (a[0].to_matrix()?.data, v.to_matrix()?.data),
                _ => {
                    let p = a[0].to_matrix()?;
                    if p.cols() != 2 && !p.is_empty() {
                        return rt("Input vertices must be a two-column matrix.");
                    }
                    let n = p.rows();
                    (p.data[..n].to_vec(), p.data[n..].to_vec())
                }
            };
            if x.len() != y.len() {
                return rt("Input x and y vectors must have the same number of elements.");
            }
            one(make(rings_from(&x, &y)))
        }
    }
}

fn centroid(rings: &[Ring]) -> (f64, f64) {
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for r in rings {
        let n = r.len();
        for i in 0..n {
            let (p, q) = (r[i], r[(i + 1) % n]);
            let c = p.0 * q.1 - q.0 * p.1;
            a += c;
            cx += (p.0 + q.0) * c;
            cy += (p.1 + q.1) * c;
        }
    }
    if a == 0.0 { (f64::NAN, f64::NAN) } else { (cx / (3.0 * a), cy / (3.0 * a)) }
}

/// Inside or on the boundary of any region.
fn inside(rings: &[Ring], (x, y): (f64, f64)) -> bool {
    rings.iter().any(|r| {
        let n = r.len();
        let mut c = false;
        for i in 0..n {
            let (p, q) = (r[i], r[(i + 1) % n]);
            // On the edge.
            let cross = (q.0 - p.0) * (y - p.1) - (q.1 - p.1) * (x - p.0);
            if cross == 0.0 && x >= p.0.min(q.0) && x <= p.0.max(q.0) && y >= p.1.min(q.1) && y <= p.1.max(q.1) {
                return true;
            }
            if (p.1 > y) != (q.1 > y) && x < (q.0 - p.0) * (y - p.1) / (q.1 - p.1) + p.0 {
                c = !c;
            }
        }
        c
    })
}

fn map_points(o: &Object, f: impl Fn(f64, f64) -> (f64, f64)) -> R<Value> {
    let rings = rings_of(o)?;
    Ok(make(rings.into_iter().map(|r| r.into_iter().map(|(x, y)| f(x, y)).collect()).collect()))
}

/// Two numbers given as (a, b) or ([a b]).
fn pair(a: &[Value], i: usize, what: &str) -> R<(f64, f64)> {
    let m = a.get(i).ok_or_else(|| LangError::new(format!("Not enough input arguments for {what}."), 0))?.to_matrix()?;
    match (m.numel(), a.get(i + 1)) {
        (2, _) => Ok((m.data[0], m.data[1])),
        (1, Some(b)) => Ok((m.data[0], b.scalar()?)),
        (1, None) => Ok((m.data[0], m.data[0])),
        _ => rt(format!("Invalid {what}.")),
    }
}

fn method(it: &mut Interpreter, name: &str, o: &Object, a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let rings = rings_of(o)?;
    Ok(match name {
        "area" => vec![Value::num(rings.iter().map(|r| signed_area(r).abs()).sum())],
        "perimeter" => vec![Value::num(
            rings.iter().map(|r| (0..r.len()).map(|i| {
                let (p, q) = (r[i], r[(i + 1) % r.len()]);
                (q.0 - p.0).hypot(q.1 - p.1)
            }).sum::<f64>()).sum(),
        )],
        "centroid" => {
            let (cx, cy) = centroid(&rings);
            if nargout <= 1 { vec![Value::Num(Matrix::row(vec![cx, cy]))] } else { vec![Value::num(cx), Value::num(cy)] }
        }
        "numsides" => vec![Value::num(rings.iter().map(|r| r.len()).sum::<usize>() as f64)],
        "translate" => {
            let (dx, dy) = pair(a, 0, "translation")?;
            vec![map_points(o, |x, y| (x + dx, y + dy))?]
        }
        "scale" => {
            let (sx, sy) = pair(a, 0, "scale factor")?;
            let (cx, cy) = if a.len() > 1 && a[0].numel() == 2 || a.len() > 2 { pair(a, if a[0].numel() == 2 { 1 } else { 2 }, "reference point")? } else { (0.0, 0.0) };
            vec![map_points(o, |x, y| (cx + (x - cx) * sx, cy + (y - cy) * sy))?]
        }
        "rotate" => {
            let theta = a.first().map(|v| v.scalar()).transpose()?.unwrap_or(0.0);
            let (cx, cy) = if a.len() > 1 { pair(a, 1, "reference point")? } else { (0.0, 0.0) };
            let (c, s) = (cosd(theta), sind(theta));
            vec![map_points(o, |x, y| {
                let (u, v) = (x - cx, y - cy);
                (cx + c * u - s * v, cy + s * u + c * v)
            })?]
        }
        "isinterior" => {
            let (xs, ys) = match a.get(1) {
                Some(y) => (a[0].to_matrix()?.data, y.to_matrix()?.data),
                None => {
                    let p = a.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?.to_matrix()?;
                    let n = p.rows();
                    (p.data[..n].to_vec(), p.data[n..].to_vec())
                }
            };
            let tf: Vec<f64> = xs.iter().zip(&ys).map(|(&x, &y)| if inside(&rings, (x, y)) { 1.0 } else { 0.0 }).collect();
            vec![Value::Num(Matrix::col(tf).cast_to(Kind::Logical))]
        }
        "boundingbox" => {
            let pts: Vec<&(f64, f64)> = rings.iter().flatten().collect();
            let lim = |f: fn(&(f64, f64)) -> f64| {
                let v: Vec<f64> = pts.iter().map(|p| f(p)).collect();
                if v.is_empty() { Matrix::new(0, 0, Vec::new()) } else { Matrix::row(vec![v.iter().cloned().fold(f64::INFINITY, f64::min), v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)]) }
            };
            vec![Value::Num(lim(|p| p.0)), Value::Num(lim(|p| p.1))]
        }
        "regions" => {
            let parts: Vec<Value> = rings.into_iter().map(|r| make(vec![r])).collect();
            vec![Value::Cell(CellArray::col(parts))]
        }
        "plot" => {
            // Each region as a closed outline (filled patches are not drawn yet).
            let mut x = Vec::new();
            let mut y = Vec::new();
            for (k, r) in rings.iter().enumerate() {
                if k > 0 {
                    x.push(f64::NAN);
                    y.push(f64::NAN);
                }
                for &(px, py) in r.iter().chain(r.first()) {
                    x.push(px);
                    y.push(py);
                }
            }
            let mut args = vec![Value::Num(Matrix::col(x)), Value::Num(Matrix::col(y))];
            args.extend(a.iter().cloned());
            it.call_function("plot", args, 0)?;
            Vec::new()
        }
        _ => return rt(format!("Undefined function '{name}' for input arguments of type 'polyshape'.")),
    })
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    match name {
        "nsidedpoly" => nsidedpoly(a),
        "polyshape" => constructor(a),
        "matlivre_polyshape_method" => {
            let which = crate::builtins::arg(a, 0, name)?.text()?;
            let nargout = crate::builtins::arg(a, 1, name)?.scalar()? as usize;
            let Some(Value::Object(o)) = a.get(2) else { return rt("Invalid polyshape.") };
            let out = method(it, &which, o, &a[3..], nargout)?;
            one(Value::Cell(CellArray::row(out)))
        }
        _ => Ok(None),
    }
}
