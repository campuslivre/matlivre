//! `zplane`: the pole-zero plot, drawn with the same line and text objects MATLAB creates (the
//! zeros `o`, the poles `x`, a dotted unit circle with the axes cross, and multiplicity labels).

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;
use std::f64::consts::PI;

pub const NAMES: &[&str] = &["zplane"];

/// Width over height of the default axes box (0.775 * 560 by 0.815 * 420 pixels).
const BOX_RATIO: f64 = 434.0 / 342.3;
const BLUE: [f64; 3] = [0.0, 0.4470, 0.7410];

type C = (f64, f64);

fn text_arg(s: &str) -> Value {
    Value::string(s)
}

fn vector_of(v: &Value, pos: usize) -> R<Matrix> {
    match v {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single | Kind::Logical) => {
            if m.dims.len() > 2 {
                return rt("Data cannot have more than 2 dimensions.");
            }
            Ok(m.clone())
        }
        Value::Num(m) if matches!(m.kind, Kind::Int(..)) => rt("Invalid data type. Input matrix must be double or single."),
        Value::Num(m) if m.kind == Kind::Char => {
            if pos == 1 {
                rt("Expected input number 2, den, to be one of these types:  double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64")
            } else {
                rt("Argument following 'like' must be a variable with a data type that supports ONES, such as a double or single variable.")
            }
        }
        _ => rt("Undefined function 'imag' for input arguments of type 'cell'."),
    }
}

fn complex_of(m: &Matrix) -> Vec<C> {
    let im = m.im.clone().unwrap_or_else(|| vec![0.0; m.numel()]);
    (0..m.numel()).map(|i| (m.data[i], im[i])).collect()
}

fn roots_of(it: &mut Interpreter, c: &[C], single: bool) -> R<Vec<C>> {
    if c.is_empty() {
        return Ok(vec![]);
    }
    if c.iter().any(|z| !z.0.is_finite() || !z.1.is_finite()) {
        return rt("Input to ROOTS must not contain NaN or Inf.");
    }
    let re: Vec<f64> = c.iter().map(|z| z.0).collect();
    let im: Vec<f64> = c.iter().map(|z| z.1).collect();
    let mut m = if im.iter().any(|v| *v != 0.0) { Matrix::complex(vec![1, c.len()], re, im) } else { Matrix::from_dims(vec![1, c.len()], re) };
    if single {
        m.kind = Kind::Single;
    }
    let r = it.call_function("roots", vec![Value::Num(m)], 1)?.remove(0).to_matrix()?;
    Ok(complex_of(&r))
}

/// Zeros and poles from the arguments: polynomials when a row vector is given, roots otherwise.
fn zeros_poles(it: &mut Interpreter, a: &[Value]) -> R<(Vec<Vec<C>>, Vec<Vec<C>>, bool)> {
    let b = vector_of(&a[0], 0)?;
    let den = if a.len() > 1 && !crate::gfx::is_gfx(&a[1]) { Some(vector_of(&a[1], 1)?) } else { None };
    let single = b.kind == Kind::Single || den.as_ref().map_or(false, |d| d.kind == Kind::Single);
    let is_row = |m: &Matrix| m.dims[0] == 1 && m.numel() > 1;
    let is_col = |m: &Matrix| m.dims[1] == 1 && m.numel() > 1;
    let is_mat = |m: &Matrix| m.dims[0] > 1 && m.dims[1] > 1;
    if is_mat(&b) || den.as_ref().map_or(false, |d| is_mat(d)) {
        // Each column is a separate set of zeros (or poles).
        let cols = |m: &Matrix| -> Vec<Vec<C>> {
            let c = complex_of(m);
            let r = m.dims[0];
            (0..m.dims[1]).map(|j| c[j * r..(j + 1) * r].to_vec()).collect()
        };
        let zs = if is_mat(&b) { cols(&b) } else if b.numel() == 0 { vec![vec![]] } else { vec![complex_of(&b)] };
        let ps = match &den {
            Some(d) if is_mat(d) => cols(d),
            Some(d) if d.numel() > 0 && is_mat(&b) => return rt("Precision must be a positive integer scalar."),
            _ => vec![vec![]],
        };
        if is_row(&b) || is_col(&b) {
            return rt("Precision must be a positive integer scalar.");
        }
        return Ok((zs, ps, single));
    }
    let poly = is_row(&b) || den.as_ref().map_or(false, |d| is_row(d));
    if poly {
        if is_col(&b) || den.as_ref().map_or(false, |d| is_col(d)) {
            return rt("When specifying polynomials, both vectors must be rows.");
        }
        let bc = complex_of(&b);
        let ac = match &den {
            Some(d) if d.numel() > 0 => complex_of(d),
            _ => vec![(1.0, 0.0)],
        };
        if bc.iter().chain(ac.iter()).any(|z| !z.0.is_finite() || !z.1.is_finite()) {
            return rt("Input to ROOTS must not contain NaN or Inf.");
        }
        if ac.is_empty() || ac.iter().all(|z| z.0 == 0.0 && z.1 == 0.0) {
            return rt("Denominator cannot be zero or empty.");
        }
        if ac[0].0 == 0.0 && ac[0].1 == 0.0 {
            return rt("Denominator must have non-zero leading coefficient.");
        }
        let mut z = roots_of(it, &bc, single)?;
        let mut p = roots_of(it, &ac, single)?;
        if !bc.is_empty() {
            if bc.len() > ac.len() {
                let mut q = vec![(0.0, 0.0); bc.len() - ac.len()];
                q.extend(p);
                p = q;
            } else if ac.len() > bc.len() {
                let mut q = vec![(0.0, 0.0); ac.len() - bc.len()];
                q.extend(z);
                z = q;
            }
        }
        Ok((vec![z], vec![p], single))
    } else {
        let z = complex_of(&b);
        let p = den.as_ref().map(complex_of).unwrap_or_default();
        Ok((vec![z], vec![p], single))
    }
}

/// Width over height of the current axes box in pixels.
fn box_ratio(it: &mut Interpreter) -> f64 {
    let get = |it: &mut Interpreter, h: Value, prop: &str| -> Option<Vec<f64>> {
        let v = it.call_function("get", vec![h, Value::string(prop)], 1).ok()?.into_iter().next()?;
        let m = v.to_matrix().ok()?;
        (m.numel() == 4).then(|| m.data.clone())
    };
    let ax = it.current_axes_handle();
    let fig = it.call_function("gcf", vec![], 1).ok().and_then(|v| v.into_iter().next());
    match (get(it, ax, "Position"), fig.and_then(|f| get(it, f, "Position"))) {
        (Some(a), Some(f)) if a[3] > 0.0 && f[3] > 0.0 => (a[2] * f[2]) / (a[3] * f[3]),
        _ => BOX_RATIO,
    }
}

fn round_single(v: f64) -> f64 {
    v as f32 as f64
}

fn points(z: &[C], single: bool) -> (Vec<f64>, Vec<f64>) {
    if z.is_empty() {
        return (vec![f64::NAN], vec![0.0]);
    }
    let all_real = z.iter().all(|c| c.1 == 0.0);
    let mut x: Vec<f64> = z.iter().map(|c| c.0).collect();
    let mut y: Vec<f64> = z.iter().map(|c| if all_real && !single { c.1 + 1e-50 } else { c.1 }).collect();
    if single {
        x.iter_mut().for_each(|v| *v = round_single(*v));
        y.iter_mut().for_each(|v| *v = round_single(*v));
    }
    (x, y)
}

/// Roots that coincide within 1e-3 are labelled with their multiplicity.
fn multiplicities(z: &[C]) -> Vec<(C, usize)> {
    let mut s: Vec<C> = z.to_vec();
    s.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)));
    let mut used = vec![false; s.len()];
    let mut out = vec![];
    for i in 0..s.len() {
        if used[i] {
            continue;
        }
        let mut count = 1;
        used[i] = true;
        for j in i + 1..s.len() {
            if !used[j] && (s[i].0 - s[j].0).hypot(s[i].1 - s[j].1) < 1e-3 {
                used[j] = true;
                count += 1;
            }
        }
        if count > 1 {
            out.push((s[i], count));
        }
    }
    out.reverse();
    out
}

pub fn call(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 3 {
        return rt("One or more output arguments not assigned during call to \"varargout\".");
    }
    let (zsets, psets, single) = zeros_poles(it, a)?;
    // The axes: a trailing axes handle selects them.
    if let Some(ax) = a.iter().skip(1).find(|v| crate::gfx::is_gfx(v)) {
        crate::builtins_plot::select_axes(it, &[ax.clone()])?;
    }
    it.call_function("newplot", vec![], 0)?;
    let all: Vec<C> = zsets.iter().chain(psets.iter()).flatten().cloned().collect();
    // The circle gets more points the closer a root lies to it.
    let dmin = all.iter().map(|c| (c.0.hypot(c.1) - 1.0).abs()).fold(f64::INFINITY, f64::min);
    let n = if dmin.is_finite() { ((1.0 / dmin).round().min(50000.0)).max(100.0) as usize } else { 100 };
    let circ: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            let t = 2.0 * PI * k as f64 / (n as f64 - 1.0);
            (t.cos(), t.sin())
        })
        .collect();
    let mut xs: Vec<f64> = circ.iter().map(|c| c.0).collect();
    let mut ys: Vec<f64> = circ.iter().map(|c| c.1).collect();
    for c in &all {
        xs.push(c.0);
        ys.push(c.1);
    }
    let fin = |v: &Vec<f64>| v.iter().cloned().filter(|x| x.is_finite()).fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(x), hi.max(x)));
    let (x0, x1) = fin(&xs);
    let (y0, y1) = fin(&ys);
    let ratio = box_ratio(it);
    let (mut lx, mut ly) = (1.1 * (x1 - x0), 1.1 * (y1 - y0));
    if lx / ly < ratio {
        lx = ly * ratio;
    } else {
        ly = lx / ratio;
    }
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let xlim = [cx - lx / 2.0, cx + lx / 2.0];
    let ylim = [cy - ly / 2.0, cy + ly / 2.0];
    let color = Value::Num(Matrix::from_dims(vec![1, 3], BLUE.to_vec()));
    let vec_value = |v: Vec<f64>| -> Value {
        let mut m = Matrix::from_dims(vec![1, v.len()], v);
        if single {
            m.kind = Kind::Single;
        }
        Value::Num(m)
    };
    let line = |it: &mut Interpreter, x: Vec<f64>, y: Vec<f64>, props: Vec<Value>| -> R<Value> {
        let mut args = vec![vec_value(x), vec_value(y), text_arg("Color"), color.clone()];
        args.extend(props);
        Ok(it.call_function("line", args, 1)?.remove(0))
    };
    let mut hz_ids: Vec<u64> = vec![];
    for z in &zsets {
        let (zx, zy) = points(z, single);
        let h = line(it, zx, zy, vec![text_arg("Marker"), text_arg("o"), text_arg("MarkerSize"), scalar_value(7.0), text_arg("LineStyle"), text_arg("none")])?;
        hz_ids.extend(crate::gfx::ids_of(&h).unwrap_or_default());
    }
    let mut hp_ids: Vec<u64> = vec![];
    for p in &psets {
        let (px, py) = points(p, single);
        let h = line(it, px, py, vec![text_arg("Marker"), text_arg("x"), text_arg("MarkerSize"), scalar_value(8.0), text_arg("LineStyle"), text_arg("none")])?;
        hp_ids.extend(crate::gfx::ids_of(&h).unwrap_or_default());
    }
    let mut cxv: Vec<f64> = circ.iter().map(|c| c.0).collect();
    let mut cyv: Vec<f64> = circ.iter().map(|c| c.1).collect();
    let (hx, hy) = ((xlim[1] - xlim[0]) / 2.0 * 201.0, (ylim[1] - ylim[0]) / 2.0 * 201.0);
    cxv.extend([f64::NAN, cx - hx, cx + hx, f64::NAN, 0.0, 0.0]);
    cyv.extend([f64::NAN, 0.0, 0.0, f64::NAN, cy - hy, cy + hy]);
    let hc = line(it, cxv, cyv, vec![text_arg("LineStyle"), text_arg(":")])?;
    let mut ht_ids: Vec<u64> = crate::gfx::ids_of(&hc).unwrap_or_default();
    let dx = 0.0125 * (xlim[1] - xlim[0]);
    for set in zsets.iter().chain(psets.iter()) {
        let all_real = set.iter().all(|c| c.1 == 0.0) && !single;
        for (pos, mult) in multiplicities(set) {
            let t = it.call_function(
                "text",
                vec![
                    scalar_value(pos.0 + dx),
                    scalar_value(pos.1 + if all_real { 1e-50 } else { 0.0 }),
                    text_arg(&mult.to_string()),
                    text_arg("HorizontalAlignment"),
                    text_arg("left"),
                    text_arg("VerticalAlignment"),
                    text_arg("bottom"),
                ],
                1,
            )?;
            ht_ids.extend(crate::gfx::ids_of(&t[0]).unwrap_or_default());
        }
    }
    let hz = crate::gfx::handles(it, &hz_ids);
    let hp = crate::gfx::handles(it, &hp_ids);
    let ax = it.current_axes_handle();
    let set = |it: &mut Interpreter, props: Vec<Value>| -> R<()> {
        let mut args = vec![ax.clone()];
        args.extend(props);
        it.call_function("set", args, 0)?;
        Ok(())
    };
    set(
        it,
        vec![
            text_arg("DataAspectRatio"),
            Value::Num(Matrix::from_dims(vec![1, 3], vec![1.0, 1.0, 1.0])),
            text_arg("XLim"),
            real_row(xlim.to_vec()),
            text_arg("YLim"),
            real_row(ylim.to_vec()),
            text_arg("Box"),
            text_arg("on"),
        ],
    )?;
    it.call_function("title", vec![text_arg("Pole-Zero Plot")], 0)?;
    it.call_function("xlabel", vec![text_arg("Real Part")], 0)?;
    it.call_function("ylabel", vec![text_arg("Imaginary Part")], 0)?;
    let ht = crate::gfx::handles_row(it, &ht_ids);
    let mut outs = vec![hz, hp, ht];
    outs.truncate(nargout.max(1));
    out(outs)
}
