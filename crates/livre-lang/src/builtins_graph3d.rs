//! 3-D plots (`plot3`, `scatter3`, `surf`, `mesh`, `contour3`, `quiver3`), surfaces in 2-D
//! (`pcolor`, `contour`, `contourf`), images (`image`, `imagesc`, `imshow`), colormaps
//! (`colormap`, `parula`, `jet`, ...), color limits, `view`, `shading`, and the shape
//! generators `peaks`, `sphere`, `cylinder` and `ellipsoid`.

use crate::builtins::{arg, none, one, Ret};
use crate::builtins_plot::{apply_props, handles_out};
use crate::error::{rt, LangError};
use crate::graphics::{self, LineStyle, Marker, Series, SeriesKind, XAxisKind};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const COLORMAPS: &[&str] = &["parula", "turbo", "sky", "abyss", "jet", "hot", "gray", "cool", "spring", "summer", "autumn", "winter", "bone", "copper", "pink", "hsv", "lines", "prism", "flag", "white"];

pub const NAMES: &[&str] = &[
    "parula", "turbo", "sky", "abyss", "jet", "hot", "gray", "cool", "spring", "summer", "autumn", "winter", "bone", "copper", "pink", "hsv", "lines", "prism",
    "flag", "white", "colormap", "clim", "caxis", "view", "shading", "plot3", "scatter3", "stem3", "surf", "surfc", "mesh", "meshc", "meshz", "surface", "pcolor",
    "contour", "contourf", "contour3", "contourc", "clabel", "quiver", "quiver3", "peaks", "sphere", "cylinder", "ellipsoid", "image", "imagesc", "imshow",
    "ind2rgb", "rgb2gray", "im2double", "im2gray", "fsurf", "fmesh", "fcontour", "fplot3", "rotate3d", "hidden",
];

fn is_textlike(v: &Value) -> bool {
    v.is_text() || matches!(v, Value::Str(_))
}

/// Splits trailing name-value pairs off the arguments (from the first text argument).
fn split_props(a: &[Value]) -> R<(&[Value], Vec<(String, Value)>)> {
    let i = a.iter().position(is_textlike).unwrap_or(a.len());
    let (head, tail) = a.split_at(i);
    if tail.len() % 2 != 0 {
        return rt(format!("Invalid parameter/value pair arguments ('{}').", tail.last().map(|v| v.text().unwrap_or_default()).unwrap_or_default()));
    }
    Ok((head, tail.chunks(2).map(|p| Ok((p[0].text()?, p[1].clone()))).collect::<R<_>>()?))
}

/// The current axes before a new plot: whether they keep their contents (`hold on`).
fn held(it: &mut Interpreter) -> bool {
    it.current_figure_index();
    let f = it.current_figure.unwrap();
    it.figures[f].axes_mut().hold
}

fn axes_mut(it: &mut Interpreter) -> &mut graphics::Axes {
    let f = it.current_figure_index();
    let id = it.figures[f].id;
    it.mark_figure_dirty(id);
    it.figures[f].axes_mut()
}

/// Adds 3-D objects, switching fresh axes to the default 3-D view.
fn add_3d(it: &mut Interpreter, series: Vec<Series>, props: &[(String, Value)], nargout: usize) -> Ret {
    let was_held = held(it);
    let ids = it.add_series(series, XAxisKind::Numeric);
    if !was_held {
        axes_mut(it).view = Some((-37.5, 30.0));
    }
    apply_props(it, &ids, props)?;
    handles_out(it, &ids, nargout)
}

/// X, Y and Z of a surface call as full rows×cols matrices (column-major).
fn grid_xyz(a: &[Value]) -> R<(usize, usize, Vec<f64>, Vec<f64>, Vec<f64>, Option<Vec<f64>>)> {
    match a.len() {
        1 | 2 => {
            let z = a[0].to_matrix()?;
            let (m, n) = (z.rows(), z.cols());
            let x = (0..m * n).map(|k| (k / m + 1) as f64).collect();
            let y = (0..m * n).map(|k| (k % m + 1) as f64).collect();
            let c = a.get(1).map(|v| v.to_matrix().map(|c| c.data)).transpose()?;
            Ok((m, n, x, y, z.data, c))
        }
        3 | 4 => {
            let (x, y, z) = (a[0].to_matrix()?, a[1].to_matrix()?, a[2].to_matrix()?);
            let (m, n) = (z.rows(), z.cols());
            let expand = |v: &Matrix, along_cols: bool| -> R<Vec<f64>> {
                if v.numel() == m * n && v.rows() == m {
                    return Ok(v.data.clone());
                }
                let len = if along_cols { n } else { m };
                if v.numel() != len {
                    return rt("Data dimensions must agree.");
                }
                Ok((0..m * n).map(|k| v.data[if along_cols { k / m } else { k % m }]).collect())
            };
            let c = a.get(3).map(|v| v.to_matrix().map(|c| c.data)).transpose()?;
            Ok((m, n, expand(&x, true)?, expand(&y, false)?, z.data, c))
        }
        _ => rt("Not enough input arguments."),
    }
}

fn surface_series(a: &[Value], mesh: bool, flat: bool) -> R<Series> {
    let (m, n, x, y, z, c) = grid_xyz(a)?;
    let mut s = Series::new(SeriesKind::Surface, x, y);
    s.c = c.unwrap_or_else(|| if flat { z.clone() } else { z.clone() });
    s.z = if flat { vec![0.0; m * n] } else { z };
    s.grid = (m, n);
    s.line_style = LineStyle::Solid;
    if mesh {
        s.color = Some([255, 255, 255]);
        s.edge_mapped = true;
    } else {
        s.face_mapped = true;
        s.edge_color = Some([0, 0, 0]);
    }
    Ok(s)
}

// ---- contours -----------------------------------------------------------------------------------

struct Grid<'a> {
    m: usize,
    n: usize,
    x: &'a [f64],
    y: &'a [f64],
    z: &'a [f64],
}

impl Grid<'_> {
    fn at(&self, i: usize, j: usize) -> usize {
        j * self.m + i
    }
}

/// Contour polylines at `level` (marching squares, joined across cells).
fn contour_lines(g: &Grid, level: f64) -> Vec<Vec<(f64, f64)>> {
    use std::collections::HashMap;
    // Edge keys: (i, j, 0) from (i, j) to (i, j+1); (i, j, 1) from (i, j) to (i+1, j).
    let point = |e: (usize, usize, u8)| -> (f64, f64) {
        let a = g.at(e.0, e.1);
        let b = if e.2 == 0 { g.at(e.0, e.1 + 1) } else { g.at(e.0 + 1, e.1) };
        let (za, zb) = (g.z[a], g.z[b]);
        let t = if zb != za { (level - za) / (zb - za) } else { 0.5 };
        (g.x[a] + t * (g.x[b] - g.x[a]), g.y[a] + t * (g.y[b] - g.y[a]))
    };
    let above = |i: usize, j: usize| g.z[g.at(i, j)] >= level;
    let mut segs: Vec<((usize, usize, u8), (usize, usize, u8))> = Vec::new();
    for i in 0..g.m.saturating_sub(1) {
        for j in 0..g.n.saturating_sub(1) {
            let vals = [g.z[g.at(i, j)], g.z[g.at(i, j + 1)], g.z[g.at(i + 1, j + 1)], g.z[g.at(i + 1, j)]];
            if vals.iter().any(|v| !v.is_finite()) {
                continue;
            }
            // Corners in order: (i,j), (i,j+1), (i+1,j+1), (i+1,j); edges between them.
            let edges = [(i, j, 0u8), (i, j + 1, 1u8), (i + 1, j, 0u8), (i, j, 1u8)];
            let corners = [above(i, j), above(i, j + 1), above(i + 1, j + 1), above(i + 1, j)];
            let crossing: Vec<usize> = (0..4).filter(|&k| corners[k] != corners[(k + 1) % 4]).collect();
            match crossing.len() {
                2 => segs.push((edges[crossing[0]], edges[crossing[1]])),
                4 => {
                    // Saddle: the cell centre decides which corners connect.
                    let centre = vals.iter().sum::<f64>() / 4.0 >= level;
                    if centre == corners[0] {
                        segs.push((edges[0], edges[1]));
                        segs.push((edges[2], edges[3]));
                    } else {
                        segs.push((edges[3], edges[0]));
                        segs.push((edges[1], edges[2]));
                    }
                }
                _ => {}
            }
        }
    }
    let mut by_edge: HashMap<(usize, usize, u8), Vec<usize>> = HashMap::new();
    for (k, (a, b)) in segs.iter().enumerate() {
        by_edge.entry(*a).or_default().push(k);
        by_edge.entry(*b).or_default().push(k);
    }
    let mut used = vec![false; segs.len()];
    let mut lines = Vec::new();
    // Open lines start at an edge used once (the grid border); then closed loops.
    let mut starts: Vec<usize> = (0..segs.len()).filter(|&k| by_edge[&segs[k].0].len() == 1 || by_edge[&segs[k].1].len() == 1).collect();
    starts.extend(0..segs.len());
    for start in starts {
        if used[start] {
            continue;
        }
        used[start] = true;
        let (mut a, mut b) = segs[start];
        if by_edge[&b].len() == 1 && by_edge[&a].len() > 1 {
            std::mem::swap(&mut a, &mut b);
        }
        let mut chain = vec![a, b];
        let mut cur = b;
        loop {
            let next = by_edge[&cur].iter().copied().find(|&k| !used[k]);
            let Some(k) = next else { break };
            used[k] = true;
            let (p, q) = segs[k];
            cur = if p == cur { q } else { p };
            chain.push(cur);
        }
        lines.push(chain.into_iter().map(point).collect());
    }
    lines
}

/// Contour levels: given values, `n` levels between the extremes, or nice steps.
fn levels(z: &[f64], spec: Option<&Matrix>, filled: bool) -> Vec<f64> {
    let finite: Vec<f64> = z.iter().copied().filter(|v| v.is_finite()).collect();
    let lo = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !lo.is_finite() || lo == hi {
        return Vec::new();
    }
    match spec {
        Some(m) if m.numel() == 1 && m.data[0].fract() == 0.0 && m.data[0] >= 1.0 => {
            let n = m.data[0] as usize;
            let step = (hi - lo) / (n + 1) as f64;
            let start = if filled { 0 } else { 1 };
            (start..=n).map(|k| lo + k as f64 * step).collect()
        }
        Some(m) => {
            let mut v = m.data.clone();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v.dedup();
            v
        }
        None => {
            let step = graphics::nice_step(hi - lo, 7.0);
            let mut v: Vec<f64> = ((lo / step).floor() as i64..=(hi / step).ceil() as i64).map(|k| k as f64 * step).filter(|&l| l > lo && l < hi).collect();
            if filled {
                v.insert(0, lo);
            }
            v
        }
    }
}

/// The contour matrix: [level count; x y ...] blocks, as `contourc` returns.
fn contour_matrix(g: &Grid, levels: &[f64]) -> Matrix {
    let mut cols: Vec<[f64; 2]> = Vec::new();
    for &l in levels {
        for line in contour_lines(g, l) {
            cols.push([l, line.len() as f64]);
            cols.extend(line.iter().map(|p| [p.0, p.1]));
        }
    }
    let n = cols.len();
    Matrix::new(2, n, cols.iter().flat_map(|c| [c[0], c[1]]).collect())
}

/// Polygons of the band lo <= z < hi within one triangle (Sutherland-Hodgman on z).
fn clip_band(tri: [(f64, f64, f64); 3], lo: f64, hi: f64) -> Vec<(f64, f64)> {
    let mut poly: Vec<(f64, f64, f64)> = tri.to_vec();
    for (bound, keep_above) in [(lo, true), (hi, false)] {
        if !bound.is_finite() {
            continue;
        }
        let inside = |p: &(f64, f64, f64)| if keep_above { p.2 >= bound } else { p.2 <= bound };
        let mut out = Vec::new();
        for k in 0..poly.len() {
            let (p, q) = (poly[k], poly[(k + 1) % poly.len()]);
            let (ip, iq) = (inside(&p), inside(&q));
            if ip {
                out.push(p);
            }
            if ip != iq {
                let t = (bound - p.2) / (q.2 - p.2);
                out.push((p.0 + t * (q.0 - p.0), p.1 + t * (q.1 - p.1), bound));
            }
        }
        poly = out;
        if poly.len() < 3 {
            return Vec::new();
        }
    }
    poly.into_iter().map(|p| (p.0, p.1)).collect()
}

/// Filled bands: NaN-separated polygons and the band value of each point.
fn contour_bands(g: &Grid, levels: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let (mut xs, mut ys, mut cs) = (Vec::new(), Vec::new(), Vec::new());
    let zmin = g.z.iter().copied().filter(|v| v.is_finite()).fold(f64::INFINITY, f64::min);
    let mut bounds: Vec<f64> = vec![f64::NEG_INFINITY];
    bounds.extend(levels.iter().copied().filter(|&l| l > zmin));
    bounds.push(f64::INFINITY);
    for i in 0..g.m.saturating_sub(1) {
        for j in 0..g.n.saturating_sub(1) {
            let p = |i: usize, j: usize| {
                let k = g.at(i, j);
                (g.x[k], g.y[k], g.z[k])
            };
            for tri in [[p(i, j), p(i, j + 1), p(i + 1, j + 1)], [p(i, j), p(i + 1, j + 1), p(i + 1, j)]] {
                if tri.iter().any(|t| !t.2.is_finite()) {
                    continue;
                }
                for b in 0..bounds.len() - 1 {
                    let poly = clip_band(tri, bounds[b], bounds[b + 1]);
                    if poly.is_empty() {
                        continue;
                    }
                    let value = if b == 0 { zmin } else { bounds[b] };
                    for (x, y) in poly {
                        xs.push(x);
                        ys.push(y);
                        cs.push(value);
                    }
                    xs.push(f64::NAN);
                    ys.push(f64::NAN);
                    cs.push(f64::NAN);
                }
            }
        }
    }
    (xs, ys, cs)
}

fn contour(it: &mut Interpreter, a: &[Value], name: &str, nargout: usize) -> Ret {
    let (head, mut props) = split_props(a)?;
    // contour(Z), contour(Z, n|v), contour(X, Y, Z), contour(X, Y, Z, n|v); a line spec may follow.
    let (grid_args, spec) = match head.len() {
        1 | 3 => (head, None),
        2 | 4 => (&head[..head.len() - 1], Some(head[head.len() - 1].to_matrix()?)),
        _ => return rt("Not enough input arguments."),
    };
    let (m, n, x, y, z, _) = grid_xyz(grid_args)?;
    let filled = name == "contourf";
    let lv = levels(&z, spec.as_ref(), filled);
    let g = Grid { m, n, x: &x, y: &y, z: &z };
    let mut s = Series::new(SeriesKind::Contour, Vec::new(), Vec::new());
    s.line_style = LineStyle::Solid;
    for &l in &lv {
        for line in contour_lines(&g, l) {
            for (px, py) in line {
                s.x.push(px);
                s.y.push(py);
                s.c.push(l);
                if name == "contour3" {
                    s.z.push(l);
                }
            }
            s.x.push(f64::NAN);
            s.y.push(f64::NAN);
            s.c.push(f64::NAN);
            if name == "contour3" {
                s.z.push(f64::NAN);
            }
        }
    }
    if filled {
        let (u, v, w) = contour_bands(&g, &lv);
        (s.u, s.v, s.w) = (u, v, w);
        s.edge_color = Some([0, 0, 0]);
    } else {
        s.edge_mapped = true;
    }
    // The line color: 'LineColor' or a color in the line spec.
    if let Some(k) = props.iter().position(|(n, _)| n.eq_ignore_ascii_case("LineColor")) {
        let (_, v) = props.remove(k);
        if is_textlike(&v) && v.text()? == "none" {
            s.line_style = LineStyle::None;
        } else if is_textlike(&v) && v.text()? == "flat" {
            s.edge_mapped = true;
        } else {
            s.edge_mapped = false;
            s.edge_color = crate::gfx::color_arg(&v)?.map(|c| c.0);
        }
    }
    // The contour matrix output needs the levels and lines, as contourc gives them.
    let matrix = (nargout > 0).then(|| contour_matrix(&g, &lv));
    let ids = if name == "contour3" {
        let was_held = held(it);
        let ids = it.add_series(vec![s], XAxisKind::Numeric);
        if !was_held {
            axes_mut(it).view = Some((-37.5, 30.0));
        }
        ids
    } else {
        it.add_series(vec![s], XAxisKind::Numeric)
    };
    let ax = axes_mut(it);
    if ax.series.len() == 1 && !ax.hold {
        let (x0, x1, y0, y1) = (minmax(&x).0, minmax(&x).1, minmax(&y).0, minmax(&y).1);
        ax.xlim = Some((x0, x1));
        ax.ylim = Some((y0, y1));
    }
    apply_props(it, &ids, &props)?;
    match matrix {
        None => none(),
        Some(mx) => {
            let mut out = vec![Value::Num(mx)];
            if nargout > 1 {
                out.push(crate::gfx::handles(it, &ids));
            }
            Ok(Some(out))
        }
    }
}

fn minmax(v: &[f64]) -> (f64, f64) {
    let lo = v.iter().copied().filter(|x| x.is_finite()).fold(f64::INFINITY, f64::min);
    let hi = v.iter().copied().filter(|x| x.is_finite()).fold(f64::NEG_INFINITY, f64::max);
    (lo, hi)
}

// ---- images ---------------------------------------------------------------------------------------

/// Image samples: values as stored (for colormapped images) or RGB in 0..1, with (rows, cols, channels).
fn image_data(v: &Value) -> R<(usize, usize, usize, Vec<f64>, Kind)> {
    let m = v.to_matrix()?;
    let dims = m.dims.clone();
    let (rows, cols) = (dims[0], dims.get(1).copied().unwrap_or(1));
    let ch = dims.get(2).copied().unwrap_or(1);
    if ch != 1 && ch != 3 {
        return rt("Image data must be either MxN (indexed or grayscale) or MxNx3 (truecolor).");
    }
    let data = if ch == 3 {
        let scale = match m.kind {
            Kind::Int(IntClass::U8) => 255.0,
            Kind::Int(IntClass::U16) => 65535.0,
            _ => 1.0,
        };
        m.data.iter().map(|&v| (v / scale).clamp(0.0, 1.0)).collect()
    } else {
        m.data.clone()
    };
    Ok((rows, cols, ch, data, m.kind))
}

fn image(it: &mut Interpreter, a: &[Value], name: &str, nargout: usize) -> Ret {
    let (head, props) = split_props(a)?;
    let mut head: Vec<Value> = head.to_vec();
    // imshow(filename), imshow(I, []), imshow(I, [lo hi]), imshow(X, map).
    let mut clims: Option<(f64, f64)> = None;
    let mut map: Option<Vec<[f64; 3]>> = None;
    if name == "imshow" {
        if let Some(Value::Num(m)) = a.first().filter(|v| v.is_text()) {
            let _ = m;
            let file = a[0].text()?;
            let r = crate::builtins_media::call(it, "imread", &[Value::chars(&file)], 2)?.unwrap_or_default();
            head = vec![r[0].clone()];
            if let Some(Value::Num(cm)) = r.get(1).filter(|c| c.numel() > 0) {
                head.push(Value::Num(cm.clone()));
            }
        }
        if head.len() >= 2 {
            let m = head[1].to_matrix()?;
            if m.numel() == 0 {
                let d = head[0].to_matrix()?;
                clims = Some(minmax(&d.data));
            } else if m.cols() == 3 && m.rows() > 1 {
                map = Some((0..m.rows()).map(|r| [m.get(r, 0), m.get(r, 1), m.get(r, 2)]).collect());
            } else if m.numel() == 2 {
                clims = Some((m.data[0], m.data[1]));
            }
            head.truncate(1);
        }
    }
    if name == "imagesc" && head.len() == 2 || (name == "imagesc" && head.len() == 4) {
        let l = head.pop().unwrap().to_matrix()?;
        if l.numel() == 2 {
            clims = Some((l.data[0], l.data[1]));
        }
    }
    let (xr, yr, cdata) = match head.len() {
        1 => (None, None, &head[0]),
        3 => (Some(head[0].to_matrix()?), Some(head[1].to_matrix()?), &head[2]),
        _ => return rt("Invalid number of input arguments."),
    };
    let (rows, cols, ch, data, kind) = image_data(cdata)?;
    let range = |r: Option<Matrix>, n: usize| -> (f64, f64) {
        match r {
            Some(m) if m.numel() >= 1 => (m.data[0], *m.data.last().unwrap()),
            _ => (1.0, n as f64),
        }
    };
    let (x0, x1) = range(xr, cols);
    let (y0, y1) = range(yr, rows);
    let mut s = Series::new(SeriesKind::Image, vec![x0, x1], vec![y0, y1]);
    s.grid = (rows, cols);
    s.channels = ch;
    s.c = data;
    // image() indexes the colormap directly (1-based for doubles, 0-based for integers).
    s.direct = name == "image" || (name == "imshow" && map.is_some());
    if s.direct && ch == 1 && matches!(kind, Kind::Int(_) | Kind::Logical) {
        s.c.iter_mut().for_each(|v| *v += 1.0);
    }
    if name == "imshow" && ch == 1 && map.is_none() && clims.is_none() {
        clims = Some(match kind {
            Kind::Int(IntClass::U8) => (0.0, 255.0),
            Kind::Int(IntClass::U16) => (0.0, 65535.0),
            _ => (0.0, 1.0),
        });
    }
    let was_held = held(it);
    let ids = it.add_series(vec![s], XAxisKind::Numeric);
    let ax = axes_mut(it);
    if !was_held {
        let (dx, dy) = graphics::image_pixel(&ax.series[ax.series.len() - 1]);
        ax.xlim = Some((x0.min(x1) - dx / 2.0, x0.max(x1) + dx / 2.0));
        ax.ylim = Some((y0.min(y1) - dy / 2.0, y0.max(y1) + dy / 2.0));
        ax.y_reverse = true;
        if name == "imshow" {
            ax.data_aspect = Some([1.0, 1.0, 1.0]);
            ax.visible_axes = Some(false);
        }
    }
    if let Some(l) = clims {
        ax.clim = Some(l);
    }
    if name == "imshow" && ch == 1 {
        ax.colormap = Some(map.unwrap_or_else(|| graphics::colormap("gray", 256).unwrap()));
    }
    apply_props(it, &ids, &props)?;
    handles_out(it, &ids, nargout)
}

// ---- colormaps, limits, view ------------------------------------------------------------------------

fn map_value(map: &[[f64; 3]]) -> Value {
    let n = map.len();
    let mut data = vec![0.0; n * 3];
    for (i, c) in map.iter().enumerate() {
        for k in 0..3 {
            data[k * n + i] = c[k];
        }
    }
    Value::Num(Matrix::new(n, 3, data))
}

fn map_arg(v: &Value) -> R<Vec<[f64; 3]>> {
    if is_textlike(v) {
        let name = v.text()?;
        let name = if name == "default" { "parula".to_string() } else { name };
        return graphics::colormap(&name, 256).ok_or_else(|| LangError::new(format!("Unrecognized colormap name '{name}'."), 0));
    }
    let m = v.to_matrix()?;
    if m.cols() != 3 || m.data.iter().any(|c| !(0.0..=1.0).contains(c)) {
        return rt("Colormap must have 3 columns: [R,G,B].");
    }
    Ok((0..m.rows()).map(|r| [m.get(r, 0), m.get(r, 1), m.get(r, 2)]).collect())
}

/// The colormap length the colormap functions use by default (the current figure's).
fn default_length(it: &Interpreter) -> usize {
    it.current_figure.and_then(|f| it.figures[f].colormap.as_ref().map(|m| m.len())).unwrap_or(256)
}

fn colormap_fn(it: &mut Interpreter, a: &[Value]) -> Ret {
    // colormap(target, map): an axes target sets the axes map, a figure the figure map.
    let (target_axes, rest) = match a.first().and_then(crate::gfx::ids_of) {
        Some(ids) => match ids.first().and_then(|&id| crate::gfx::resolve(it, id)) {
            Some(crate::gfx::Target::Axes(f, ax)) => {
                it.current_figure = Some(f);
                it.figures[f].current_axes = ax;
                (true, &a[1..])
            }
            Some(crate::gfx::Target::Figure(f)) => {
                it.current_figure = Some(f);
                (false, &a[1..])
            }
            _ => return rt("Invalid or deleted object."),
        },
        None => (false, a),
    };
    let f = it.current_figure_index();
    let Some(v) = rest.first() else {
        let fig = &it.figures[f];
        let m = fig.axes[fig.current_axes].colormap.clone().or_else(|| fig.colormap.clone()).unwrap_or_else(|| graphics::colormap("parula", 256).unwrap());
        return one(map_value(&m));
    };
    let map = map_arg(v)?;
    let id = it.figures[f].id;
    it.mark_figure_dirty(id);
    if target_axes {
        it.figures[f].axes_mut().colormap = Some(map.clone());
    } else {
        it.figures[f].colormap = Some(map.clone());
        for ax in it.figures[f].axes.iter_mut() {
            ax.colormap = None;
        }
    }
    none()
}

fn view_fn(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let ax = axes_mut(it);
    let new = match a.len() {
        0 => None,
        1 => {
            let m = a[0].to_matrix()?;
            match (m.numel(), m.data.first()) {
                (1, Some(&2.0)) => Some((0.0, 90.0)),
                (1, Some(&3.0)) => Some((-37.5, 30.0)),
                (2, _) => Some((m.data[0], m.data[1])),
                _ => return rt("Invalid view specification."),
            }
        }
        _ => Some((a[0].scalar()?, a[1].scalar()?)),
    };
    if let Some(v) = new {
        ax.view = Some(v);
        return none();
    }
    let (az, el) = ax.view.unwrap_or((0.0, 90.0));
    if nargout >= 2 {
        return Ok(Some(vec![Value::num(az), Value::num(el)]));
    }
    one(Value::Num(Matrix::row(vec![az, el])))
}

// ---- shapes ---------------------------------------------------------------------------------------------

fn peaks_at(x: f64, y: f64) -> f64 {
    3.0 * (1.0 - x).powi(2) * (-x * x - (y + 1.0).powi(2)).exp() - 10.0 * (x / 5.0 - x.powi(3) - y.powi(5)) * (-x * x - y * y).exp() - (-(x + 1.0).powi(2) - y * y).exp() / 3.0
}

/// Returns (x, y, z) matrices or plots them as a surface without outputs.
fn shape_out(it: &mut Interpreter, m: usize, n: usize, x: Vec<f64>, y: Vec<f64>, z: Vec<f64>, nargout: usize) -> Ret {
    let mk = |d: Vec<f64>| Value::Num(Matrix::new(m, n, d));
    if nargout == 0 {
        let s = surface_series(&[mk(x), mk(y), mk(z)], false, false)?;
        return add_3d(it, vec![s], &[], 0);
    }
    Ok(Some(vec![mk(x), mk(y), mk(z)]))
}

fn sphere_xyz(n: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let m = n + 1;
    let (mut x, mut y, mut z) = (vec![0.0; m * m], vec![0.0; m * m], vec![0.0; m * m]);
    for j in 0..m {
        let theta = (2.0 * j as f64 - n as f64) / n as f64 * std::f64::consts::PI;
        for i in 0..m {
            let phi = (2.0 * i as f64 - n as f64) / n as f64 * std::f64::consts::FRAC_PI_2;
            let k = j * m + i;
            x[k] = phi.cos() * theta.cos();
            y[k] = phi.cos() * theta.sin();
            z[k] = phi.sin();
        }
    }
    // Exact zeros where cos/sin of multiples of pi/2 leave rounding noise.
    for v in x.iter_mut().chain(y.iter_mut()).chain(z.iter_mut()) {
        if v.abs() < 1e-15 {
            *v = 0.0;
        }
    }
    (x, y, z)
}

// ---- function plots ---------------------------------------------------------------------------------------

fn call_fn(it: &mut Interpreter, f: &Value, args: Vec<Value>) -> R<Matrix> {
    let Value::Func(fv) = f else { return rt("Expected a function handle.") };
    let r = it.call_value(fv, args, 1)?;
    r.into_iter().next().ok_or_else(|| LangError::new("Function returned no value.", 0))?.to_matrix()
}

fn fn_grid(it: &mut Interpreter, f: &Value, lims: Option<&Value>, n: usize) -> R<(usize, usize, Vec<f64>, Vec<f64>, Vec<f64>)> {
    let l = lims.map(|v| v.to_matrix()).transpose()?.map(|m| m.data).unwrap_or_else(|| vec![-5.0, 5.0]);
    let (x0, x1, y0, y1) = if l.len() >= 4 { (l[0], l[1], l[2], l[3]) } else { (l[0], l[1], l[0], l[1]) };
    let xs: Vec<f64> = (0..n).map(|k| x0 + (x1 - x0) * k as f64 / (n - 1) as f64).collect();
    let ys: Vec<f64> = (0..n).map(|k| y0 + (y1 - y0) * k as f64 / (n - 1) as f64).collect();
    let x: Vec<f64> = (0..n * n).map(|k| xs[k / n]).collect();
    let y: Vec<f64> = (0..n * n).map(|k| ys[k % n]).collect();
    let z = call_fn(it, f, vec![Value::Num(Matrix::new(n, n, x.clone())), Value::Num(Matrix::new(n, n, y.clone()))])?;
    let z = if z.numel() == 1 { vec![z.data[0]; n * n] } else { z.data };
    Ok((n, n, x, y, z))
}

pub fn call(it: &mut Interpreter, name: &str, args: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    let a = if COLORMAPS.contains(&name) || matches!(name, "colormap" | "peaks" | "sphere" | "cylinder" | "ellipsoid" | "contourc" | "ind2rgb" | "rgb2gray" | "im2double" | "im2gray") { args } else { crate::builtins_plot::select_axes(it, args)? };
    if COLORMAPS.contains(&name) {
        let n = match a.first() {
            Some(v) => v.scalar()? as usize,
            None => default_length(it),
        };
        return one(map_value(&graphics::colormap(name, n).unwrap()));
    }
    match name {
        "colormap" => colormap_fn(it, a),
        "clim" | "caxis" => {
            let ax = axes_mut(it);
            match a.first() {
                None => {
                    let (lo, hi) = ax.color_limits();
                    one(Value::Num(Matrix::row(vec![lo, hi])))
                }
                Some(v) if is_textlike(v) => {
                    match v.text()?.as_str() {
                        "auto" => ax.clim = None,
                        "manual" => ax.clim = Some(ax.color_limits()),
                        "mode" => return one(Value::chars(if ax.clim.is_some() { "manual" } else { "auto" })),
                        other => return rt(format!("Invalid option '{other}'.")),
                    }
                    none()
                }
                Some(v) => {
                    let m = v.to_matrix()?;
                    if m.numel() != 2 || !(m.data[0] < m.data[1]) {
                        return rt("Value must be a 1x2 vector of numeric type in which the second element is larger than the first and may be Inf.");
                    }
                    ax.clim = Some((m.data[0], m.data[1]));
                    none()
                }
            }
        }
        "view" => view_fn(it, a, nargout),
        "shading" => {
            let mode = arg(a, 0, name)?.text()?.to_lowercase();
            let ax = axes_mut(it);
            for s in ax.series.iter_mut().filter(|s| s.kind == SeriesKind::Surface) {
                match mode.as_str() {
                    "flat" | "interp" => {
                        s.edge_color = None;
                        s.edge_mapped = false;
                        s.face_mapped = true;
                        s.face_none = false;
                    }
                    "faceted" => {
                        s.edge_color = Some([0, 0, 0]);
                        s.edge_mapped = false;
                        s.face_mapped = true;
                        s.face_none = false;
                    }
                    other => return rt(format!("Unknown shading type '{other}'.")),
                }
            }
            none()
        }
        "plot3" => {
            // plot3(x, y, z, spec, x2, y2, z2, spec2, ..., Name, Value).
            let mut series = Vec::new();
            let mut i = 0;
            while i + 2 < a.len() && !is_textlike(&a[i]) {
                let (x, y, z) = (a[i].to_matrix()?, a[i + 1].to_matrix()?, a[i + 2].to_matrix()?);
                i += 3;
                let spec = a.get(i).filter(|v| v.is_text()).and_then(|v| graphics::parse_line_spec(&v.text().ok()?));
                if spec.is_some() {
                    i += 1;
                }
                // Matrix arguments plot one line per column.
                let cols = if x.rows() == 1 || x.cols() == 1 { 1 } else { x.cols() };
                let len = x.numel() / cols;
                for k in 0..cols {
                    let take = |m: &Matrix| if m.numel() == x.numel() { m.data[k * len..(k + 1) * len].to_vec() } else { m.data.clone() };
                    let mut s = Series::new(SeriesKind::Line, take(&x), take(&y));
                    s.z = take(&z);
                    if let Some((c, st, mk)) = spec {
                        s.color = c.or(s.color);
                        if let Some(st) = st {
                            s.line_style = st;
                        } else if mk.is_some() {
                            s.line_style = LineStyle::None;
                        }
                        s.marker = mk;
                    }
                    series.push(s);
                }
            }
            let (_, props) = split_props(&a[i..])?;
            add_3d(it, series, &props, nargout)
        }
        "scatter3" | "stem3" => {
            let (head, props) = split_props(a)?;
            if head.len() < 3 {
                return rt("Not enough input arguments.");
            }
            let (x, y, z) = (head[0].to_matrix()?, head[1].to_matrix()?, head[2].to_matrix()?);
            let mut s = Series::new(if name == "stem3" { SeriesKind::Stem } else { SeriesKind::Scatter }, x.data, y.data);
            s.z = z.data;
            if name == "scatter3" {
                if let Some(sz) = head.get(3).filter(|v| v.numel() == 1) {
                    s.marker_size = (sz.scalar()?.sqrt() as f32).max(2.0);
                }
            } else {
                s.marker = Some(Marker::Circle);
            }
            let props: Vec<(String, Value)> = props.into_iter().filter(|(n, _)| n != "filled").collect();
            add_3d(it, vec![s], &props, nargout)
        }
        "surf" | "mesh" | "surfc" | "meshc" | "meshz" | "surface" | "pcolor" => {
            let (head, props) = split_props(a)?;
            let mesh = name.starts_with("mesh");
            let s = surface_series(head, mesh, name == "pcolor")?;
            match name {
                "surface" => {
                    let ids = it.add_decoration(vec![s]);
                    apply_props(it, &ids, &props)?;
                    handles_out(it, &ids, nargout)
                }
                "pcolor" => {
                    let (x0, x1) = minmax(&s.x);
                    let (y0, y1) = minmax(&s.y);
                    let ids = it.add_series(vec![s], XAxisKind::Numeric);
                    let ax = axes_mut(it);
                    ax.xlim = Some((x0, x1));
                    ax.ylim = Some((y0, y1));
                    apply_props(it, &ids, &props)?;
                    handles_out(it, &ids, nargout)
                }
                _ => {
                    let mut series = vec![s.clone()];
                    if name == "surfc" || name == "meshc" {
                        // Contours of z drawn on the floor below the surface.
                        let (zmin, _) = minmax(&s.z);
                        let g = Grid { m: s.grid.0, n: s.grid.1, x: &s.x, y: &s.y, z: &s.z };
                        let mut c = Series::new(SeriesKind::Contour, Vec::new(), Vec::new());
                        c.edge_mapped = true;
                        for l in levels(&s.z, None, false) {
                            for line in contour_lines(&g, l) {
                                for (px, py) in line {
                                    c.x.push(px);
                                    c.y.push(py);
                                    c.z.push(zmin);
                                    c.c.push(l);
                                }
                                c.x.push(f64::NAN);
                                c.y.push(f64::NAN);
                                c.z.push(f64::NAN);
                                c.c.push(f64::NAN);
                            }
                        }
                        series.push(c);
                    }
                    add_3d(it, series, &props, nargout)
                }
            }
        }
        "contour" | "contourf" | "contour3" => contour(it, a, name, nargout),
        "contourc" => {
            let (grid_args, spec) = match a.len() {
                1 | 3 => (a, None),
                2 | 4 => (&a[..a.len() - 1], Some(a[a.len() - 1].to_matrix()?)),
                _ => return rt("Not enough input arguments."),
            };
            let (m, n, x, y, z, _) = grid_xyz(grid_args)?;
            let lv = levels(&z, spec.as_ref(), false);
            let g = Grid { m, n, x: &x, y: &y, z: &z };
            one(Value::Num(contour_matrix(&g, &lv)))
        }
        "clabel" | "rotate3d" | "hidden" => none(),
        "quiver" | "quiver3" => {
            let (head, props) = split_props(a)?;
            let three = name == "quiver3";
            let nums: Vec<Matrix> = head.iter().map(|v| v.to_matrix()).collect::<R<_>>()?;
            let (x, y, z, u, v, w, scale) = match (three, nums.len()) {
                (false, 2) | (false, 3) => {
                    let (m, n) = (nums[0].rows(), nums[0].cols());
                    let x = (0..m * n).map(|k| (k / m + 1) as f64).collect();
                    let y = (0..m * n).map(|k| (k % m + 1) as f64).collect();
                    (x, y, Vec::new(), nums[0].data.clone(), nums[1].data.clone(), Vec::new(), nums.get(2).map(|s| s.data[0]))
                }
                (false, 4) | (false, 5) => (nums[0].data.clone(), nums[1].data.clone(), Vec::new(), nums[2].data.clone(), nums[3].data.clone(), Vec::new(), nums.get(4).map(|s| s.data[0])),
                (true, 6) | (true, 7) => (nums[0].data.clone(), nums[1].data.clone(), nums[2].data.clone(), nums[3].data.clone(), nums[4].data.clone(), nums[5].data.clone(), nums.get(6).map(|s| s.data[0])),
                _ => return rt("Invalid number of input arguments."),
            };
            let n = u.len();
            let fit = |v: Vec<f64>| if v.len() == n { v } else { (0..n).map(|k| v[k % v.len().max(1)]).collect() };
            let (x, y) = (fit(x), fit(y));
            // Automatic scaling: the longest arrow spans about 0.9 of the grid spacing.
            let factor = scale.unwrap_or(1.0);
            let k = if factor == 0.0 {
                1.0
            } else {
                let side = (n as f64).sqrt().max(1.0);
                let (x0, x1) = minmax(&x);
                let (y0, y1) = minmax(&y);
                let spacing = (((x1 - x0) / side).powi(2) + ((y1 - y0) / side).powi(2)).sqrt();
                let longest = (0..n).map(|i| (u[i].powi(2) + v[i].powi(2) + w.get(i).map(|w| w * w).unwrap_or(0.0)).sqrt()).fold(0.0, f64::max);
                if longest > 0.0 && spacing > 0.0 { 0.9 * factor * spacing / longest } else { factor }
            };
            let mut s = Series::new(SeriesKind::Quiver, x, y);
            s.u = u.iter().map(|v| v * k).collect();
            s.v = v.iter().map(|v| v * k).collect();
            if three {
                s.z = fit(z);
                s.w = w.iter().map(|v| v * k).collect();
                return add_3d(it, vec![s], &props, nargout);
            }
            let ids = it.add_series(vec![s], XAxisKind::Numeric);
            apply_props(it, &ids, &props)?;
            handles_out(it, &ids, nargout)
        }
        "peaks" => {
            let (m, n, x, y) = match a.len() {
                0 | 1 => {
                    let n = a.first().map(|v| v.to_matrix()).transpose()?;
                    let xs: Vec<f64> = match n {
                        Some(v) if v.numel() > 1 => v.data,
                        Some(v) => {
                            let n = v.data[0] as usize;
                            (0..n).map(|k| -3.0 + 6.0 * k as f64 / (n.max(2) - 1) as f64).collect()
                        }
                        None => (0..49).map(|k| -3.0 + 6.0 * k as f64 / 48.0).collect(),
                    };
                    let n = xs.len();
                    let x: Vec<f64> = (0..n * n).map(|k| xs[k / n]).collect();
                    let y: Vec<f64> = (0..n * n).map(|k| xs[k % n]).collect();
                    (n, n, x, y)
                }
                _ => {
                    let (x, y) = (a[0].to_matrix()?, a[1].to_matrix()?);
                    (x.rows(), x.cols(), x.data, y.data)
                }
            };
            let z: Vec<f64> = x.iter().zip(&y).map(|(&x, &y)| peaks_at(x, y)).collect();
            match nargout {
                0 => shape_out(it, m, n, x, y, z, 0),
                1 => one(Value::Num(Matrix::new(m, n, z))),
                _ => Ok(Some(vec![Value::Num(Matrix::new(m, n, x)), Value::Num(Matrix::new(m, n, y)), Value::Num(Matrix::new(m, n, z))])),
            }
        }
        "sphere" => {
            let n = a.first().map(|v| v.scalar()).transpose()?.unwrap_or(20.0) as usize;
            let (x, y, z) = sphere_xyz(n);
            shape_out(it, n + 1, n + 1, x, y, z, nargout)
        }
        "ellipsoid" => {
            let v: Vec<f64> = a.iter().take(6).map(|v| v.scalar()).collect::<R<_>>()?;
            if v.len() < 6 {
                return rt("Not enough input arguments.");
            }
            let n = a.get(6).map(|v| v.scalar()).transpose()?.unwrap_or(20.0) as usize;
            let (x, y, z) = sphere_xyz(n);
            let x = x.iter().map(|p| v[0] + v[3] * p).collect();
            let y = y.iter().map(|p| v[1] + v[4] * p).collect();
            let z = z.iter().map(|p| v[2] + v[5] * p).collect();
            shape_out(it, n + 1, n + 1, x, y, z, nargout)
        }
        "cylinder" => {
            let r = a.first().map(|v| v.to_matrix()).transpose()?.map(|m| m.data).unwrap_or_else(|| vec![1.0, 1.0]);
            let r = if r.len() == 1 { vec![r[0], r[0]] } else { r };
            let n = a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(20.0) as usize;
            let m = r.len();
            let (mut x, mut y, mut z) = (vec![0.0; m * (n + 1)], vec![0.0; m * (n + 1)], vec![0.0; m * (n + 1)]);
            for j in 0..=n {
                let t = 2.0 * std::f64::consts::PI * j as f64 / n as f64;
                let (c, s) = (t.cos(), t.sin());
                for i in 0..m {
                    let k = j * m + i;
                    x[k] = r[i] * if c.abs() < 1e-15 { 0.0 } else { c };
                    y[k] = r[i] * if s.abs() < 1e-15 { 0.0 } else { s };
                    z[k] = i as f64 / (m - 1) as f64;
                }
            }
            shape_out(it, m, n + 1, x, y, z, nargout)
        }
        "image" | "imagesc" | "imshow" => image(it, a, name, nargout),
        "ind2rgb" => {
            let x = arg(a, 0, name)?.to_matrix()?;
            let map = map_arg(arg(a, 1, name)?)?;
            let offset = if matches!(x.kind, Kind::Int(_) | Kind::Logical) { 0 } else { 1 };
            let n = x.numel();
            let mut data = vec![0.0; n * 3];
            for (i, &v) in x.data.iter().enumerate() {
                let idx = ((v as i64 - offset).clamp(0, map.len() as i64 - 1)) as usize;
                for k in 0..3 {
                    data[k * n + i] = map[idx][k];
                }
            }
            one(Value::Num(Matrix::from_dims(vec![x.rows(), x.cols(), 3], data)))
        }
        "rgb2gray" | "im2gray" => {
            let m = arg(a, 0, name)?.to_matrix()?;
            // A colormap (n×3) converts to gray levels per row; MxNx3 images per pixel.
            let three_d = m.dims.len() == 3 && m.dims[2] == 3;
            if !three_d {
                if name == "im2gray" || !(m.cols() == 3 && m.dims.len() == 2) {
                    return one(Value::Num(m));
                }
            }
            let (n, rows, cols) = if three_d { (m.dims[0] * m.dims[1], m.dims[0], m.dims[1]) } else { (m.rows(), m.rows(), 1) };
            let w = [0.298936021293775, 0.587043074451121, 0.114020904255103];
            let mut out: Vec<f64> = (0..n).map(|i| (0..3).map(|k| w[k] * m.data[k * n + i]).sum()).collect();
            if matches!(m.kind, Kind::Int(_)) {
                out.iter_mut().for_each(|v| *v = v.round());
            }
            let res = if three_d { Matrix::new(rows, cols, out) } else { Matrix::new(rows, 3, out.repeat(3)) };
            one(Value::Num(res.cast_to(m.kind)))
        }
        "im2double" => {
            let m = arg(a, 0, name)?.to_matrix()?;
            let indexed = a.get(1).map(|v| v.text().unwrap_or_default() == "indexed").unwrap_or(false);
            let data: Vec<f64> = match m.kind {
                Kind::Int(IntClass::U8) if indexed => m.data.iter().map(|v| v + 1.0).collect(),
                Kind::Int(IntClass::U16) if indexed => m.data.iter().map(|v| v + 1.0).collect(),
                Kind::Int(IntClass::U8) => m.data.iter().map(|v| v / 255.0).collect(),
                Kind::Int(IntClass::U16) => m.data.iter().map(|v| v / 65535.0).collect(),
                Kind::Int(IntClass::I16) => m.data.iter().map(|v| (v + 32768.0) / 65535.0).collect(),
                _ => m.data.clone(),
            };
            one(Value::Num(Matrix::from_dims(m.dims.clone(), data)))
        }
        "fsurf" | "fmesh" => {
            let f = arg(a, 0, name)?.clone();
            let (m, n, x, y, z) = fn_grid(it, &f, a.get(1).filter(|v| !is_textlike(v)), 35)?;
            let mk = |d: Vec<f64>| Value::Num(Matrix::new(m, n, d));
            let s = surface_series(&[mk(x), mk(y), mk(z)], name == "fmesh", false)?;
            add_3d(it, vec![s], &[], nargout)
        }
        "fcontour" => {
            let f = arg(a, 0, name)?.clone();
            let (m, n, x, y, z) = fn_grid(it, &f, a.get(1).filter(|v| !is_textlike(v)), 71)?;
            let mk = |d: Vec<f64>| Value::Num(Matrix::new(m, n, d));
            contour(it, &[mk(x), mk(y), mk(z)], "contour", nargout)
        }
        "fplot3" => {
            let (fx, fy, fz) = (arg(a, 0, name)?.clone(), arg(a, 1, name)?.clone(), arg(a, 2, name)?.clone());
            let l = a.get(3).filter(|v| !is_textlike(v)).map(|v| v.to_matrix()).transpose()?.map(|m| m.data).unwrap_or_else(|| vec![-5.0, 5.0]);
            let t: Vec<f64> = (0..300).map(|k| l[0] + (l[1] - l[0]) * k as f64 / 299.0).collect();
            let tv = Value::Num(Matrix::row(t.clone()));
            let x = call_fn(it, &fx, vec![tv.clone()])?.data;
            let y = call_fn(it, &fy, vec![tv.clone()])?.data;
            let z = call_fn(it, &fz, vec![tv])?.data;
            let fit = |v: Vec<f64>| if v.len() == 1 { vec![v[0]; t.len()] } else { v };
            let mut s = Series::new(SeriesKind::Line, fit(x), fit(y));
            s.z = fit(z);
            add_3d(it, vec![s], &[], nargout)
        }
        _ => Ok(None),
    }
}
