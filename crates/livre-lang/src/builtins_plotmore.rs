//! More graphics: tiled layouts (`tiledlayout`, `nexttile`), annotations, `rectangle`,
//! transparency (`alpha`), lights and the camera (`light`, `camlight`, `camorbit`, ...), color
//! orders, and charts built from the existing primitives (`pie`, `bar3`, `hist`, `pareto`,
//! `feather`, `waterfall`, `ribbon`, `comet`, `fill3`), plus `makehgtform`, `viewmtx`,
//! `brighten` and `texlabel`.

use crate::builtins::{arg, none, one, Ret};
use crate::builtins_plot::{apply_props, handles_out};
use crate::error::{rt, LangError};
use crate::gfx::{self, Target};
use crate::graphics::{self, Axes, FigObjKind, FigObject, LineStyle, Marker, Series, SeriesKind, XAxisKind};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "tiledlayout", "nexttile", "tilenum", "tilerowcol", "annotation", "rectangle", "alpha", "alim", "alphamap", "light", "camlight", "lightangle", "lighting",
    "material", "camorbit", "campos", "camtarget", "camup", "camva", "camproj", "camzoom", "camroll", "camdolly", "campan", "camlookat", "colororder",
    "orderedcolors", "fill3", "pie", "bar3", "bar3h", "hist", "pareto", "feather", "waterfall", "ribbon", "comet", "comet3", "newplot", "shg", "refresh", "gco",
    "gcbo", "gcbf", "brighten", "makehgtform", "viewmtx", "pbaspect", "texlabel", "rgbplot",
];

fn is_textlike(v: &Value) -> bool {
    v.is_text() || matches!(v, Value::Str(s) if s.data.len() == 1)
}

/// Positional arguments up to the first text, then name-value pairs.
fn split_props(a: &[Value]) -> R<(&[Value], Vec<(String, Value)>)> {
    let i = a.iter().position(is_textlike).unwrap_or(a.len());
    let (head, tail) = a.split_at(i);
    if tail.len() % 2 != 0 {
        return rt(format!("Invalid parameter/value pair arguments ('{}').", tail.last().map(|v| v.text().unwrap_or_default()).unwrap_or_default()));
    }
    Ok((head, tail.chunks(2).map(|p| Ok((p[0].text()?, p[1].clone()))).collect::<R<_>>()?))
}

fn take_prop(props: &mut Vec<(String, Value)>, name: &str) -> Option<Value> {
    let i = props.iter().rposition(|(k, _)| k.eq_ignore_ascii_case(name))?;
    let v = props.remove(i).1;
    props.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
    Some(v)
}

fn mat(v: &Value) -> R<Matrix> {
    v.to_matrix()
}

fn row(v: &[f64]) -> Value {
    Value::Num(Matrix::row(v.to_vec()))
}

fn rgb_matrix(map: &[[f64; 3]]) -> Value {
    let n = map.len();
    let mut data = vec![0.0; n * 3];
    for (i, c) in map.iter().enumerate() {
        for k in 0..3 {
            data[k * n + i] = c[k];
        }
    }
    Value::Num(Matrix::new(n, 3, data))
}

fn map_rows(m: &Matrix) -> R<Vec<[f64; 3]>> {
    if m.cols() != 3 {
        return rt("Color data must be an m-by-3 matrix of RGB triplets.");
    }
    let n = m.rows();
    Ok((0..n).map(|i| [m.data[i], m.data[n + i], m.data[2 * n + i]]).collect())
}

fn to_u8(c: [f64; 3]) -> [u8; 3] {
    [(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8]
}

fn current_axes(it: &mut Interpreter) -> &mut Axes {
    it.current_figure().axes_mut()
}

/// `get(gca, name)` / `set(gca, name, v)`.
fn axes_get(it: &mut Interpreter, name: &str) -> R<Value> {
    let h = it.current_axes_handle();
    gfx::call(it, "get", &[h, Value::chars(name)], 1)?.and_then(|mut v| v.pop()).ok_or_else(|| LangError::new("No value.", 0))
}

fn axes_set(it: &mut Interpreter, name: &str, v: Value) -> R<()> {
    let h = it.current_axes_handle();
    gfx::call(it, "set", &[h, Value::chars(name), v], 0)?;
    Ok(())
}

/// Leading figure/axes handle: makes it current and returns the rest.
fn leading_handle<'a>(it: &mut Interpreter, a: &'a [Value]) -> &'a [Value] {
    let Some(ids) = a.first().and_then(gfx::ids_of) else { return a };
    match ids.first().and_then(|&id| gfx::resolve(it, id)) {
        Some(Target::Axes(f, ax)) => {
            it.current_figure = Some(f);
            it.figures[f].current_axes = ax;
            &a[1..]
        }
        Some(Target::Figure(f)) | Some(Target::FigObj(f, _)) => {
            it.current_figure = Some(f);
            &a[1..]
        }
        _ => a,
    }
}

// ---- tiled layouts -------------------------------------------------------------------------

fn layout_of(it: &mut Interpreter, f: usize) -> Option<usize> {
    it.figures[f].objects.iter().position(|o| o.kind == FigObjKind::Layout)
}

fn flow_grid(n: usize) -> (usize, usize) {
    let cols = (n as f64).sqrt().ceil().max(1.0) as usize;
    (n.div_ceil(cols).max(1), cols)
}

fn tiledlayout(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let (pos, props) = split_props(a)?;
    let flow = pos.is_empty() && a.first().map(|v| v.text().map(|t| t.eq_ignore_ascii_case("flow")).unwrap_or(false)).unwrap_or(true);
    let (props, flow, grid) = if !pos.is_empty() {
        let r = pos[0].as_usize()?;
        let c = match pos.get(1) {
            Some(v) => v.as_usize()?,
            None => return rt("Specify the grid size as two positive integers or as 'flow'."),
        };
        (props, false, (r.max(1), c.max(1)))
    } else if a.first().map(is_textlike).unwrap_or(false) && a.len() % 2 == 1 {
        // tiledlayout(arrangement, name, value, ...)
        let (_, p) = split_props(&a[1..])?;
        let arr = a[0].text()?.to_lowercase();
        if !matches!(arr.as_str(), "flow" | "vertical" | "horizontal") {
            return rt(format!("'{}' is not a valid tile arrangement.", a[0].text()?));
        }
        (p, true, (1, 1))
    } else {
        (props, flow, (1, 1))
    };
    let id = gfx::alloc(it);
    let fig = it.current_figure();
    let fid = fig.id;
    *fig = graphics::Figure::new(fid);
    fig.grid = grid;
    fig.flow = flow;
    fig.axes = vec![Axes::default(); grid.0 * grid.1];
    fig.objects.push(FigObject::new(id, FigObjKind::Layout));
    apply_props(it, &[id], &props)?;
    handles_out(it, &[id], nargout)
}

fn nexttile(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let f = it.current_figure_index();
    if layout_of(it, f).is_none() {
        tiledlayout(it, &[Value::chars("flow")], 0)?;
    }
    let fig = &mut it.figures[f];
    let k = match a.first() {
        // nexttile(span) with a 1x2 span takes the next tile; spans are not modelled.
        Some(Value::Num(m)) if m.kind != Kind::Char && m.numel() == 1 => {
            let k = m.data[0];
            if k < 1.0 || k.fract() != 0.0 {
                return rt("Tile number must be a positive integer.");
            }
            k as usize - 1
        }
        _ => fig.next_tile,
    };
    if fig.flow {
        let n = (k + 1).max(fig.axes.len().min(fig.next_tile.max(k + 1)));
        let g = flow_grid(n);
        if g.0 * g.1 > fig.axes.len() || fig.grid != g {
            fig.grid = g;
            fig.axes.resize(g.0 * g.1, Axes::default());
        }
    } else if k >= fig.axes.len() {
        return rt(format!("Tile {} does not exist in a {}-by-{} layout.", k + 1, fig.grid.0, fig.grid.1));
    }
    fig.current_axes = k;
    fig.next_tile = k + 1;
    let id = fig.id;
    it.mark_figure_dirty(id);
    if nargout == 0 {
        return none();
    }
    one(it.current_axes_handle())
}

fn layout_grid(it: &mut Interpreter, v: &Value) -> R<(usize, usize)> {
    let id = gfx::ids_of(v).and_then(|i| i.first().copied()).ok_or_else(|| LangError::new("First argument must be a TiledChartLayout.", 0))?;
    match gfx::resolve(it, id) {
        Some(Target::FigObj(f, _)) => Ok(it.figures[f].grid),
        _ => rt("First argument must be a TiledChartLayout."),
    }
}

/// `title(t, ...)` and friends on a tiled layout: the figure's overall title.
pub fn layout_label(it: &mut Interpreter, name: &str, a: &[Value]) -> Option<Ret> {
    let id = a.first().and_then(gfx::ids_of).and_then(|i| i.first().copied())?;
    let Some(Target::FigObj(f, k)) = gfx::resolve(it, id) else { return None };
    if it.figures[f].objects[k].kind != FigObjKind::Layout {
        return None;
    }
    let text = match a.get(1) {
        Some(v) => match v {
            Value::Cell(c) => c.data.iter().map(|x| x.text()).collect::<R<Vec<_>>>().map(|t| t.join("\n")),
            Value::Str(s) => Ok(s.data.iter().map(|x| x.clone().unwrap_or_default()).collect::<Vec<_>>().join("\n")),
            other => other.text(),
        },
        None => Ok(String::new()),
    };
    let text = match text {
        Ok(t) => t,
        Err(e) => return Some(Err(e)),
    };
    if name == "title" || name == "sgtitle" {
        it.figures[f].name = text.clone();
    }
    it.figures[f].objects[k].text = text;
    let fid = it.figures[f].id;
    it.mark_figure_dirty(fid);
    Some(none())
}

// ---- annotations ---------------------------------------------------------------------------

fn annotation(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let kind_name = arg(a, 0, "annotation")?.text()?.to_lowercase();
    let kind = match kind_name.as_str() {
        "line" => FigObjKind::Line,
        "arrow" => FigObjKind::Arrow,
        "doublearrow" => FigObjKind::DoubleArrow,
        "textarrow" => FigObjKind::TextArrow,
        "textbox" => FigObjKind::TextBox,
        "rectangle" => FigObjKind::Rectangle,
        "ellipse" => FigObjKind::Ellipse,
        _ => return rt(format!("Invalid annotation type '{}'.", a[0].text()?)),
    };
    let (pos, props) = split_props(&a[1..])?;
    let id = gfx::alloc(it);
    let mut o = FigObject::new(id, kind);
    if kind.is_line() {
        if let [x, y, ..] = pos {
            let (x, y) = (mat(x)?, mat(y)?);
            if x.numel() != 2 || y.numel() != 2 {
                return rt("X and Y must be 2-element vectors.");
            }
            o.x = [x.data[0], x.data[1]];
            o.y = [y.data[0], y.data[1]];
        }
    } else if let Some(p) = pos.first() {
        let p = mat(p)?;
        if p.numel() != 4 {
            return rt("Position must be a 4-element vector.");
        }
        o.pos = [p.data[0], p.data[1], p.data[2], p.data[3]];
    }
    if kind == FigObjKind::TextBox {
        o.edge = Some([0, 0, 0]);
    }
    it.current_figure().objects.push(o);
    apply_props(it, &[id], &props)?;
    handles_out(it, &[id], nargout)
}

// ---- rectangle, fill3 ----------------------------------------------------------------------

fn rectangle(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let (_, mut props) = split_props(a)?;
    let pos = match take_prop(&mut props, "Position") {
        Some(v) => mat(&v)?.data,
        None => vec![0.0, 0.0, 1.0, 1.0],
    };
    if pos.len() != 4 {
        return rt("Position must be a 4-element vector.");
    }
    let curv = match take_prop(&mut props, "Curvature") {
        Some(v) => {
            let m = mat(&v)?;
            if m.numel() == 1 { vec![m.data[0], m.data[0]] } else { m.data }
        }
        None => vec![0.0, 0.0],
    };
    let (x, y) = graphics::rect_polygon(&pos, &curv);
    let mut s = Series::new(SeriesKind::Rectangle, x, y);
    s.u = pos;
    s.v = curv;
    s.face_none = true;
    s.edge_color = Some([0, 0, 0]);
    s.line_style = LineStyle::Solid;
    let ids = it.add_decoration(vec![s]);
    apply_props(it, &ids, &props)?;
    handles_out(it, &ids, nargout)
}

fn fill3(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let mut series = Vec::new();
    let mut i = 0;
    while i + 2 < a.len() && !is_textlike(&a[i]) {
        let (x, y, z) = (mat(&a[i])?, mat(&a[i + 1])?, mat(&a[i + 2])?);
        let mut s = Series::new(SeriesKind::Patch, x.data, y.data);
        s.z = z.data;
        s.line_style = LineStyle::Solid;
        s.edge_color = Some([0, 0, 0]);
        i += 3;
        if let Some(c) = a.get(i) {
            if is_textlike(c) && graphics::parse_line_spec(&c.text()?).is_some() || matches!(c, Value::Num(m) if m.kind != Kind::Char && m.numel() == 3) {
                s.color = gfx::color_arg(c)?.map(|c| c.0);
                i += 1;
            } else if matches!(c, Value::Num(m) if m.kind != Kind::Char) {
                let m = mat(c)?;
                s.c = m.data;
                s.face_mapped = true;
                i += 1;
            }
        }
        series.push(s);
    }
    let (_, props) = split_props(&a[i..])?;
    crate::builtins_graph3d::add_3d(it, series, &props, nargout)
}

// ---- transparency, lights, camera ----------------------------------------------------------

fn alpha(it: &mut Interpreter, a: &[Value]) -> Ret {
    let a = leading_handle(it, a);
    let Some(v) = a.first() else { return rt("Not enough input arguments.") };
    let value = match v {
        Value::Num(m) if m.kind != Kind::Char && m.numel() == 1 => m.data[0].clamp(0.0, 1.0) as f32,
        _ => match v.text()?.to_lowercase().as_str() {
            "opaque" => 1.0,
            "clear" => 0.0,
            // 'flat', 'interp', 'texture': per-vertex alpha is not modelled.
            _ => return none(),
        },
    };
    let ax = current_axes(it);
    for s in ax.series.iter_mut().filter(|s| matches!(s.kind, SeriesKind::Surface | SeriesKind::Patch | SeriesKind::Rectangle | SeriesKind::Image)) {
        s.face_alpha = value;
    }
    none()
}

fn alim(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    match a.first() {
        Some(v) if !is_textlike(v) => {
            let m = mat(v)?;
            if m.numel() != 2 || !(m.data[0] < m.data[1]) {
                return rt("Limits must be a 2-element vector of increasing numeric values.");
            }
            axes_set(it, "ALim", v.clone())?;
            none()
        }
        Some(_) => none(),
        None => {
            let v = axes_get(it, "ALim")?;
            if nargout == 0 { Ok(Some(vec![v])) } else { one(v) }
        }
    }
}

fn alphamap(a: &[Value]) -> Ret {
    let n = match a.iter().find(|v| !is_textlike(v) && v.numel() == 1) {
        Some(v) => v.as_usize()?,
        None => 64,
    };
    let kind = a.iter().find(|v| is_textlike(v)).map(|v| v.text()).transpose()?.unwrap_or_else(|| "default".into()).to_lowercase();
    let t = |k: usize| if n <= 1 { 1.0 } else { k as f64 / (n - 1) as f64 };
    let data: Vec<f64> = (0..n)
        .map(|k| match kind.as_str() {
            "rampdown" => 1.0 - t(k),
            "vup" => 1.0 - (2.0 * t(k) - 1.0).abs(),
            "vdown" => (2.0 * t(k) - 1.0).abs(),
            "increase" | "decrease" | "spin" => t(k),
            _ => t(k),
        })
        .collect();
    one(Value::Num(Matrix::col(data)))
}

/// Unit vector from the target toward the camera for view (az, el) in degrees.
fn view_dir(az: f64, el: f64) -> [f64; 3] {
    let (az, el) = (az.to_radians(), el.to_radians());
    [az.sin() * el.cos(), -az.cos() * el.cos(), el.sin()]
}

/// Axes centre and ranges ((x0, x1), (y0, y1), (z0, z1)).
fn axes_box(ax: &Axes) -> [(f64, f64); 3] {
    let ((x0, x1), (y0, y1)) = ax.limits();
    let z = ax.zlim.unwrap_or_else(|| ax.zbounds());
    [(x0, x1), (y0, y1), z]
}

fn add_light(it: &mut Interpreter, pos: [f64; 3], style: &str, props: &[(String, Value)], nargout: usize) -> Ret {
    let mut s = Series::new(SeriesKind::Light, vec![pos[0]], vec![pos[1]]);
    s.z = vec![pos[2]];
    s.label = style.to_string();
    let ids = it.add_decoration(vec![s]);
    current_axes(it).lighting_none = false;
    apply_props(it, &ids, props)?;
    handles_out(it, &ids, nargout)
}

fn light(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let (_, mut props) = split_props(a)?;
    let pos = match take_prop(&mut props, "Position") {
        Some(v) => {
            let m = mat(&v)?;
            if m.numel() != 3 {
                return rt("Position must be a 3-element vector.");
            }
            [m.data[0], m.data[1], m.data[2]]
        }
        None => [1.0, 0.0, 1.0],
    };
    let style = take_prop(&mut props, "Style").map(|v| v.text()).transpose()?.unwrap_or_else(|| "infinite".into());
    add_light(it, pos, &style, &props, nargout)
}

/// A position `d` plot-box units from the axes centre in direction (az, el).
fn position_at(ax: &Axes, az: f64, el: f64, d: f64) -> [f64; 3] {
    let b = axes_box(ax);
    let dir = view_dir(az, el);
    let mut p = [0.0; 3];
    for k in 0..3 {
        let (lo, hi) = b[k];
        p[k] = (lo + hi) / 2.0 + d * dir[k] * (hi - lo);
    }
    p
}

fn camlight(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    // An existing light to move comes first.
    let (target, a) = match a.first().and_then(gfx::ids_of) {
        Some(ids) => (ids.first().copied(), &a[1..]),
        None => (None, a),
    };
    let (az0, el0) = current_axes(it).view.unwrap_or((0.0, 90.0));
    let mut style = "local".to_string();
    let mut rest: Vec<&Value> = Vec::new();
    for v in a {
        match v.text().ok().map(|t| t.to_lowercase()).as_deref() {
            Some("local") | Some("infinite") => style = v.text()?.to_lowercase(),
            _ => rest.push(v),
        }
    }
    let (daz, del) = match rest.first() {
        Some(v) if is_textlike(v) => match v.text()?.to_lowercase().as_str() {
            "headlight" => (0.0, 0.0),
            "right" => (30.0, 30.0),
            "left" => (-30.0, 30.0),
            other => return rt(format!("Invalid camlight position '{other}'.")),
        },
        Some(v) => (v.scalar()?, rest.get(1).map(|x| x.scalar()).transpose()?.unwrap_or(0.0)),
        None => (30.0, 30.0),
    };
    let pos = position_at(current_axes(it), az0 + daz, (el0 + del).clamp(-90.0, 90.0), 5.0 * 3f64.sqrt());
    if let Some(id) = target {
        let h = gfx::handle(it, id);
        gfx::call(it, "set", &[h.clone(), Value::chars("Position"), row(&pos), Value::chars("Style"), Value::chars(&style)], 0)?;
        return if nargout == 0 { none() } else { one(h) };
    }
    add_light(it, pos, &style, &[], nargout)
}

fn lightangle(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let (target, a) = match a.first().and_then(gfx::ids_of) {
        Some(ids) => (ids.first().copied(), &a[1..]),
        None => (None, a),
    };
    let az = arg(a, 0, "lightangle")?.scalar()?;
    let el = arg(a, 1, "lightangle")?.scalar()?;
    let pos = view_dir(az, el);
    if let Some(id) = target {
        let h = gfx::handle(it, id);
        gfx::call(it, "set", &[h.clone(), Value::chars("Position"), row(&pos)], 0)?;
        return if nargout == 0 { none() } else { one(h) };
    }
    add_light(it, pos, "infinite", &[], nargout)
}

fn lighting(it: &mut Interpreter, a: &[Value]) -> Ret {
    let a = leading_handle(it, a);
    let mode = arg(a, 0, "lighting")?.text()?.to_lowercase();
    if !matches!(mode.as_str(), "flat" | "gouraud" | "phong" | "none") {
        return rt(format!("Invalid lighting method '{mode}'."));
    }
    current_axes(it).lighting_none = mode == "none";
    none()
}

fn camorbit(it: &mut Interpreter, a: &[Value]) -> Ret {
    let a = leading_handle(it, a);
    let dt = arg(a, 0, "camorbit")?.scalar()?;
    let dp = arg(a, 1, "camorbit")?.scalar()?;
    let ax = current_axes(it);
    let (az, el) = ax.view.unwrap_or((0.0, 90.0));
    ax.view = Some((az + dt, (el + dp).clamp(-90.0, 90.0)));
    none()
}

fn camera_prop(name: &str) -> &'static str {
    match name {
        "campos" => "CameraPosition",
        "camtarget" => "CameraTarget",
        "camup" => "CameraUpVector",
        "camva" => "CameraViewAngle",
        _ => "Projection",
    }
}

/// Camera values MATLAB computes for a 3-D view when they are not set.
fn camera_default(it: &mut Interpreter, prop: &str) -> Option<Value> {
    let ax = current_axes(it).clone();
    let (az, el) = ax.view?;
    let b = axes_box(&ax);
    Some(match prop {
        "CameraTarget" => row(&[(b[0].0 + b[0].1) / 2.0, (b[1].0 + b[1].1) / 2.0, (b[2].0 + b[2].1) / 2.0]),
        "CameraPosition" => row(&position_at(&ax, az, el, 5.0 * 3f64.sqrt())),
        "CameraUpVector" => row(&[0.0, 0.0, 1.0]),
        "CameraViewAngle" => Value::num(10.339584907202),
        _ => return None,
    })
}

fn camera(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let prop = camera_prop(name);
    if let Some(v) = a.first() {
        if is_textlike(v) && v.text()?.eq_ignore_ascii_case("auto") {
            return none();
        }
        if name == "camproj" {
            let t = v.text()?.to_lowercase();
            if !matches!(t.as_str(), "orthographic" | "perspective") {
                return rt("Projection must be 'orthographic' or 'perspective'.");
            }
        }
        axes_set(it, prop, v.clone())?;
        return none();
    }
    let h = it.current_axes_handle();
    let id = gfx::ids_of(&h).and_then(|i| i.first().copied()).unwrap_or(0);
    let v = match it.gfx_store.get(&id).and_then(|m| m.iter().rev().find(|(k, _)| k == prop).map(|(_, v)| v.clone())) {
        Some(v) => v,
        None => match camera_default(it, prop) {
            Some(v) => v,
            None => axes_get(it, prop)?,
        },
    };
    let _ = nargout;
    one(v)
}

fn camzoom(it: &mut Interpreter, a: &[Value]) -> Ret {
    let a = leading_handle(it, a);
    let z = arg(a, 0, "camzoom")?.scalar()?;
    if z <= 0.0 {
        return rt("Zoom factor must be positive.");
    }
    let va = camera(it, "camva", &[], 1)?.and_then(|mut v| v.pop()).map(|v| v.scalar()).transpose()?.unwrap_or(10.339584907202);
    let nv = 2.0 * ((va.to_radians() / 2.0).tan() / z).atan().to_degrees();
    axes_set(it, "CameraViewAngle", Value::num(nv))?;
    none()
}

// ---- color orders --------------------------------------------------------------------------

fn palette(name: &str) -> Option<Vec<[f64; 3]>> {
    let gem: Vec<[f64; 3]> = gfx::COLOR_ORDER.to_vec();
    let p: Vec<[f64; 3]> = match name {
        "gem" => gem,
        "gem12" => gem.into_iter().chain([[1.0, 0.27, 0.227], [0.396, 0.509, 0.992], [1.0, 0.839, 0.039], [0.0, 0.639, 0.639], [0.796, 0.517, 0.364]]).collect(),
        "glow" | "glow12" => {
            let mut v = vec![[0.149, 0.549, 0.866], [0.96, 0.466, 0.16], [1.0, 0.909, 0.392], [0.752, 0.36, 0.984], [0.286, 0.858, 0.25], [0.423, 0.956, 1.0], [0.949, 0.403, 0.772]];
            if name == "glow12" {
                v.extend([[1.0, 0.478, 0.454], [0.49, 0.662, 1.0], [0.996, 0.752, 0.298], [0.121, 0.811, 0.745], [0.862, 0.6, 0.423]]);
            }
            v
        }
        "sail" => vec![[0.062, 0.258, 0.501], [0.329, 0.713, 1.0], [1.0, 0.27, 0.227], [0.564, 0.149, 0.133], [0.066, 0.443, 0.745]],
        "reef" => vec![[0.866, 0.329, 0.0], [0.329, 0.713, 1.0], [0.066, 0.443, 0.745], [0.996, 0.564, 0.262], [0.454, 0.921, 0.854], [0.0, 0.639, 0.639]],
        "meadow" => vec![[0.007, 0.345, 0.054], [0.227, 0.784, 0.192], [1.0, 0.839, 0.039], [0.96, 0.466, 0.16], [0.752, 0.298, 0.043], [0.98, 0.541, 0.831], [0.49, 0.662, 1.0]],
        "dye" => vec![[0.717, 0.192, 0.172], [0.231, 0.666, 0.196], [0.368, 0.133, 0.588], [0.066, 0.443, 0.745], [0.866, 0.329, 0.0], [0.007, 0.47, 0.501], [0.913, 0.317, 0.721]],
        "earth" => vec![[0.062, 0.258, 0.501], [0.717, 0.192, 0.172], [0.611, 0.466, 0.125], [0.007, 0.345, 0.054], [0.862, 0.6, 0.423], [0.372, 0.105, 0.031], [1.0, 0.819, 0.619]],
        _ => return None,
    };
    Some(p)
}

fn palette_arg(v: &Value) -> R<Vec<[f64; 3]>> {
    if is_textlike(v) {
        let n = v.text()?;
        return palette(&n.to_lowercase()).ok_or_else(|| LangError::new(format!("'{n}' is not a valid palette name."), 0));
    }
    map_rows(&mat(v)?)
}

fn colororder(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    if let Some(v) = a.first() {
        let p = palette_arg(v)?;
        // The figure's order applies to its axes, including ones made later.
        let fig = it.current_figure();
        for ax in fig.axes.iter_mut() {
            ax.color_order = None;
        }
        fig.color_order = Some(p);
        if nargout == 0 {
            return none();
        }
    }
    let fig = it.current_figure();
    let p = fig.axes_mut().color_order.clone().or_else(|| fig.color_order.clone()).unwrap_or_else(|| gfx::COLOR_ORDER.to_vec());
    one(rgb_matrix(&p))
}

// ---- charts --------------------------------------------------------------------------------

/// Wedge polygon of a pie slice from angle t0 to t1 (radians from +x, the slice drawn
/// counter-clockwise), centred at (cx, cy).
fn wedge(cx: f64, cy: f64, t0: f64, t1: f64) -> (Vec<f64>, Vec<f64>) {
    let n = (((t1 - t0).abs() / (2.0 * std::f64::consts::PI)) * 100.0).ceil().max(2.0) as usize;
    let mut x = vec![cx];
    let mut y = vec![cy];
    for k in 0..=n {
        let t = t0 + (t1 - t0) * k as f64 / n as f64;
        x.push(cx + t.cos());
        y.push(cy + t.sin());
    }
    x.push(cx);
    y.push(cy);
    (x, y)
}

fn pie(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let x = mat(arg(a, 0, "pie")?)?;
    let mut explode = vec![0.0; x.numel()];
    let mut labels: Option<Vec<String>> = None;
    for v in &a[1..] {
        match v {
            Value::Cell(_) | Value::Str(_) => {
                labels = Some(match v {
                    Value::Cell(c) => c.data.iter().map(|x| x.text()).collect::<R<_>>()?,
                    Value::Str(s) => s.data.iter().map(|x| x.clone().unwrap_or_default()).collect(),
                    _ => unreachable!(),
                })
            }
            Value::Num(m) if m.kind != Kind::Char => explode = m.data.clone(),
            _ => {}
        }
    }
    let total: f64 = x.data.iter().sum();
    if !(total > 0.0) {
        return rt("Data must contain at least one positive value.");
    }
    // Shares: of the total, or the values themselves for a partial pie (sum < 1).
    let frac: Vec<f64> = x.data.iter().map(|&v| if total < 1.0 { v } else { v / total }).collect();
    let mut series = Vec::new();
    let mut t = std::f64::consts::FRAC_PI_2;
    let n = frac.len();
    for (k, &f) in frac.iter().enumerate() {
        // Counter-clockwise from the top, as MATLAB draws.
        let t1 = t + f * 2.0 * std::f64::consts::PI;
        let mid = (t + t1) / 2.0;
        let off = if explode.get(k).copied().unwrap_or(0.0) != 0.0 { 0.1 } else { 0.0 };
        let (cx, cy) = (off * mid.cos(), off * mid.sin());
        let (px, py) = wedge(cx, cy, t, t1);
        let mut p = Series::new(SeriesKind::Patch, px, py);
        p.line_style = LineStyle::Solid;
        p.edge_color = Some([0, 0, 0]);
        p.c = vec![k as f64 + 1.0];
        p.face_mapped = true;
        series.push(p);
        let label = match &labels {
            Some(l) => l.get(k).cloned().unwrap_or_default(),
            None => {
                let pct = f * 100.0;
                if pct > 0.0 && pct < 1.0 { "< 1%".to_string() } else { format!("{}%", pct.round()) }
            }
        };
        let mut tx = Series::new(SeriesKind::Text, vec![cx + 1.1 * mid.cos()], vec![cy + 1.1 * mid.sin()]);
        tx.label = label;
        series.push(tx);
        t = t1;
    }
    let ids = it.add_series(series, XAxisKind::Numeric);
    let ax = current_axes(it);
    ax.data_aspect = Some([1.0, 1.0, 1.0]);
    ax.visible_axes = Some(false);
    ax.xlim = Some((-1.2, 1.2));
    ax.ylim = Some((-1.2, 1.2));
    ax.clim = Some((1.0, n.max(2) as f64));
    handles_out(it, &ids, nargout)
}

/// Faces of a box (6 quads, NaN-separated) from (x0, y0, z0) to (x1, y1, z1).
fn box_faces(x: (f64, f64), y: (f64, f64), z: (f64, f64), out: &mut (Vec<f64>, Vec<f64>, Vec<f64>)) {
    let c = |i: usize| -> [f64; 3] { [if i & 1 == 0 { x.0 } else { x.1 }, if i & 2 == 0 { y.0 } else { y.1 }, if i & 4 == 0 { z.0 } else { z.1 }] };
    for face in [[0, 1, 3, 2], [4, 5, 7, 6], [0, 1, 5, 4], [2, 3, 7, 6], [0, 2, 6, 4], [1, 3, 7, 5]] {
        if !out.0.is_empty() {
            out.0.push(f64::NAN);
            out.1.push(f64::NAN);
            out.2.push(f64::NAN);
        }
        for i in face {
            let p = c(i);
            out.0.push(p[0]);
            out.1.push(p[1]);
            out.2.push(p[2]);
        }
    }
}

fn bar3(it: &mut Interpreter, a: &[Value], horizontal: bool, nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let (pos, props) = split_props(a)?;
    let nums: Vec<&Value> = pos.iter().filter(|v| matches!(v, Value::Num(m) if m.kind != Kind::Char)).collect();
    let (ypos, z, width) = match nums.as_slice() {
        [z] => (None, mat(z)?, 0.8),
        [y, z] if z.numel() > 1 || y.numel() > 1 => (Some(mat(y)?), mat(z)?, 0.8),
        [z, w] => (None, mat(z)?, w.scalar()?),
        [y, z, w, ..] => (Some(mat(y)?), mat(z)?, w.scalar()?),
        _ => return rt("Not enough input arguments."),
    };
    let z = if z.rows() == 1 { Matrix::new(z.numel(), 1, z.data.clone()) } else { z };
    let (m, n) = (z.rows(), z.cols());
    let rows: Vec<f64> = match ypos {
        Some(y) => y.data,
        None => (1..=m).map(|v| v as f64).collect(),
    };
    let dy = graphics::bar_spacing(&rows);
    let h = width / 2.0;
    let mut series = Vec::new();
    for j in 0..n {
        let mut f = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..m {
            let v = z.data[j * m + i];
            if !v.is_finite() {
                continue;
            }
            let (cx, cy) = (j as f64 + 1.0, rows[i]);
            let (bx, by, bz) = ((cx - h, cx + h), (cy - h * dy, cy + h * dy), (0.0f64.min(v), 0.0f64.max(v)));
            if horizontal {
                box_faces(bz, bx, by, &mut f);
            } else {
                box_faces(bx, by, bz, &mut f);
            }
        }
        let mut s = Series::new(SeriesKind::Patch, f.0, f.1);
        s.z = f.2;
        s.line_style = LineStyle::Solid;
        s.edge_color = Some([0, 0, 0]);
        s.c = vec![j as f64 + 1.0];
        s.face_mapped = true;
        series.push(s);
    }
    let r = crate::builtins_graph3d::add_3d(it, series, &props, nargout)?;
    current_axes(it).clim = Some((1.0, n.max(2) as f64));
    Ok(r)
}

/// Bin counts and centres as `hist` computes them.
fn hist_counts(y: &[f64], spec: Option<&Matrix>) -> R<(Vec<f64>, Vec<f64>)> {
    let finite: Vec<f64> = y.iter().copied().filter(|v| !v.is_nan()).collect();
    let (lo, hi) = finite.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, &v| (a.0.min(v), a.1.max(v)));
    let centres: Vec<f64> = match spec {
        Some(m) if m.numel() > 1 => m.data.clone(),
        other => {
            let nb = other.map(|m| m.data[0] as usize).unwrap_or(10).max(1);
            let (lo, hi) = if lo == hi || !lo.is_finite() { (lo - 0.5, hi + 0.5) } else { (lo, hi) };
            let w = (hi - lo) / nb as f64;
            (0..nb).map(|k| (lo + w * k as f64) + w / 2.0).collect()
        }
    };
    let nb = centres.len();
    let edges: Vec<f64> = (0..nb.saturating_sub(1)).map(|k| centres[k] + (centres[k + 1] - centres[k]) / 2.0).collect();
    let mut counts = vec![0.0; nb];
    for &v in &finite {
        // Interior bins include their right edge.
        let k = edges.iter().position(|&e| v <= e).unwrap_or(nb - 1);
        counts[k] += 1.0;
    }
    Ok((counts, centres))
}

fn hist(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let y = mat(arg(a, 0, "hist")?)?;
    let spec = a.get(1).map(mat).transpose()?;
    let vector = y.rows() == 1 || y.cols() == 1;
    let cols: Vec<Vec<f64>> = if vector { vec![y.data.clone()] } else { (0..y.cols()).map(|j| y.data[j * y.rows()..(j + 1) * y.rows()].to_vec()).collect() };
    let mut all = Vec::new();
    let mut centres = Vec::new();
    for c in &cols {
        let (n, x) = hist_counts(c, spec.as_ref())?;
        all.push(n);
        centres = x;
    }
    if nargout > 0 {
        let nb = centres.len();
        let counts = if vector { Value::Num(Matrix::row(all.pop().unwrap())) } else { Value::Num(Matrix::new(nb, all.len(), all.concat())) };
        return Ok(Some(vec![counts, Value::Num(Matrix::row(centres))]));
    }
    let spacing = graphics::bar_spacing(&centres);
    let series = all
        .into_iter()
        .map(|n| {
            let mut s = Series::new(SeriesKind::Bar, centres.clone(), n);
            s.bar_width = Some(spacing);
            s
        })
        .collect();
    it.add_series(series, XAxisKind::Numeric);
    none()
}

fn pareto(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let y = mat(arg(a, 0, "pareto")?)?;
    let names: Option<Vec<String>> = match a.get(1) {
        Some(Value::Cell(c)) => Some(c.data.iter().map(|x| x.text()).collect::<R<_>>()?),
        Some(Value::Str(s)) => Some(s.data.iter().map(|x| x.clone().unwrap_or_default()).collect()),
        _ => None,
    };
    let mut order: Vec<usize> = (0..y.numel()).collect();
    order.sort_by(|&p, &q| y.data[q].partial_cmp(&y.data[p]).unwrap_or(std::cmp::Ordering::Equal));
    let vals: Vec<f64> = order.iter().map(|&i| y.data[i]).collect();
    let xs: Vec<f64> = (1..=vals.len()).map(|v| v as f64).collect();
    let mut cum = Vec::with_capacity(vals.len());
    let mut acc = 0.0;
    for v in &vals {
        acc += v;
        cum.push(acc);
    }
    let bars = Series::new(SeriesKind::Bar, xs.clone(), vals);
    let mut line = Series::new(SeriesKind::Line, xs, cum);
    line.marker = None;
    let ids = it.add_series(vec![bars, line], XAxisKind::Numeric);
    if let Some(names) = names {
        let ax = current_axes(it);
        ax.xticks.values = Some((1..=order.len()).map(|v| v as f64).collect());
        ax.xticks.labels = Some(order.iter().map(|&i| names.get(i).cloned().unwrap_or_default()).collect());
    }
    handles_out(it, &ids, nargout)
}

fn feather(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let u = mat(arg(a, 0, "feather")?)?;
    let (du, dv) = match a.get(1) {
        Some(v) if !is_textlike(v) => (u.data.clone(), mat(v)?.data),
        _ => (u.data.clone(), u.im.clone().unwrap_or_else(|| vec![0.0; u.numel()])),
    };
    let n = du.len();
    let mut s = Series::new(SeriesKind::Quiver, (1..=n).map(|v| v as f64).collect(), vec![0.0; n]);
    s.u = du;
    s.v = dv;
    let mut axis = Series::new(SeriesKind::Line, vec![1.0, n as f64], vec![0.0, 0.0]);
    axis.color = Some([0, 0, 0]);
    let ids = it.add_series(vec![s, axis], XAxisKind::Numeric);
    current_axes(it).data_aspect = Some([1.0, 1.0, 1.0]);
    handles_out(it, &ids, nargout)
}

/// X, Y, Z of a surface-style call (`waterfall(Z)`, `waterfall(X, Y, Z)`).
fn xyz(a: &[Value]) -> R<(usize, usize, Vec<f64>, Vec<f64>, Vec<f64>)> {
    let z = mat(a.last().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?)?;
    let (m, n) = (z.rows(), z.cols());
    if a.len() >= 3 {
        let (x, y) = (mat(&a[0])?, mat(&a[1])?);
        let full = |v: &Matrix, along_cols: bool| -> Vec<f64> {
            if v.numel() == m * n {
                v.data.clone()
            } else {
                (0..m * n).map(|k| if along_cols { v.data[(k / m).min(v.numel() - 1)] } else { v.data[(k % m).min(v.numel() - 1)] }).collect()
            }
        };
        return Ok((m, n, full(&x, true), full(&y, false), z.data));
    }
    let x = (0..m * n).map(|k| (k / m + 1) as f64).collect();
    let y = (0..m * n).map(|k| (k % m + 1) as f64).collect();
    Ok((m, n, x, y, z.data))
}

fn waterfall(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let (pos, props) = split_props(a)?;
    let (m, n, x, y, z) = xyz(pos)?;
    let (zlo, zhi) = z.iter().filter(|v| v.is_finite()).fold((f64::INFINITY, f64::NEG_INFINITY), |a, &v| (a.0.min(v), a.1.max(v)));
    let map = graphics::colormap("parula", 256).unwrap_or_default();
    let mut series = Vec::new();
    for i in 0..m {
        let idx: Vec<usize> = (0..n).map(|j| j * m + i).collect();
        let mut s = Series::new(SeriesKind::Line, idx.iter().map(|&k| x[k]).collect(), idx.iter().map(|&k| y[k]).collect());
        s.z = idx.iter().map(|&k| z[k]).collect();
        let mean = s.z.iter().sum::<f64>() / n.max(1) as f64;
        s.color = graphics::map_color(&map, (zlo, zhi), mean).map(to_u8);
        series.push(s);
    }
    crate::builtins_graph3d::add_3d(it, series, &props, nargout)
}

fn ribbon(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let (pos, props) = split_props(a)?;
    let (y, z, width) = match pos {
        [z] => (None, mat(z)?, 0.75),
        [z, w] if w.numel() == 1 => (None, mat(z)?, w.scalar()?),
        [y, z] => (Some(mat(y)?), mat(z)?, 0.75),
        [y, z, w, ..] => (Some(mat(y)?), mat(z)?, w.scalar()?),
        _ => return rt("Not enough input arguments."),
    };
    let z = if z.rows() == 1 { Matrix::new(z.numel(), 1, z.data.clone()) } else { z };
    let (m, n) = (z.rows(), z.cols());
    let ys: Vec<f64> = match y {
        Some(y) if y.numel() >= m => y.data[..m].to_vec(),
        _ => (1..=m).map(|v| v as f64).collect(),
    };
    let mut series = Vec::new();
    for j in 0..n {
        let cx = j as f64 + 1.0;
        let mut s = Series::new(SeriesKind::Surface, Vec::new(), Vec::new());
        for side in [-width / 2.0, width / 2.0] {
            for i in 0..m {
                s.x.push(cx + side);
                s.y.push(ys[i]);
                s.z.push(z.data[j * m + i]);
                s.c.push(cx);
            }
        }
        s.grid = (m, 2);
        s.face_mapped = true;
        s.edge_color = Some([0, 0, 0]);
        series.push(s);
    }
    let r = crate::builtins_graph3d::add_3d(it, series, &props, nargout)?;
    current_axes(it).clim = Some((1.0, n.max(2) as f64));
    Ok(r)
}

fn comet(it: &mut Interpreter, a: &[Value], three: bool, nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let nums: Vec<Matrix> = a.iter().map(mat).collect::<R<_>>()?;
    let (x, y, z) = match (three, nums.len()) {
        (false, 1) => ((1..=nums[0].numel()).map(|v| v as f64).collect(), nums[0].data.clone(), Vec::new()),
        (false, _) => (nums[0].data.clone(), nums[1].data.clone(), Vec::new()),
        (true, n) if n >= 3 => (nums[0].data.clone(), nums[1].data.clone(), nums[2].data.clone()),
        (true, _) => {
            let zz = nums[0].data.clone();
            let n = zz.len();
            (zz.iter().map(|t| t.cos()).collect(), zz.iter().map(|t| t.sin()).collect::<Vec<_>>().into_iter().take(n).collect(), zz)
        }
    };
    let mut line = Series::new(SeriesKind::Line, x.clone(), y.clone());
    line.z = z.clone();
    let k = x.len().saturating_sub(1);
    let mut head = Series::new(SeriesKind::Line, vec![x.get(k).copied().unwrap_or(f64::NAN)], vec![y.get(k).copied().unwrap_or(f64::NAN)]);
    head.z = z.get(k).map(|&v| vec![v]).unwrap_or_default();
    head.marker = Some(Marker::Circle);
    head.line_style = LineStyle::None;
    head.color = Some([255, 0, 0]);
    if three {
        crate::builtins_graph3d::add_3d(it, vec![line, head], &[], nargout)
    } else {
        let ids = it.add_series(vec![line, head], XAxisKind::Numeric);
        handles_out(it, &ids, nargout)
    }
}

// ---- utilities -----------------------------------------------------------------------------

fn brighten(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = leading_handle(it, a);
    let (map, beta) = match a {
        [m, b] => (Some(map_rows(&mat(m)?)?), b.scalar()?),
        [b] => (None, b.scalar()?),
        _ => return rt("Not enough input arguments."),
    };
    if !(-1.0 < beta && beta < 1.0) {
        return rt("Beta must be greater than -1 and less than 1.");
    }
    let gamma = if beta > 0.0 { 1.0 - beta } else { 1.0 / (1.0 + beta) };
    let base = match &map {
        Some(m) => m.clone(),
        None => {
            let fig = it.current_figure();
            fig.colormap.clone().unwrap_or_else(|| graphics::colormap("parula", 256).unwrap_or_default())
        }
    };
    let out: Vec<[f64; 3]> = base.iter().map(|c| [c[0].powf(gamma), c[1].powf(gamma), c[2].powf(gamma)]).collect();
    if map.is_none() && nargout == 0 {
        it.current_figure().colormap = Some(out);
        return none();
    }
    one(rgb_matrix(&out))
}

fn mat4(m: [[f64; 4]; 4]) -> Value {
    let mut data = vec![0.0; 16];
    for i in 0..4 {
        for j in 0..4 {
            data[j * 4 + i] = m[i][j];
        }
    }
    Value::Num(Matrix::new(4, 4, data))
}

fn mul4(a: [[f64; 4]; 4], b: [[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut c = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            c[i][j] = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    c
}

const I4: [[f64; 4]; 4] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

fn rotation(axis: [f64; 3], t: f64) -> [[f64; 4]; 4] {
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    let [x, y, z] = if n > 0.0 { [axis[0] / n, axis[1] / n, axis[2] / n] } else { [0.0, 0.0, 1.0] };
    let (c, s) = (t.cos(), t.sin());
    let k = 1.0 - c;
    [
        [c + x * x * k, x * y * k - z * s, x * z * k + y * s, 0.0],
        [y * x * k + z * s, c + y * y * k, y * z * k - x * s, 0.0],
        [z * x * k - y * s, z * y * k + x * s, c + z * z * k, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn makehgtform(a: &[Value]) -> Ret {
    let mut m = I4;
    let mut i = 0;
    while i < a.len() {
        let op = a[i].text()?.to_lowercase();
        let v = mat(arg(a, i + 1, "makehgtform")?)?;
        let t = match op.as_str() {
            "translate" => {
                if v.numel() != 3 {
                    return rt("Translation must be a 3-element vector.");
                }
                let mut t = I4;
                for k in 0..3 {
                    t[k][3] = v.data[k];
                }
                t
            }
            "scale" => {
                let s = if v.numel() == 1 { [v.data[0]; 3] } else if v.numel() == 3 { [v.data[0], v.data[1], v.data[2]] } else { return rt("Scale must be a scalar or a 3-element vector.") };
                let mut t = I4;
                for k in 0..3 {
                    t[k][k] = s[k];
                }
                t
            }
            "xrotate" => rotation([1.0, 0.0, 0.0], v.data[0]),
            "yrotate" => rotation([0.0, 1.0, 0.0], v.data[0]),
            "zrotate" => rotation([0.0, 0.0, 1.0], v.data[0]),
            "axisrotate" => {
                let t = arg(a, i + 2, "makehgtform")?.scalar()?;
                i += 1;
                if v.numel() != 3 {
                    return rt("Axis must be a 3-element vector.");
                }
                rotation([v.data[0], v.data[1], v.data[2]], t)
            }
            _ => return rt(format!("Invalid transformation '{op}'.")),
        };
        m = mul4(m, t);
        i += 2;
    }
    one(mat4(m))
}

fn viewmtx(a: &[Value]) -> Ret {
    let az = arg(a, 0, "viewmtx")?.scalar()?.to_radians();
    let el = arg(a, 1, "viewmtx")?.scalar()?.to_radians();
    if a.get(2).map(|v| v.scalar()).transpose()?.unwrap_or(0.0) != 0.0 {
        return rt("Perspective view matrices (a nonzero view angle) are not supported in this version.");
    }
    let (sa, ca, se, ce) = (az.sin(), az.cos(), el.sin(), el.cos());
    one(mat4([[ca, sa, 0.0, 0.0], [-se * sa, se * ca, ce, 0.0], [ce * sa, -ce * ca, se, 0.0], [0.0, 0.0, 0.0, 1.0]]))
}

const GREEK: &[&str] = &[
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "vartheta", "iota", "kappa", "lambda", "mu", "nu", "xi", "pi", "rho", "sigma",
    "varsigma", "tau", "upsilon", "phi", "chi", "psi", "omega", "Gamma", "Delta", "Theta", "Lambda", "Xi", "Pi", "Sigma", "Upsilon", "Phi", "Psi", "Omega",
];

/// One identifier or number as TeX: `{x}`, `{\alpha}`, `{\lambda_{12}}`.
fn tex_word(w: &str) -> String {
    if GREEK.contains(&w) {
        return format!("{{\\{w}}}");
    }
    let split = w.find(|c: char| c.is_ascii_digit()).unwrap_or(w.len());
    let (head, digits) = w.split_at(split);
    if !digits.is_empty() && GREEK.contains(&head) && digits.chars().all(|c| c.is_ascii_digit()) {
        return format!("{{\\{head}_{{{digits}}}}}");
    }
    format!("{{{w}}}")
}

fn texlabel_str(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let word_end = |i: usize| {
        let mut j = i;
        while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '.' && j > i && chars[i].is_ascii_digit()) {
            j += 1;
        }
        j
    };
    while i < chars.len() {
        let c = chars[i];
        if c.is_alphanumeric() {
            let j = word_end(i);
            out.push_str(&tex_word(&chars[i..j].iter().collect::<String>()));
            i = j;
        } else if (c == '^' || c == '_') && i + 1 < chars.len() {
            out.push(c);
            if chars[i + 1] == '(' {
                // A parenthesized exponent becomes the braced group.
                let mut depth = 0;
                let mut j = i + 1;
                while j < chars.len() {
                    match chars[j] {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                let inner: String = chars[i + 2..j.min(chars.len())].iter().collect();
                out.push_str(&format!("{{{}}}", texlabel_str(&inner)));
                i = j + 1;
            } else {
                let j = word_end(i + 1).max(i + 2);
                out.push_str(&format!("{{{}}}", chars[i + 1..j].iter().collect::<String>()));
                i = j;
            }
        } else if c == '*' {
            out.push(' ');
            i += 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn texlabel(a: &[Value]) -> Ret {
    let s = arg(a, 0, "texlabel")?.text()?;
    if a.get(1).map(|v| v.text().map(|t| t.eq_ignore_ascii_case("literal")).unwrap_or(false)).unwrap_or(false) {
        return one(Value::chars(&s));
    }
    one(Value::chars(&texlabel_str(&s)))
}

fn rgbplot(it: &mut Interpreter, a: &[Value]) -> Ret {
    let m = map_rows(&mat(arg(a, 0, "rgbplot")?)?)?;
    let x: Vec<f64> = (1..=m.len()).map(|v| v as f64).collect();
    let series = [[255, 0, 0], [0, 255, 0], [0, 0, 255]]
        .iter()
        .enumerate()
        .map(|(k, &c)| {
            let mut s = Series::new(SeriesKind::Line, x.clone(), m.iter().map(|r| r[k]).collect());
            s.color = Some(c);
            s
        })
        .collect();
    it.add_series(series, XAxisKind::Numeric);
    none()
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "tiledlayout" => tiledlayout(it, a, nargout),
        "nexttile" => nexttile(it, a, nargout),
        "tilenum" => {
            let (_, c) = layout_grid(it, arg(a, 0, name)?)?;
            let r = arg(a, 1, name)?.scalar()?;
            let col = arg(a, 2, name)?.scalar()?;
            one(Value::num((r - 1.0) * c as f64 + col))
        }
        "tilerowcol" => {
            let (_, c) = layout_grid(it, arg(a, 0, name)?)?;
            let k = arg(a, 1, name)?.scalar()? as usize;
            if k == 0 {
                return rt("Tile number must be a positive integer.");
            }
            Ok(Some(vec![Value::num(((k - 1) / c + 1) as f64), Value::num(((k - 1) % c + 1) as f64)]))
        }
        "annotation" => annotation(it, a, nargout),
        "rectangle" => rectangle(it, a, nargout),
        "fill3" => fill3(it, a, nargout),
        "alpha" => alpha(it, a),
        "alim" => alim(it, a, nargout),
        "alphamap" => alphamap(leading_handle(it, a)),
        "light" => light(it, a, nargout),
        "camlight" => camlight(it, a, nargout),
        "lightangle" => lightangle(it, a, nargout),
        "lighting" => lighting(it, a),
        "material" => none(),
        "camorbit" => camorbit(it, a),
        "campos" | "camtarget" | "camup" | "camva" | "camproj" => camera(it, name, a, nargout),
        "camzoom" => camzoom(it, a),
        "camroll" | "camdolly" | "campan" | "camlookat" => {
            leading_handle(it, a);
            none()
        }
        "colororder" => colororder(it, a, nargout),
        "orderedcolors" => one(rgb_matrix(&palette_arg(arg(a, 0, name)?)?)),
        "pie" => pie(it, a, nargout),
        "bar3" => bar3(it, a, false, nargout),
        "bar3h" => bar3(it, a, true, nargout),
        "hist" => hist(it, a, nargout),
        "pareto" => pareto(it, a, nargout),
        "feather" => feather(it, a, nargout),
        "waterfall" => waterfall(it, a, nargout),
        "ribbon" => ribbon(it, a, nargout),
        "comet" => comet(it, a, false, nargout),
        "comet3" => comet(it, a, true, nargout),
        "newplot" => {
            let ax = current_axes(it);
            if !ax.hold {
                let (grid, id) = (ax.grid, ax.id);
                *ax = Axes { grid, id, ..Axes::default() };
            }
            if nargout == 0 {
                return none();
            }
            one(it.current_axes_handle())
        }
        "shg" | "refresh" => none(),
        "gco" | "gcbo" | "gcbf" => one(gfx::handles(it, &[])),
        "brighten" => brighten(it, a, nargout),
        "makehgtform" => makehgtform(a),
        "viewmtx" => viewmtx(a),
        "pbaspect" => {
            let a = leading_handle(it, a);
            match a.first() {
                Some(v) if !is_textlike(v) => {
                    axes_set(it, "PlotBoxAspectRatio", v.clone())?;
                    none()
                }
                Some(_) => none(),
                None => one(axes_get(it, "PlotBoxAspectRatio")?),
            }
        }
        "texlabel" => texlabel(a),
        "rgbplot" => rgbplot(it, a),
        _ => Ok(None),
    }
}
