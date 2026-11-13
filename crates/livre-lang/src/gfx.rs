//! Graphics handles: the root, figures, axes, their labels and rulers, and plotted objects
//! as MATLAB handle objects with `get`/`set` and dot access to their properties.
//!
//! A handle value is an object of the MATLAB class (`matlab.graphics.chart.primitive.Line`,
//! ...) holding a column of handle numbers, so arrays of handles (`h = plot(Y)`) index and
//! compare like MATLAB's. Properties that the renderer uses map onto the figure model;
//! others are kept per handle so `get` returns what `set` stored, and the rest report
//! MATLAB's defaults (from `gfx_props.tsv`, recorded from MATLAB R2023b).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::builtins::{arg, none, one, Ret};
use crate::class::{ClassDef, Object};
use crate::error::{rt, LangError};
use crate::graphics::{self, Axes, Figure, LineStyle, Marker, Series, SeriesKind};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &["set", "get", "isgraphics", "ishandle", "ishghandle", "groot", "gobjects", "findobj", "findall", "allchild", "ancestor", "axes", "cla", "isvalid", "findfigs", "handle", "copyobj"];

/// Handle numbers: 0 is the root, figure numbers are their own handles, and axes take a
/// block of `SLOTS` numbers from `OBJ_BASE` up (their labels and rulers use the block).
pub const ROOT: u64 = 0;
pub const OBJ_BASE: u64 = 1 << 20;
pub const SLOTS: u64 = 16;
const PLACEHOLDER: u64 = u64::MAX >> 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Title = 1,
    XLabel,
    YLabel,
    ZLabel,
    Subtitle,
    XAxis,
    YAxis,
    ZAxis,
    Legend,
    /// The right y ruler of `yyaxis` axes (`YAxis` is then the left one).
    YAxisRight,
    /// The y label of the inactive side of `yyaxis` axes.
    YLabelOther,
    /// The `SecondaryLabel` texts of the x, y and z rulers.
    XSecondary,
    YSecondary,
    ZSecondary,
}

const PARTS: [Part; 14] = [
    Part::Title, Part::XLabel, Part::YLabel, Part::ZLabel, Part::Subtitle, Part::XAxis, Part::YAxis, Part::ZAxis, Part::Legend, Part::YAxisRight, Part::YLabelOther,
    Part::XSecondary, Part::YSecondary, Part::ZSecondary,
];

/// The secondary label part of a ruler part.
fn secondary_of(p: Part) -> Option<(Part, usize)> {
    match p {
        Part::XAxis => Some((Part::XSecondary, 0)),
        Part::YAxis | Part::YAxisRight => Some((Part::YSecondary, 1)),
        Part::ZAxis => Some((Part::ZSecondary, 2)),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Root,
    Figure(usize),
    Axes(usize, usize),
    Series(usize, usize, usize),
    Part(usize, usize, Part),
    /// A tiled layout or annotation of figure `.0`: `figures[.0].objects[.1]`.
    FigObj(usize, usize),
}

thread_local! {
    /// Whether a root default (`set(groot, 'defaultAxesColor', ...)`) was ever set: only then
    /// do builtin calls look for the objects they made.
    static DEFAULTS_SET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn defaults_active() -> bool {
    DEFAULTS_SET.with(|d| d.get())
}

/// The root defaults, as (name without `default`, value).
fn root_defaults(it: &Interpreter) -> Vec<(String, Value)> {
    let Some(m) = it.gfx_store.get(&ROOT) else { return Vec::new() };
    m.iter().filter(|(n, _)| n.len() > 7 && n[..7].eq_ignore_ascii_case("default")).map(|(n, v)| (n[7..].to_string(), v.clone())).collect()
}

/// Handles of every figure, axes and plotted object (giving axes their handles).
pub(crate) fn snapshot_ids(it: &mut Interpreter) -> Vec<u64> {
    let mut out = Vec::new();
    for f in 0..it.figures.len() {
        out.push(it.figures[f].id as u64);
        for a in 0..it.figures[f].axes.len() {
            out.push(axes_id(it, f, a));
            out.extend(it.figures[f].axes[a].series.iter().map(|s| s.id));
        }
    }
    out
}

/// Applies the root defaults to the objects a builtin call made (those not in `before`),
/// leaving the properties the call named itself.
pub(crate) fn apply_new_defaults(it: &mut Interpreter, before: &[u64], args: &[Value]) {
    let skip: Vec<String> = args.iter().filter_map(|a| if a.is_text() { a.text().ok() } else { None }).collect();
    for id in snapshot_ids(it) {
        if id != 0 && !before.contains(&id) {
            apply_defaults(it, id, &skip);
        }
    }
}

/// Sets each root default that applies to object `id`, except the properties in `skip`.
pub(crate) fn apply_defaults(it: &mut Interpreter, id: u64, skip: &[String]) {
    let defs = root_defaults(it);
    if defs.is_empty() {
        return;
    }
    let Some(t) = resolve(it, id) else { return };
    let class = class_of(it, t);
    if crate::charts::is_chart(class) {
        return;
    }
    let Ok(Some(kind)) = get_prop(it, id, "Type").map(|v| v.text().ok()) else { return };
    for (n, v) in defs {
        if n.len() <= kind.len() || !n[..kind.len()].eq_ignore_ascii_case(&kind) {
            continue;
        }
        let rest = &n[kind.len()..];
        // Plotted lines take their colors from the color order, not defaultLineColor.
        if class.contains(".chart.") && rest.eq_ignore_ascii_case("Color") {
            continue;
        }
        if skip.iter().any(|s| s.eq_ignore_ascii_case(rest)) {
            continue;
        }
        if let Some(p) = canonical(class, rest) {
            let _ = set_prop(it, id, &p, v);
        }
    }
}

/// The next free handle number for an axes or plotted object.
pub fn alloc(it: &mut Interpreter) -> u64 {
    let id = it.next_handle.max(OBJ_BASE);
    it.next_handle = id + SLOTS;
    id
}

/// The handle of axes `a` of figure `f`, assigned on first use.
pub fn axes_id(it: &mut Interpreter, f: usize, a: usize) -> u64 {
    if it.figures[f].axes[a].id == 0 {
        let id = alloc(it);
        it.figures[f].axes[a].id = id;
    }
    it.figures[f].axes[a].id
}

pub fn resolve(it: &Interpreter, id: u64) -> Option<Target> {
    if id == ROOT {
        return Some(Target::Root);
    }
    if id < OBJ_BASE {
        return it.figures.iter().position(|f| f.id as u64 == id).map(Target::Figure);
    }
    let base = id - (id - OBJ_BASE) % SLOTS;
    let off = (id - base) as usize;
    for (fi, f) in it.figures.iter().enumerate() {
        if let Some(k) = f.objects.iter().position(|o| o.id == id) {
            return Some(Target::FigObj(fi, k));
        }
        for (ai, ax) in f.axes.iter().enumerate() {
            if ax.id == base && ax.id != 0 {
                return match off {
                    0 => Some(Target::Axes(fi, ai)),
                    k => PARTS.get(k - 1).map(|p| Target::Part(fi, ai, *p)),
                };
            }
            if off == 0 {
                if let Some(si) = ax.series.iter().position(|s| s.id == id) {
                    return Some(Target::Series(fi, ai, si));
                }
            }
        }
    }
    None
}

fn series_class(s: &Series) -> &'static str {
    if let Some(c) = s.class_hint {
        return c;
    }
    match s.kind {
        SeriesKind::Line => "matlab.graphics.chart.primitive.Line",
        SeriesKind::Scatter => "matlab.graphics.chart.primitive.Scatter",
        SeriesKind::Bar | SeriesKind::BarH => "matlab.graphics.chart.primitive.Bar",
        SeriesKind::Stairs => "matlab.graphics.chart.primitive.Stair",
        SeriesKind::Area => "matlab.graphics.chart.primitive.Area",
        SeriesKind::Stem => "matlab.graphics.chart.primitive.Stem",
        SeriesKind::ErrorBar => "matlab.graphics.chart.primitive.ErrorBar",
        SeriesKind::Patch => "matlab.graphics.primitive.Patch",
        SeriesKind::Text => "matlab.graphics.primitive.Text",
        SeriesKind::ConstantLine => "matlab.graphics.chart.decoration.ConstantLine",
        SeriesKind::Region => "matlab.graphics.chart.decoration.ConstantRegion",
        SeriesKind::Surface => "matlab.graphics.chart.primitive.Surface",
        SeriesKind::Rectangle => "matlab.graphics.primitive.Rectangle",
        SeriesKind::Light => "matlab.graphics.primitive.Light",
        SeriesKind::Image => "matlab.graphics.primitive.Image",
        SeriesKind::Contour => "matlab.graphics.chart.primitive.Contour",
        SeriesKind::Quiver => "matlab.graphics.chart.primitive.Quiver",
    }
}

fn class_of(it: &Interpreter, t: Target) -> &'static str {
    match t {
        Target::Root => "matlab.ui.Root",
        Target::Figure(_) => "matlab.ui.Figure",
        Target::Axes(f, a) if it.figures[f].axes[a].polar => "matlab.graphics.axis.PolarAxes",
        Target::Axes(..) => "matlab.graphics.axis.Axes",
        Target::Series(f, a, s) => series_class(&it.figures[f].axes[a].series[s]),
        Target::Part(_, _, Part::XAxis | Part::YAxis | Part::ZAxis | Part::YAxisRight) => "matlab.graphics.axis.decorator.NumericRuler",
        Target::Part(_, _, Part::Legend) => "matlab.graphics.illustration.Legend",
        Target::Part(..) => "matlab.graphics.primitive.Text",
        Target::FigObj(f, k) => it.figures[f].objects[k].kind.class(),
    }
}

pub fn is_gfx_class(name: &str) -> bool {
    name.starts_with("matlab.graphics.") || name.starts_with("matlab.ui.")
}

/// The short class name MATLAB uses in messages (`Line` for `...primitive.Line`).
fn short(class: &str) -> &str {
    match class {
        "matlab.ui.Root" => "Root",
        c => c.rsplit('.').next().unwrap_or(c),
    }
}

fn class_def(name: &str) -> Rc<ClassDef> {
    thread_local! {
        static DEFS: RefCell<HashMap<String, Rc<ClassDef>>> = RefCell::new(HashMap::new());
    }
    DEFS.with(|d| {
        if let Some(def) = d.borrow().get(name) {
            return def.clone();
        }
        let src = "classdef GraphicsHandle < handle\n  properties (Hidden)\n    GraphicsIds\n  end\nend\n";
        let ast = crate::class::parse_classdef(src).expect("built-in class source parses");
        let mut def = ClassDef::build(ast, &[]);
        def.name = name.to_string();
        let def = Rc::new(def);
        d.borrow_mut().insert(name.to_string(), def.clone());
        def
    })
}

/// A handle value for `ids` (a column; one id is a scalar handle).
pub fn handles(it: &Interpreter, ids: &[u64]) -> Value {
    let classes: Vec<&str> = ids.iter().map(|&id| resolve(it, id).map(|t| class_of(it, t)).unwrap_or("matlab.graphics.GraphicsPlaceholder")).collect();
    let class = match classes.first() {
        None => "matlab.graphics.GraphicsPlaceholder",
        Some(c) if classes.iter().all(|x| x == c) => c,
        // Patches with text (pie) share the primitive base class.
        _ if classes.iter().all(|c| c.starts_with("matlab.graphics.primitive.")) => "matlab.graphics.primitive.Data",
        _ => "matlab.graphics.Graphics",
    };
    let data: Vec<f64> = ids.iter().map(|&i| i as f64).collect();
    let m = if ids.is_empty() { Matrix::new(0, 0, Vec::new()) } else { Matrix::new(data.len(), 1, data) };
    Value::Object(Object { class: class_def(class), props: Rc::new(RefCell::new(vec![Value::Num(m)])) })
}

/// A handle value for `ids` as a row.
pub fn handles_row(it: &Interpreter, ids: &[u64]) -> Value {
    let v = handles(it, ids);
    if let Value::Object(o) = &v {
        if let Value::Num(m) = &mut o.props.borrow_mut()[0] {
            if m.numel() > 0 {
                m.dims = vec![1, ids.len()];
            }
        }
    }
    v
}

fn is_data_class(c: &str) -> bool {
    c.starts_with("matlab.graphics.primitive.") || c.starts_with("matlab.graphics.chart.primitive.")
}

fn colormap_value(map: Vec<[f64; 3]>) -> Value {
    let n = map.len();
    let mut data = vec![0.0; n * 3];
    for (i, c) in map.iter().enumerate() {
        for k in 0..3 {
            data[k * n + i] = c[k];
        }
    }
    Value::Num(Matrix::new(n, 3, data))
}

/// `[h1 h2 ...]` of graphics handles: an array of their objects, of their common class.
pub fn concat(items: Vec<Value>, dim: usize) -> R<Value> {
    let mut classes: Vec<String> = Vec::new();
    let mut parts = Vec::new();
    for v in &items {
        let Value::Object(o) = v else { unreachable!() };
        let ids = match o.props.borrow().first() {
            Some(Value::Num(m)) => m.clone(),
            _ => Matrix::new(0, 0, Vec::new()),
        };
        if ids.data.is_empty() {
            continue;
        }
        classes.push(o.class.name.clone());
        parts.push((vec![ids.rows(), ids.cols()], ids.data));
    }
    let class = match classes.first() {
        None => "matlab.graphics.GraphicsPlaceholder".to_string(),
        Some(c) if classes.iter().all(|x| x == c) => c.clone(),
        // The nearest common base class of plotted objects and axes.
        _ if classes.iter().all(|c| is_data_class(c)) => "matlab.graphics.primitive.Data".to_string(),
        _ if classes.iter().all(|c| is_data_class(c) || c.starts_with("matlab.graphics.axis.")) => "matlab.graphics.primitive.world.Group".to_string(),
        _ => "matlab.graphics.Graphics".to_string(),
    };
    let m = if parts.is_empty() {
        Matrix::new(0, 0, Vec::new())
    } else {
        let (d, data) = crate::nd::concat(parts, dim)?;
        if d.len() > 2 {
            return rt("Arrays of graphics objects with more than two dimensions are not supported.");
        }
        Matrix::new(d[0], d[1], data)
    };
    Ok(Value::Object(Object { class: class_def(&class), props: Rc::new(RefCell::new(vec![Value::Num(m)])) }))
}

/// `h(k) = obj` into an array of graphics objects; new places hold placeholders, and an
/// array that mixes classes is of class matlab.graphics.Graphics.
pub fn assign_handles(base: &Value, idx: &[crate::nd::IndexArg], rhs: &Value) -> R<Value> {
    let ids = |v: &Value| -> (Matrix, String) {
        let Value::Object(o) = v else { unreachable!() };
        let m = match o.props.borrow().first() {
            Some(Value::Num(m)) => m.clone(),
            _ => Matrix::new(0, 0, Vec::new()),
        };
        (m, o.class.name.clone())
    };
    let (mut b, bc) = ids(base);
    let (r, rc) = ids(rhs);
    let mut dims = b.dims.clone();
    crate::nd::assign(&mut dims, &mut b.data, idx, &r.dims, r.data.clone(), PLACEHOLDER as f64)?;
    if dims.len() > 2 {
        return rt("Arrays of graphics objects with more than two dimensions are not supported.");
    }
    let class = if bc == rc { bc } else { "matlab.graphics.Graphics".to_string() };
    let m = Matrix::from_dims(dims, b.data);
    Ok(Value::Object(Object { class: class_def(&class), props: Rc::new(RefCell::new(vec![Value::Num(m)])) }))
}

pub fn handle(it: &Interpreter, id: u64) -> Value {
    handles(it, &[id])
}

/// The handle numbers of a graphics handle value.
pub fn ids_of(v: &Value) -> Option<Vec<u64>> {
    match v {
        Value::Object(o) if is_gfx_class(&o.class.name) => match o.props.borrow().first() {
            Some(Value::Num(m)) => Some(m.data.iter().map(|&x| x as u64).collect()),
            _ => Some(Vec::new()),
        },
        _ => None,
    }
}

pub fn is_gfx(v: &Value) -> bool {
    ids_of(v).is_some()
}

/// Handle numbers of an argument that is a graphics handle or a figure number.
fn target_ids(it: &Interpreter, v: &Value) -> R<Vec<u64>> {
    if let Some(ids) = ids_of(v) {
        return Ok(ids);
    }
    match v {
        Value::Num(m) if m.kind != Kind::Char => {
            let ids: Vec<u64> = m.data.iter().map(|&x| x as u64).collect();
            if ids.iter().zip(&m.data).all(|(&i, &x)| i as f64 == x && resolve(it, i).is_some()) {
                Ok(ids)
            } else {
                rt("Invalid or deleted object.")
            }
        }
        _ => rt("Invalid or deleted object."),
    }
}

/// The figure number of a figure handle (or the number itself).
pub fn figure_number(it: &Interpreter, v: &Value) -> R<Option<u32>> {
    if let Some(ids) = ids_of(v) {
        return match ids.first().and_then(|&id| resolve(it, id)) {
            Some(Target::Figure(f)) => Ok(Some(it.figures[f].id)),
            Some(Target::Axes(f, _) | Target::Series(f, _, _) | Target::Part(f, _, _)) => Ok(Some(it.figures[f].id)),
            _ => rt("Invalid or deleted object."),
        };
    }
    if v.is_text() {
        return Ok(None);
    }
    Ok(Some(v.scalar()? as u32))
}

// ---- property table ------------------------------------------------------------------------

struct PropTable {
    /// Class → property names (in MATLAB's order) with their default literals.
    by_class: HashMap<&'static str, Vec<(&'static str, &'static str)>>,
}

fn table() -> &'static PropTable {
    static T: std::sync::OnceLock<PropTable> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut by_class: HashMap<&'static str, Vec<(&'static str, &'static str)>> = HashMap::new();
        for line in include_str!("gfx_props.tsv").lines() {
            let mut cols = line.splitn(3, '\t');
            if let (Some(c), Some(n), Some(d)) = (cols.next(), cols.next(), cols.next()) {
                by_class.entry(c).or_default().push((n, d.trim_end_matches('\r')));
            }
        }
        // `line` makes a primitive line; plot's chart line shares its properties.
        PropTable { by_class }
    })
}

fn props_of(class: &str) -> &'static [(&'static str, &'static str)] {
    let t = table();
    // surf makes a chart Surface, which shares the primitive surface's properties.
    t.by_class.get(class).or_else(|| t.by_class.get(class.replace(".chart.primitive.", ".primitive.").as_str())).map(|v| v.as_slice()).unwrap_or(&[])
}

/// MATLAB's spelling of a property name given in any case, or `None`.
fn canonical(class: &str, name: &str) -> Option<String> {
    if let Some((n, _)) = props_of(class).iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
        return Some(n.to_string());
    }
    // Root defaults (`set(groot, 'defaultLineLineWidth', 2)`).
    let lower = name.to_ascii_lowercase();
    if class == "matlab.ui.Root" && (lower.starts_with("default") || lower.starts_with("factory")) {
        return Some(name.to_string());
    }
    // Hidden properties: readable and settable, but not listed by `get`.
    if class == "matlab.graphics.axis.decorator.NumericRuler" && lower == "secondarylabel" {
        return Some("SecondaryLabel".into());
    }
    if class == "matlab.ui.Root" && (lower == "diary" || lower == "diaryfile") {
        return Some(if lower == "diary" { "Diary" } else { "DiaryFile" }.into());
    }
    None
}

fn default_value(it: &mut Interpreter, class: &str, name: &str) -> Value {
    let lit = props_of(class).iter().find(|(n, _)| *n == name).map(|p| p.1).unwrap_or("[]");
    if lit.starts_with('<') {
        return Value::empty();
    }
    let lit = lit.replace("zeros(1,0)", "zeros(1, 0)");
    it.eval_text(&lit, 1).ok().and_then(|v| v.into_iter().next()).unwrap_or_else(Value::empty)
}

// ---- value conversions ---------------------------------------------------------------------

/// MATLAB's default color order.
pub const COLOR_ORDER: [[f64; 3]; 7] = [
    [0.0, 0.4470, 0.7410],
    [0.8500, 0.3250, 0.0980],
    [0.9290, 0.6940, 0.1250],
    [0.4940, 0.1840, 0.5560],
    [0.4660, 0.6740, 0.1880],
    [0.3010, 0.7450, 0.9330],
    [0.6350, 0.0780, 0.1840],
];

const NAMED: [(&str, char, [f64; 3]); 8] = [
    ("red", 'r', [1.0, 0.0, 0.0]),
    ("green", 'g', [0.0, 1.0, 0.0]),
    ("blue", 'b', [0.0, 0.0, 1.0]),
    ("cyan", 'c', [0.0, 1.0, 1.0]),
    ("magenta", 'm', [1.0, 0.0, 1.0]),
    ("yellow", 'y', [1.0, 1.0, 0.0]),
    ("black", 'k', [0.0, 0.0, 0.0]),
    ("white", 'w', [1.0, 1.0, 1.0]),
];

fn rgb_value(c: [f64; 3]) -> Value {
    Value::Num(Matrix::row(c.to_vec()))
}

/// The MATLAB RGB of a render color: named colors map back to their pure values.
fn render_to_rgb(c: [u8; 3]) -> [f64; 3] {
    for (name, _, rgb) in NAMED {
        if graphics::named_color(name) == Some(c) {
            return rgb;
        }
    }
    [c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0]
}

/// A color argument: a name, a short name, '#rrggbb', an RGB triplet or 'none'.
/// Returns the render color and the MATLAB RGB value.
pub(crate) fn color_arg(v: &Value) -> R<Option<([u8; 3], [f64; 3])>> {
    if v.is_text() || matches!(v, Value::Str(_)) {
        let t = v.text()?.to_lowercase();
        if t == "none" {
            return Ok(None);
        }
        if let Some(hex) = t.strip_prefix('#') {
            // '#RGB' is short for '#RRGGBB'.
            let hex: String = if hex.len() == 3 { hex.chars().flat_map(|c| [c, c]).collect() } else { hex.to_string() };
            if hex.len() != 6 {
                return rt("Invalid color value.");
            }
            let n = u32::from_str_radix(&hex, 16).map_err(|_| LangError::new("Invalid color value.", 0))?;
            let rgb = [((n >> 16) & 255) as f64 / 255.0, ((n >> 8) & 255) as f64 / 255.0, (n & 255) as f64 / 255.0];
            return Ok(Some((to_u8(rgb), rgb)));
        }
        for (name, ch, rgb) in NAMED {
            if t == name || (t.len() == 1 && t.starts_with(ch)) {
                return Ok(Some((graphics::named_color(name).unwrap(), rgb)));
            }
        }
        return rt(format!("'{t}' is not a valid value. Use one of these values: 'red' | 'green' | 'blue' | 'cyan' | 'magenta' | 'yellow' | 'black' | 'white' | 'none'."));
    }
    let m = v.to_matrix()?;
    if m.numel() != 3 {
        return rt("Color value must be a 3 element vector.");
    }
    let k = if m.data.iter().all(|&c| c <= 1.0) { 1.0 } else { 255.0 };
    let rgb = [m.data[0] / k, m.data[1] / k, m.data[2] / k];
    if rgb.iter().any(|c| !(0.0..=1.0).contains(c)) {
        return rt("Color value must be a 3 element vector.");
    }
    Ok(Some((to_u8(rgb), rgb)))
}

fn to_u8(c: [f64; 3]) -> [u8; 3] {
    [(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8]
}

fn on_off(v: &Value) -> R<bool> {
    if v.is_text() || matches!(v, Value::Str(_)) {
        return match v.text()?.to_lowercase().as_str() {
            "on" => Ok(true),
            "off" => Ok(false),
            other => rt(format!("'{other}' is not a valid value. Use one of these values: 'on' | 'off'.")),
        };
    }
    Ok(v.scalar()? != 0.0)
}

fn on_off_value(b: bool) -> Value {
    Value::chars(if b { "on" } else { "off" })
}

fn style_name(s: LineStyle) -> &'static str {
    match s {
        LineStyle::Solid => "-",
        LineStyle::Dashed => "--",
        LineStyle::Dotted => ":",
        LineStyle::DashDot => "-.",
        LineStyle::None => "none",
    }
}

pub(crate) fn marker_name(m: Option<Marker>) -> &'static str {
    match m {
        None => "none",
        Some(Marker::Circle) => "o",
        Some(Marker::Square) => "square",
        Some(Marker::Diamond) => "diamond",
        Some(Marker::Plus) => "+",
        Some(Marker::Cross) => "x",
        Some(Marker::Star) => "*",
        Some(Marker::Point) => ".",
        Some(Marker::TriangleUp) => "^",
        Some(Marker::TriangleDown) => "v",
    }
}

pub(crate) fn parse_marker(t: &str) -> Option<Marker> {
    match t.to_lowercase().as_str() {
        "square" => Some(Marker::Square),
        "diamond" => Some(Marker::Diamond),
        "none" => None,
        other => graphics::parse_line_spec(other).and_then(|p| p.2),
    }
}

fn row(v: &[f64]) -> Value {
    Value::Num(Matrix::row(v.to_vec()))
}

fn texts(v: &Value) -> R<Vec<String>> {
    Ok(match v {
        Value::Cell(c) => c.data.iter().map(|x| if x.numel() == 0 { Ok(String::new()) } else { x.text() }).collect::<R<_>>()?,
        Value::Str(s) => s.data.iter().map(|x| x.clone().unwrap_or_default()).collect(),
        Value::Num(m) if m.kind == Kind::Char => (0..m.rows()).map(|r| (0..m.cols()).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or(' ')).collect::<String>().trim_end().to_string()).collect(),
        Value::Num(m) => m.data.iter().map(|&x| crate::display::fmt_g(x, 5)).collect(),
        other => vec![other.text()?],
    })
}

/// Tick labels as MATLAB returns them: a cell column, or the char matrix they were set as.
fn label_value(ticks: &graphics::Ticks, items: &[String]) -> Value {
    if !ticks.as_char {
        return cell_col(items);
    }
    let w = items.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let rows: Vec<Vec<char>> = items.iter().map(|s| s.chars().chain(std::iter::repeat(' ')).take(w).collect()).collect();
    let data: Vec<f64> = (0..w).flat_map(|j| rows.iter().map(move |r| r[j] as u32 as f64)).collect();
    let mut m = Matrix::new(items.len(), w, data);
    m.kind = Kind::Char;
    Value::Num(m)
}

fn cell_col(items: &[String]) -> Value {
    Value::Cell(CellArray::col(items.iter().map(|s| Value::chars(s)).collect()))
}

// ---- get / set -------------------------------------------------------------------------------

pub(crate) fn stored(it: &Interpreter, id: u64, name: &str) -> Option<Value> {
    it.gfx_store.get(&id).and_then(|m| m.iter().find(|(n, _)| n == name)).map(|(_, v)| v.clone())
}

pub(crate) fn store(it: &mut Interpreter, id: u64, name: &str, v: Value) {
    let m = it.gfx_store.entry(id).or_default();
    match m.iter_mut().find(|(n, _)| n == name) {
        Some(e) => e.1 = v,
        None => m.push((name.to_string(), v)),
    }
}

/// Ticks of an axis: set values, or the automatic ones within the limits.
/// The z limits of axes: `zlim`, or the data range rounded out to whole tick steps.
fn z_limits(ax: &graphics::Axes) -> (f64, f64) {
    if let Some(l) = ax.zlim {
        return l;
    }
    let (lo, hi) = ax.zbounds();
    let step = graphics::nice_step(hi - lo, 5.0);
    ((lo / step + 1e-9).floor() * step, (hi / step - 1e-9).ceil() * step)
}

pub(crate) fn tick_values(ticks: &graphics::Ticks, lo: f64, hi: f64) -> Vec<f64> {
    if let Some(v) = &ticks.values {
        return v.clone();
    }
    let step = graphics::nice_step(hi - lo, 5.0);
    let a = (lo / step - 1e-9).ceil() * step;
    let n = ((hi - a) / step + 1e-9).floor().max(0.0) as usize;
    (0..=n.min(40)).map(|k| {
        let v = a + k as f64 * step;
        if v.abs() < step * 1e-9 { 0.0 } else { v }
    }).collect()
}

fn tick_labels(ticks: &graphics::Ticks, values: &[f64]) -> Vec<String> {
    if let Some(l) = &ticks.labels {
        return l.clone();
    }
    values.iter().map(|&v| format_tick(ticks.format.as_deref(), v)).collect()
}

/// A tick label with a `xtickformat`-style format (MATLAB's `%g` by default).
pub fn format_tick(format: Option<&str>, v: f64) -> String {
    match format {
        None | Some("%g") => crate::display::fmt_g(v, 5),
        Some("usd") => format!("${}", crate::format::sprintf("%.2f", &[Value::num(v)]).unwrap_or_default()),
        Some("eur") => format!("€{}", crate::format::sprintf("%.2f", &[Value::num(v)]).unwrap_or_default()),
        Some("gbp") => format!("£{}", crate::format::sprintf("%.2f", &[Value::num(v)]).unwrap_or_default()),
        Some("jpy") => format!("¥{}", crate::format::sprintf("%.0f", &[Value::num(v)]).unwrap_or_default()),
        Some("percentage") => format!("{}%", crate::display::fmt_g(v, 5)),
        Some("degrees") => format!("{}°", crate::display::fmt_g(v, 5)),
        Some(f) => crate::format::sprintf(f, &[Value::num(v)]).unwrap_or_else(|_| crate::display::fmt_g(v, 5)),
    }
}

/// Property `name` (MATLAB's spelling) of handle `id`.
pub(crate) fn get_prop(it: &mut Interpreter, id: u64, name: &str) -> R<Value> {
    let t = resolve(it, id).ok_or_else(|| LangError::new("Invalid or deleted object.", 0))?;
    let class = class_of(it, t);
    // Charts keep their own properties; their drawn parts are rebuilt from them.
    if crate::charts::is_chart(class) {
        if let Some(v) = stored(it, id, name) {
            return Ok(v);
        }
        if name == "Parent" {
            if let Some(p) = crate::charts::special_parent(it, id, class) {
                return Ok(handle(it, p));
            }
        }
        if let (Target::Series(f, a, _), "Parent") = (t, name) {
            let parent = if crate::charts::in_axes(class) { axes_id(it, f, a) } else { it.figures[f].id as u64 };
            return Ok(handle(it, parent));
        }
        return Ok(default_value(it, class, name));
    }
    // Plotted objects hold their data tips.
    if let (Target::Series(..), "Children") = (t, name) {
        return Ok(handles(it, &crate::charts::datatips_of(it, id)));
    }
    if let Some(v) = computed(it, id, t, name)? {
        return Ok(v);
    }
    if let Some(v) = stored(it, id, name) {
        return Ok(v);
    }
    Ok(default_value(it, class, name))
}

/// Properties read from the figure model; `None` for the ones that are stored or default.
fn computed(it: &mut Interpreter, id: u64, t: Target, name: &str) -> R<Option<Value>> {
    Ok(Some(match t {
        Target::Root => match name {
            "CurrentFigure" => match it.current_figure {
                Some(f) => handle(it, it.figures[f].id as u64),
                None => handles(it, &[]),
            },
            "Children" => {
                let ids: Vec<u64> = it.figures.iter().rev().map(|f| f.id as u64).collect();
                handles(it, &ids)
            }
            "Type" => Value::chars("root"),
            "Diary" => Value::chars(if it.diary.0 { "on" } else { "off" }),
            "DiaryFile" => Value::chars(&it.diary.1),
            _ => return Ok(None),
        },
        Target::Figure(f) => match name {
            "Number" => Value::num(it.figures[f].id as f64),
            "Colormap" => colormap_value(it.figures[f].colormap.clone().unwrap_or_else(|| graphics::colormap("parula", 256).unwrap_or_default())),
            "Children" => {
                let n = it.figures[f].axes.len();
                let ids: Vec<u64> = (0..n).rev().map(|a| axes_id(it, f, a)).collect();
                handles(it, &ids)
            }
            "CurrentAxes" => {
                let a = it.figures[f].current_axes;
                let id = axes_id(it, f, a);
                handle(it, id)
            }
            "Type" => Value::chars("figure"),
            "Parent" => handle(it, ROOT),
            _ => return Ok(None),
        },
        Target::Axes(f, a) if it.figures[f].axes[a].polar && polar_prop(name) => {
            let ax = &it.figures[f].axes[a];
            let ((t0, t1), (r0, r1)) = ax.polar_limits();
            match name {
                "ThetaLim" => row(&[t0, t1]),
                "RLim" => row(&[r0, r1]),
                "ThetaLimMode" => Value::chars(if ax.xlim.is_some() { "manual" } else { "auto" }),
                "RLimMode" => Value::chars(if ax.ylim.is_some() { "manual" } else { "auto" }),
                "ThetaTick" => row(&ax.polar_theta_ticks()),
                "RTick" => row(&ax.polar_r_ticks()),
                "ThetaTickLabel" => cell_col(&ax.polar_theta_labels()),
                "RTickLabel" => cell_col(&ax.polar_r_labels()),
                "ThetaTickMode" => Value::chars(if ax.xticks.values.is_some() { "manual" } else { "auto" }),
                "RTickMode" => Value::chars(if ax.yticks.values.is_some() { "manual" } else { "auto" }),
                "ThetaDir" => Value::chars(if ax.theta_cw { "clockwise" } else { "counterclockwise" }),
                "ThetaZeroLocation" => Value::chars(match ax.theta_zero as i64 {
                    90 => "top",
                    180 => "left",
                    270 => "bottom",
                    _ => "right",
                }),
                "RAxisLocation" => Value::num(ax.r_axis_angle.unwrap_or(80.0)),
                "RTickLabelRotation" => Value::num(ax.yticks.angle as f64),
                _ => Value::chars("polaraxes"),
            }
        }
        Target::Axes(f, a) => {
            let ax = &it.figures[f].axes[a];
            let ((x0, x1), (y0, y1)) = ax.limits();
            match name {
                "XLim" => row(&[x0, x1]),
                "YLim" => row(&[y0, y1]),
                // Auto z limits: the data range widened to whole tick steps.
                "ZLim" => {
                    let (z0, z1) = z_limits(ax);
                    row(&[z0, z1])
                }
                "ZTick" if stored(it, id, name).is_none() => {
                    let (z0, z1) = z_limits(ax);
                    row(&tick_values(&graphics::Ticks::default(), z0, z1))
                }
                // Labels read back as a column, however they were set.
                "ZTickLabel" if matches!(stored(it, id, name), Some(Value::Cell(_))) => {
                    let Some(Value::Cell(c)) = stored(it, id, name) else { unreachable!() };
                    let n = c.data.len();
                    Value::Cell(CellArray::from_dims(vec![n, 1], c.data))
                }
                "ZTickLabel" if stored(it, id, name).is_none() => {
                    let (z0, z1) = z_limits(ax);
                    let values = match stored(it, id, "ZTick") {
                        Some(v) => v.to_matrix()?.data,
                        None => tick_values(&graphics::Ticks::default(), z0, z1),
                    };
                    let t = graphics::Ticks::default();
                    label_value(&t, &tick_labels(&t, &values))
                }
                "XLimMode" => Value::chars(if ax.xlim.is_some() { "manual" } else { "auto" }),
                "YLimMode" => Value::chars(if ax.ylim.is_some() { "manual" } else { "auto" }),
                "XScale" => Value::chars(if ax.xlog { "log" } else { "linear" }),
                "YScale" => Value::chars(if ax.ylog { "log" } else { "linear" }),
                "YDir" => Value::chars(if ax.y_reverse { "reverse" } else { "normal" }),
                "XGrid" | "YGrid" => on_off_value(ax.grid),
                "XTick" => row(&tick_values(&ax.xticks, x0, x1)),
                "YTick" => row(&tick_values(&ax.yticks, y0, y1)),
                "XTickLabel" => label_value(&ax.xticks, &tick_labels(&ax.xticks, &tick_values(&ax.xticks, x0, x1))),
                "YTickLabel" => label_value(&ax.yticks, &tick_labels(&ax.yticks, &tick_values(&ax.yticks, y0, y1))),
                "XTickMode" => Value::chars(if ax.xticks.values.is_some() { "manual" } else { "auto" }),
                "YTickMode" => Value::chars(if ax.yticks.values.is_some() { "manual" } else { "auto" }),
                "XTickLabelRotation" => Value::num(ax.xticks.angle as f64),
                "YTickLabelRotation" => Value::num(ax.yticks.angle as f64),
                "FontSize" => Value::num(ax.font_size.unwrap_or(10.0) as f64),
                "NextPlot" => Value::chars(if ax.hold { "add" } else { "replace" }),
                "DataAspectRatio" => row(&ax.data_aspect.unwrap_or([1.0, 1.0, 1.0])),
                "DataAspectRatioMode" => Value::chars(if ax.data_aspect.is_some() { "manual" } else { "auto" }),
                "Visible" => on_off_value(ax.visible_axes.unwrap_or(true)),
                "Type" => Value::chars("axes"),
                "View" => {
                    let (az, el) = ax.view.unwrap_or((0.0, 90.0));
                    row(&[az, el])
                }
                "Position" | "InnerPosition" if stored(it, id, name).is_none() => row(&[0.13, 0.11, 0.775, 0.815]),
                "OuterPosition" if stored(it, id, name).is_none() => row(&[0.0, 0.0, 1.0, 1.0]),
                "YAxisLocation" if ax.yy.is_some() => Value::chars(if ax.yy.as_ref().unwrap().right_active { "right" } else { "left" }),
                "YAxis" if ax.yy.is_some() => {
                    let base = axes_id(it, f, a);
                    let ids = [base + Part::YAxis as u64, base + Part::YAxisRight as u64];
                    let v = handles(it, &ids);
                    if let Value::Object(o) = &v {
                        o.props.borrow_mut()[0] = Value::Num(Matrix::col(ids.iter().map(|&i| i as f64).collect()));
                    }
                    v
                }
                "Title" | "XLabel" | "YLabel" | "ZLabel" | "Subtitle" | "XAxis" | "YAxis" | "ZAxis" | "Legend" => {
                    let part = match name {
                        "Title" => Part::Title,
                        "XLabel" => Part::XLabel,
                        "YLabel" => Part::YLabel,
                        "ZLabel" => Part::ZLabel,
                        "Subtitle" => Part::Subtitle,
                        "XAxis" => Part::XAxis,
                        "YAxis" => Part::YAxis,
                        "ZAxis" => Part::ZAxis,
                        _ => Part::Legend,
                    };
                    if part == Part::Legend && ax.legend.is_none() {
                        return Ok(Some(handles(it, &[])));
                    }
                    let base = axes_id(it, f, a);
                    handle(it, base + part as u64)
                }
                "Children" => {
                    // yyaxis axes list the active side's objects.
                    let side = ax.yy.as_ref().map(|yy| yy.right_active);
                    // Chart parts and data tips are not children of the axes.
                    let hidden = crate::charts::hidden_in_axes(it, f, a);
                    let ids: Vec<u64> = ax.series.iter().rev().filter(|s| side.map_or(true, |r| s.right == r) && !hidden.contains(&s.id)).map(|s| s.id).collect();
                    handles(it, &ids)
                }
                "Parent" => handle(it, it.figures[f].id as u64),
                _ => return Ok(None),
            }
        }
        Target::Series(f, a, si) => {
            let s = &it.figures[f].axes[a].series[si];
            match name {
                "XData" | "YData" | "ZData" | "CData" if s.kind == SeriesKind::Surface => {
                    let v = match name {
                        "XData" => &s.x,
                        "YData" => &s.y,
                        "ZData" => &s.z,
                        _ => &s.c,
                    };
                    Value::Num(Matrix::new(s.grid.0, s.grid.1, v.clone()))
                }
                "CDataMapping" if s.kind == SeriesKind::Image => Value::chars(if s.direct { "direct" } else { "scaled" }),
                "CData" if s.kind == SeriesKind::Image => {
                    let dims = if s.channels == 3 { vec![s.grid.0, s.grid.1, 3] } else { vec![s.grid.0, s.grid.1] };
                    Value::Num(Matrix::from_dims(dims, s.c.clone()))
                }
                "LevelList" if s.kind == SeriesKind::Contour => {
                    let mut l: Vec<f64> = s.c.iter().chain(&s.w).copied().filter(|v| v.is_finite()).collect();
                    l.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    l.dedup();
                    row(&l)
                }
                "XData" if s.kind == SeriesKind::BarH => row(&s.x),
                "XData" => row(&s.x),
                "YData" => row(&s.y),
                "ZData" => row(&s.z),
                "Position" if s.kind == SeriesKind::Rectangle => row(&s.u),
                "Curvature" if s.kind == SeriesKind::Rectangle => row(&s.v),
                "Position" if s.kind == SeriesKind::Light => row(&[s.x[0], s.y[0], s.z.first().copied().unwrap_or(0.0)]),
                "Style" if s.kind == SeriesKind::Light => Value::chars(&s.label),
                "CData" if s.kind == SeriesKind::Scatter && stored(it, id, name).is_none() => rgb_value(match s.color {
                    Some(c) => render_to_rgb(c),
                    None => COLOR_ORDER[si % COLOR_ORDER.len()],
                }),
                "Color" | "FaceColor" if !matches!(s.kind, SeriesKind::Text | SeriesKind::Surface | SeriesKind::Image | SeriesKind::Contour | SeriesKind::Rectangle | SeriesKind::Light) => match stored(it, id, name) {
                    Some(v) => v,
                    None => rgb_value(match s.color {
                        Some(c) => render_to_rgb(c),
                        None if s.kind == SeriesKind::ConstantLine => [0.15, 0.15, 0.15],
                        None => COLOR_ORDER[si % COLOR_ORDER.len()],
                    }),
                },
                "LineStyle" => Value::chars(style_name(s.line_style)),
                "LineWidth" => Value::num(s.line_width as f64),
                "Marker" => Value::chars(marker_name(s.marker)),
                "MarkerSize" => Value::num(s.marker_size as f64),
                "DisplayName" => Value::chars(s.name.as_deref().unwrap_or("")),
                "Visible" => on_off_value(s.visible),
                "String" if s.kind == SeriesKind::Text => Value::chars(&s.label),
                "Label" if s.kind == SeriesKind::ConstantLine => Value::chars(&s.label),
                "Position" if s.kind == SeriesKind::Text => row(&[s.x[0], s.y[0], 0.0]),
                "Value" => match s.kind {
                    SeriesKind::ConstantLine if s.horizontal => Value::num(s.y[0]),
                    SeriesKind::ConstantLine => Value::num(s.x[0]),
                    SeriesKind::Region if s.horizontal => row(&s.y),
                    SeriesKind::Region => row(&s.x),
                    _ => return Ok(None),
                },
                "InterceptAxis" => Value::chars(if s.horizontal { "y" } else { "x" }),
                "FaceAlpha" => Value::num(s.face_alpha as f64),
                "Horizontal" => on_off_value(s.kind == SeriesKind::BarH),
                "FontSize" if s.font_size.is_some() => Value::num(s.font_size.unwrap() as f64),
                "YNegativeDelta" | "YPositiveDelta" => match &s.err {
                    Some((lo, hi)) => row(if name == "YNegativeDelta" { lo } else { hi }),
                    None => row(&[]),
                },
                "Type" => Value::chars(match s.kind {
                    SeriesKind::Line => "line",
                    SeriesKind::Scatter => "scatter",
                    SeriesKind::Bar | SeriesKind::BarH => "bar",
                    SeriesKind::Stairs => "stair",
                    SeriesKind::Area => "area",
                    SeriesKind::Stem => "stem",
                    SeriesKind::ErrorBar => "errorbar",
                    SeriesKind::Patch => "patch",
                    SeriesKind::Text => "text",
                    SeriesKind::ConstantLine => "constantline",
                    SeriesKind::Region => "constantregion",
                    SeriesKind::Surface => "surface",
                    SeriesKind::Image => "image",
                    SeriesKind::Contour => "contour",
                    SeriesKind::Quiver => "quiver",
                    SeriesKind::Rectangle => "rectangle",
                    SeriesKind::Light => "light",
                }),
                "Parent" => {
                    let id = axes_id(it, f, a);
                    handle(it, id)
                }
                _ => return Ok(None),
            }
        }
        Target::Part(f, a, p) => {
            let ax = &it.figures[f].axes[a];
            match (p, name) {
                (Part::Title, "String") => Value::chars(ax.title.split('\n').next().unwrap_or("")),
                (Part::Subtitle, "String") => Value::chars(ax.title.split_once('\n').map(|x| x.1).unwrap_or("")),
                (Part::XLabel, "String") => Value::chars(&ax.xlabel),
                (Part::YLabel, "String") => Value::chars(&ax.ylabel),
                (Part::ZLabel, "String") => Value::chars(&ax.zlabel),
                (Part::XSecondary, "String") => Value::chars(&ax.secondary[0]),
                (Part::YSecondary, "String") => Value::chars(&ax.secondary[1]),
                (Part::ZSecondary, "String") => Value::chars(&ax.secondary[2]),
                (Part::XSecondary | Part::YSecondary | Part::ZSecondary, "Type") => Value::chars("text"),
                (Part::Legend, "String") => {
                    let names = ax.legend.clone().unwrap_or_default();
                    Value::Cell(CellArray::row(names.iter().map(|n| Value::chars(n)).collect()))
                }
                (Part::Title | Part::Subtitle | Part::XLabel | Part::YLabel | Part::ZLabel, "Type") => Value::chars("text"),
                (Part::Title | Part::Subtitle | Part::XLabel | Part::YLabel | Part::ZLabel, "HorizontalAlignment") if stored(it, id, name).is_none() => Value::chars("center"),
                (Part::Title, "FontWeight") => match stored(it, id, name) {
                    Some(v) => v,
                    None => Value::chars("bold"),
                },
                (Part::Title | Part::Subtitle | Part::XLabel | Part::YLabel | Part::ZLabel, "FontSize") => match stored(it, id, name) {
                    Some(v) => v,
                    None => Value::num(ax.font_size.unwrap_or(10.0) as f64 * if p == Part::Title { 1.1 } else { 1.1 }),
                },
                (Part::YLabelOther, "String") => Value::chars(ax.yy.as_ref().map(|yy| yy.other.ylabel.as_str()).unwrap_or("")),
                (Part::YLabelOther, "Type") => Value::chars("text"),
                (Part::XAxis | Part::YAxis | Part::ZAxis | Part::YAxisRight, "SecondaryLabel") => {
                    let base = axes_id(it, f, a);
                    handle(it, base + secondary_of(p).unwrap().0 as u64)
                }
                (Part::XAxis | Part::YAxis | Part::YAxisRight, _) => {
                    let x = p == Part::XAxis;
                    // A side of yyaxis axes: its own limits, ticks, label and color.
                    let right = p == Part::YAxisRight;
                    let side = ax.yy.as_ref().filter(|_| !x).map(|yy| (yy.right_active == right, yy.colors[right as usize]));
                    let view = if side.is_some() { ax.side(right) } else { ax.clone() };
                    let ax = &view;
                    let ((x0, x1), (y0, y1)) = ax.limits();
                    let (lo, hi, ticks, log) = if x { (x0, x1, &ax.xticks, ax.xlog) } else { (y0, y1, &ax.yticks, ax.ylog) };
                    match name {
                        "Color" if side.is_some() => row(&side.unwrap().1),
                        "Limits" => row(&[lo, hi]),
                        "TickValues" => row(&tick_values(ticks, lo, hi)),
                        "TickLabels" => cell_col(&tick_labels(ticks, &tick_values(ticks, lo, hi))),
                        "TickLabelFormat" => Value::chars(ticks.format.as_deref().unwrap_or("%g")),
                        "TickLabelRotation" => Value::num(ticks.angle as f64),
                        "Scale" => Value::chars(if log { "log" } else { "linear" }),
                        "Label" => {
                            let base = axes_id(it, f, a);
                            let part = match side {
                                Some((false, _)) => Part::YLabelOther,
                                _ if x => Part::XLabel,
                                _ => Part::YLabel,
                            };
                            handle(it, base + part as u64)
                        }
                        "FontSize" => Value::num(ax.font_size.unwrap_or(10.0) as f64),
                        "Parent" => {
                            let id = axes_id(it, f, a);
                            handle(it, id)
                        }
                        _ => return Ok(None),
                    }
                }
                (_, "Parent") => {
                    let id = axes_id(it, f, a);
                    handle(it, id)
                }
                _ => return Ok(None),
            }
        }
        Target::FigObj(f, k) => {
            let o = &it.figures[f].objects[k];
            let line = o.kind.is_line();
            match name {
                "X" if line => row(&o.x),
                "Y" if line => row(&o.y),
                "Position" if line => row(&[o.x[0], o.y[0], o.x[1] - o.x[0], o.y[1] - o.y[0]]),
                "Position" if o.kind != graphics::FigObjKind::Layout => row(&o.pos),
                "String" if matches!(o.kind, graphics::FigObjKind::TextBox | graphics::FigObjKind::TextArrow) => {
                    Value::Cell(CellArray::row(vec![Value::chars(&o.text)]))
                }
                "LineWidth" => Value::num(o.line_width as f64),
                "FontSize" => Value::num(o.font_size as f64),
                "Visible" => on_off_value(o.visible),
                "GridSize" if o.kind == graphics::FigObjKind::Layout => {
                    let g = it.figures[f].grid;
                    row(&[g.0 as f64, g.1 as f64])
                }
                "TileArrangement" if o.kind == graphics::FigObjKind::Layout => Value::chars(if it.figures[f].flow { "flow" } else { "fixed" }),
                "Children" if o.kind == graphics::FigObjKind::Layout => {
                    let n = it.figures[f].axes.len();
                    let ids: Vec<u64> = (0..n).rev().map(|a| axes_id(it, f, a)).collect();
                    handles(it, &ids)
                }
                "Parent" => handle(it, it.figures[f].id as u64),
                _ => return Ok(None),
            }
        }
    }))
}

fn mark(it: &mut Interpreter, f: usize) {
    let id = it.figures[f].id;
    it.mark_figure_dirty(id);
}

fn limits_arg(v: &Value) -> R<Option<(f64, f64)>> {
    if v.is_text() {
        return Ok(None);
    }
    let m = v.to_matrix()?;
    if m.numel() != 2 || !(m.data[0] < m.data[1]) {
        return rt("Value must be a 1x2 vector of numeric type in which the second element is larger than the first and may be Inf.");
    }
    Ok(Some((m.data[0], m.data[1])))
}

/// Sets property `name` (MATLAB's spelling) of handle `id`.
pub(crate) fn set_prop(it: &mut Interpreter, id: u64, name: &str, v: Value) -> R<()> {
    if let Some(r) = histogram_set(it, id, name, &v) {
        return r;
    }
    let auto = v.text().map(|t| t == "auto").unwrap_or(false);
    let mut save: Option<Value> = None;
    let t = resolve(it, id).ok_or_else(|| LangError::new("Invalid or deleted object.", 0))?;
    if crate::charts::is_chart(class_of(it, t)) {
        if let Target::Series(f, ..) = t {
            mark(it, f);
        }
        return crate::charts::set(it, id, name, v);
    }
    let mut keep = true;
    match t {
        Target::Root | Target::Figure(_) => {
            if t == Target::Root && name.len() > 7 && name[..7].eq_ignore_ascii_case("default") {
                DEFAULTS_SET.with(|d| d.set(true));
            }
            if let (Target::Figure(f), "Name") = (t, name) {
                mark(it, f);
            }
        }
        Target::Axes(f, a) if it.figures[f].axes[a].polar && polar_prop(name) => {
            mark(it, f);
            let ax = &mut it.figures[f].axes[a];
            keep = false;
            match name {
                "ThetaLim" => ax.xlim = limits_arg(&v)?,
                "RLim" => ax.ylim = limits_arg(&v)?,
                "ThetaLimMode" if auto => ax.xlim = None,
                "RLimMode" if auto => ax.ylim = None,
                "ThetaTick" => ax.xticks.values = Some(v.to_matrix()?.data),
                "RTick" => ax.yticks.values = Some(v.to_matrix()?.data),
                "ThetaTickMode" if auto => ax.xticks.values = None,
                "RTickMode" if auto => ax.yticks.values = None,
                "ThetaTickLabel" => ax.xticks.labels = Some(texts(&v)?),
                "RTickLabel" => ax.yticks.labels = Some(texts(&v)?),
                "RTickLabelRotation" => ax.yticks.angle = v.scalar()? as f32,
                "ThetaDir" => ax.theta_cw = v.text()?.eq_ignore_ascii_case("clockwise"),
                "ThetaZeroLocation" => {
                    ax.theta_zero = match v.text()?.to_lowercase().as_str() {
                        "right" => 0.0,
                        "top" => 90.0,
                        "left" => 180.0,
                        "bottom" => 270.0,
                        _ => return rt("ThetaZeroLocation must be 'right', 'top', 'left', or 'bottom'."),
                    }
                }
                "RAxisLocation" => ax.r_axis_angle = Some(v.scalar()?),
                _ => keep = true,
            }
        }
        Target::Axes(f, a) => {
            mark(it, f);
            let ax = &mut it.figures[f].axes[a];
            keep = false;
            match name {
                "XLim" => ax.xlim = limits_arg(&v)?,
                "YLim" => ax.ylim = limits_arg(&v)?,
                "ZLim" => ax.zlim = limits_arg(&v)?,
                "XLimMode" if auto => ax.xlim = None,
                "YLimMode" if auto => ax.ylim = None,
                "XScale" => ax.xlog = v.text()?.eq_ignore_ascii_case("log"),
                "YScale" => ax.ylog = v.text()?.eq_ignore_ascii_case("log"),
                "YDir" => ax.y_reverse = v.text()?.eq_ignore_ascii_case("reverse"),
                "XGrid" | "YGrid" | "ZGrid" => ax.grid = on_off(&v)?,
                "XTick" => ax.xticks.values = Some(v.to_matrix()?.data),
                "YTick" => ax.yticks.values = Some(v.to_matrix()?.data),
                "XTickMode" if auto => ax.xticks.values = None,
                "YTickMode" if auto => ax.yticks.values = None,
                "XTickLabel" => {
                    ax.xticks.labels = Some(texts(&v)?);
                    ax.xticks.as_char = matches!(&v, Value::Num(m) if m.kind == Kind::Char);
                }
                "YTickLabel" => {
                    ax.yticks.labels = Some(texts(&v)?);
                    ax.yticks.as_char = matches!(&v, Value::Num(m) if m.kind == Kind::Char);
                }
                "XTickLabelMode" if auto => ax.xticks.labels = None,
                "YTickLabelMode" if auto => ax.yticks.labels = None,
                "XTickLabelRotation" => ax.xticks.angle = v.scalar()? as f32,
                "YTickLabelRotation" => ax.yticks.angle = v.scalar()? as f32,
                "FontSize" => ax.font_size = Some(v.scalar()? as f32),
                "ColorOrder" => {
                    let m = v.to_matrix()?;
                    if m.cols() != 3 || m.rows() == 0 {
                        return rt("Value must be an m-by-3 matrix of RGB triplets.");
                    }
                    ax.color_order = Some((0..m.rows()).map(|r| [m.get(r, 0), m.get(r, 1), m.get(r, 2)]).collect());
                    keep = true;
                }
                "NextPlot" => ax.hold = matches!(v.text()?.as_str(), "add" | "replacechildren"),
                "DataAspectRatio" => {
                    let m = v.to_matrix()?;
                    if m.numel() != 3 {
                        return rt("Value must be a 1x3 vector of positive numbers.");
                    }
                    ax.data_aspect = Some([m.data[0], m.data[1], m.data[2]]);
                }
                "DataAspectRatioMode" if auto => ax.data_aspect = None,
                "Visible" => ax.visible_axes = Some(on_off(&v)?),
                "View" => {
                    let m = v.to_matrix()?;
                    if m.numel() != 2 {
                        return rt("Value must be a 1x2 vector.");
                    }
                    ax.view = if m.data == [0.0, 90.0] { None } else { Some((m.data[0], m.data[1])) };
                }
                // Assigning a label's own handle (ax.XLabel.String = ... writes back XLabel).
                "Title" | "XLabel" | "YLabel" | "ZLabel" | "Subtitle" | "XAxis" | "YAxis" | "ZAxis" | "Legend" if is_gfx(&v) => {}
                "Children" | "Parent" | "Type" => {}
                _ => keep = true,
            }
        }
        Target::Series(f, a, si) => {
            mark(it, f);
            let s = &mut it.figures[f].axes[a].series[si];
            keep = false;
            match name {
                "CDataMapping" if s.kind == SeriesKind::Image => s.direct = v.text()?.eq_ignore_ascii_case("direct"),
                "Position" | "Curvature" if s.kind == SeriesKind::Rectangle => {
                    let m = v.to_matrix()?;
                    if name == "Position" {
                        if m.numel() != 4 {
                            return rt("Value must be a 4-element vector.");
                        }
                        s.u = m.data;
                    } else {
                        s.v = if m.numel() == 1 { vec![m.data[0], m.data[0]] } else { m.data };
                    }
                    let (x, y) = graphics::rect_polygon(&s.u, &s.v);
                    s.x = x;
                    s.y = y;
                }
                "FaceColor" if s.kind == SeriesKind::Rectangle => {
                    let c = color_arg(&v)?;
                    s.face_none = c.is_none();
                    s.color = c.map(|c| c.0);
                    keep = true;
                }
                "Position" if s.kind == SeriesKind::Light => {
                    let m = v.to_matrix()?;
                    if m.numel() != 3 {
                        return rt("Value must be a 1x3 vector.");
                    }
                    s.x = vec![m.data[0]];
                    s.y = vec![m.data[1]];
                    s.z = vec![m.data[2]];
                }
                "Style" if s.kind == SeriesKind::Light => s.label = v.text()?,
                "XData" => s.x = v.to_matrix()?.data,
                "YData" => s.y = v.to_matrix()?.data,
                "FaceColor" | "EdgeColor" if s.kind == SeriesKind::Surface => {
                    let mode = if v.is_text() { v.text()?.to_lowercase() } else { String::new() };
                    let face = name == "FaceColor";
                    match mode.as_str() {
                        "none" => {
                            if face { s.face_none = true } else { s.edge_color = None; s.edge_mapped = false }
                        }
                        "flat" | "interp" | "texturemap" => {
                            if face { s.face_none = false; s.face_mapped = true } else { s.edge_mapped = true }
                        }
                        _ => {
                            let c = color_arg(&v)?.map(|c| c.0);
                            if face { s.face_none = c.is_none(); s.face_mapped = false; s.color = c } else { s.edge_mapped = false; s.edge_color = c }
                        }
                    }
                    keep = true;
                }
                "ZData" => s.z = v.to_matrix()?.data,
                "CData" => s.c = v.to_matrix()?.data,
                // Markers colored by CData.
                "MarkerFaceColor" | "MarkerEdgeColor" if v.text().map(|t| t.eq_ignore_ascii_case("flat")).unwrap_or(false) => {
                    save = Some(Value::chars("flat"));
                    keep = true;
                }
                "Color" | "FaceColor" | "MarkerFaceColor" | "MarkerEdgeColor" if !(name.starts_with("Marker") && auto) => {
                    let c = color_arg(&v)?;
                    if name == "Color" || name == "FaceColor" || c.is_some() {
                        s.color = c.map(|c| c.0);
                    }
                    save = Some(c.map(|c| rgb_value(c.1)).unwrap_or_else(|| Value::chars("none")));
                    keep = true;
                }
                "EdgeColor" => {
                    let c = color_arg(&v)?;
                    s.edge_color = c.map(|c| c.0);
                    save = Some(c.map(|c| rgb_value(c.1)).unwrap_or_else(|| Value::chars("none")));
                    keep = true;
                }
                "LineStyle" => {
                    let t = v.text()?;
                    s.line_style = if t == "none" { LineStyle::None } else { graphics::parse_line_spec(&t).and_then(|p| p.1).ok_or_else(|| LangError::new(format!("'{t}' is not a valid value. Use one of these values: '-' | '--' | ':' | '-.' | 'none'."), 0))? };
                }
                "LineWidth" => s.line_width = v.scalar()? as f32,
                "Marker" => s.marker = parse_marker(&v.text()?),
                "MarkerSize" => s.marker_size = v.scalar()? as f32,
                "SizeData" => s.marker_size = (v.scalar()?.sqrt() as f32).max(2.0),
                "DisplayName" => s.name = Some(v.text()?),
                "Visible" => s.visible = on_off(&v)?,
                "String" | "Label" => s.label = texts(&v)?.join("\n"),
                "Position" => {
                    let m = v.to_matrix()?;
                    if m.numel() >= 2 {
                        s.x = vec![m.data[0]];
                        s.y = vec![m.data[1]];
                    }
                }
                "Value" => {
                    let m = v.to_matrix()?;
                    if s.horizontal { s.y = m.data } else { s.x = m.data }
                }
                "FaceAlpha" => s.face_alpha = v.scalar()? as f32,
                "FontSize" => s.font_size = Some(v.scalar()? as f32),
                "BarWidth" => {
                    s.bar_width = Some(v.scalar()? * graphics::bar_spacing(&s.x));
                    keep = true;
                }
                "Children" | "Parent" | "Type" => {}
                _ => keep = true,
            }
        }
        Target::Part(f, a, p) => {
            mark(it, f);
            if name == "String" {
                let text = texts(&v)?.join("\n");
                let ax = &mut it.figures[f].axes[a];
                match p {
                    Part::Title => {
                        let sub = ax.title.split_once('\n').map(|x| x.1.to_string());
                        ax.title = match sub {
                            Some(sub) => format!("{text}\n{sub}"),
                            None => text,
                        };
                    }
                    Part::Subtitle => {
                        let main = ax.title.split('\n').next().unwrap_or("").to_string();
                        ax.title = if text.is_empty() { main } else { format!("{main}\n{text}") };
                    }
                    Part::XLabel => ax.xlabel = text,
                    Part::YLabel => ax.ylabel = text,
                    Part::YLabelOther => {
                        if let Some(yy) = ax.yy.as_mut() {
                            yy.other.ylabel = text;
                        }
                    }
                    Part::ZLabel => ax.zlabel = text,
                    Part::XSecondary => ax.secondary[0] = text,
                    Part::YSecondary => ax.secondary[1] = text,
                    Part::ZSecondary => ax.secondary[2] = text,
                    Part::Legend => ax.legend = Some(texts(&v)?),
                    _ => {}
                }
                keep = false;
            } else if matches!(p, Part::XAxis | Part::YAxis | Part::YAxisRight) {
                let x = p == Part::XAxis;
                let ax = &mut it.figures[f].axes[a];
                // The inactive side of yyaxis axes: made active while it is set.
                let active = ax.yy.as_ref().map(|yy| yy.right_active);
                let flip = !x && active.is_some_and(|r| r != (p == Part::YAxisRight));
                if flip {
                    ax.switch_side(p == Part::YAxisRight);
                }
                let r = set_ruler(ax, x, name, &v);
                if flip {
                    ax.switch_side(active.unwrap());
                }
                keep = !r?;
            } else if p == Part::Legend && name == "Visible" && !on_off(&v)? {
                it.figures[f].axes[a].legend = None;
                keep = false;
            }
        }
        Target::FigObj(f, k) => {
            mark(it, f);
            keep = false;
            let grid_arg = if name == "GridSize" { Some(v.to_matrix()?) } else { None };
            let o = &mut it.figures[f].objects[k];
            let line = o.kind.is_line();
            match name {
                "X" | "Y" if line => {
                    let m = v.to_matrix()?;
                    if m.numel() != 2 {
                        return rt("Value must be a 2-element vector.");
                    }
                    let p = [m.data[0], m.data[1]];
                    if name == "X" { o.x = p } else { o.y = p }
                }
                "Position" => {
                    let m = v.to_matrix()?;
                    if m.numel() != 4 {
                        return rt("Value must be a 4-element vector.");
                    }
                    if line {
                        o.x = [m.data[0], m.data[0] + m.data[2]];
                        o.y = [m.data[1], m.data[1] + m.data[3]];
                    } else {
                        o.pos = [m.data[0], m.data[1], m.data[2], m.data[3]];
                    }
                }
                "String" => o.text = texts(&v)?.join("\n"),
                "Color" => {
                    o.color = color_arg(&v)?.map(|c| c.0).unwrap_or([0, 0, 0]);
                    keep = true;
                }
                "FaceColor" | "BackgroundColor" => {
                    o.face = color_arg(&v)?.map(|c| c.0);
                    keep = true;
                }
                "EdgeColor" => {
                    o.edge = color_arg(&v)?.map(|c| c.0);
                    keep = true;
                }
                "LineWidth" => o.line_width = v.scalar()? as f32,
                "FontSize" => o.font_size = v.scalar()? as f32,
                "Visible" => o.visible = on_off(&v)?,
                "GridSize" => {
                    let m = grid_arg.unwrap();
                    if m.numel() != 2 {
                        return rt("Value must be a 1x2 vector.");
                    }
                    let (r, c) = (m.data[0].max(1.0) as usize, m.data[1].max(1.0) as usize);
                    let fig = &mut it.figures[f];
                    fig.flow = false;
                    if fig.grid != (r, c) {
                        fig.grid = (r, c);
                        fig.axes.resize(r * c, Axes::default());
                    }
                }
                "Children" | "Parent" | "Type" => {}
                _ => keep = true,
            }
        }
    }
    if save.is_none() && name.ends_with("Color") && v.is_text() {
        if let Ok(Some((_, rgb))) = color_arg(&v) {
            save = Some(rgb_value(rgb));
        }
    }
    if keep {
        store(it, id, name, save.unwrap_or(v));
    }
    Ok(())
}

/// Sets a property of the x or (active) y ruler of `ax`; false when it is only stored.
fn set_ruler(ax: &mut graphics::Axes, x: bool, name: &str, v: &Value) -> R<bool> {
    let ticks = if x { &mut ax.xticks } else { &mut ax.yticks };
    match name {
        "TickLabelFormat" => ticks.format = Some(v.text()?),
        "TickValues" => ticks.values = Some(v.to_matrix()?.data),
        "TickLabels" => ticks.labels = Some(texts(v)?),
        "TickLabelRotation" => ticks.angle = v.scalar()? as f32,
        "Limits" => {
            let l = limits_arg(v)?;
            if x { ax.xlim = l } else { ax.ylim = l }
        }
        "Scale" => {
            let log = v.text()? == "log";
            if x { ax.xlog = log } else { ax.ylog = log }
        }
        "FontSize" => ax.font_size = Some(v.scalar()? as f32),
        "Label" if is_gfx(v) => {}
        _ => return Ok(false),
    }
    Ok(true)
}

/// Resolves a property name for `set`/`get` (any case) or errors like MATLAB.
fn prop_name(it: &Interpreter, id: u64, name: &str) -> R<String> {
    let t = resolve(it, id).ok_or_else(|| LangError::new("Invalid or deleted object.", 0))?;
    let class = class_of(it, t);
    canonical(class, name).ok_or_else(|| LangError::new(format!("Unrecognized property {name} for class {}.", short(class)), 0))
}

/// `h.Name` (case-sensitive, as dot access is in MATLAB).
pub fn member(it: &mut Interpreter, v: &Value, name: &str) -> R<Value> {
    let ids = ids_of(v).unwrap_or_default();
    let class = match v {
        Value::Object(o) => o.class.name.clone(),
        _ => String::new(),
    };
    let mut out = Vec::new();
    for id in &ids {
        let t = resolve(it, *id).ok_or_else(|| LangError::new("Invalid or deleted object.", 0))?;
        let c = class_of(it, t);
        if canonical(c, name).as_deref() != Some(name) {
            return rt(format!("Unrecognized method, property, or field '{name}' for class '{class}'."));
        }
        out.push(get_prop(it, *id, name)?);
    }
    match out.len() {
        1 => Ok(out.pop().unwrap()),
        0 => rt(format!("Unrecognized method, property, or field '{name}' for class '{class}'.")),
        _ => rt("Expected one output from a curly brace or dot indexing expression, but there were more."),
    }
}

/// `h.Name = v`.
pub fn set_member(it: &mut Interpreter, v: &Value, name: &str, value: Value) -> R<()> {
    let class = match v {
        Value::Object(o) => o.class.name.clone(),
        _ => String::new(),
    };
    for id in ids_of(v).unwrap_or_default() {
        let t = resolve(it, id).ok_or_else(|| LangError::new("Invalid or deleted object.", 0))?;
        if canonical(class_of(it, t), name).as_deref() != Some(name) {
            return rt(format!("Unrecognized property '{name}' for class '{class}'."));
        }
        set_prop(it, id, name, value.clone())?;
    }
    Ok(())
}

/// `h(idx)` on a handle array.
pub fn index(it: &Interpreter, v: &Value, idx: &[crate::nd::IndexArg]) -> R<Value> {
    let ids = ids_of(v).unwrap_or_default();
    let m = Value::Num(Matrix::new(ids.len(), if ids.is_empty() { 0 } else { 1 }, ids.iter().map(|&i| i as f64).collect()));
    let picked = crate::indexing::index(&m, idx)?.to_matrix()?;
    Ok(handles(it, &picked.data.iter().map(|&x| x as u64).collect::<Vec<_>>()))
}

/// `==`/`~=` between handles; `None` when the operands are not graphics handles.
pub fn compare(eq: bool, a: &Value, b: &Value) -> Option<Value> {
    let (x, y) = (ids_of(a)?, ids_of(b)?);
    let n = x.len().max(y.len());
    if x.len() != y.len() && x.len() != 1 && y.len() != 1 {
        return None;
    }
    let data = (0..n).map(|i| {
        let same = x[if x.len() == 1 { 0 } else { i }] == y[if y.len() == 1 { 0 } else { i }];
        if same == eq { 1.0 } else { 0.0 }
    }).collect::<Vec<_>>();
    Some(Value::Num(Matrix::new(n, if n == 0 { 0 } else { 1 }, data).cast_to(Kind::Logical)))
}

/// The properties `h = ...` shows for each class.
fn short_props(class: &str, it: &Interpreter, id: u64) -> Vec<&'static str> {
    match class {
        "matlab.ui.Root" => vec!["CurrentFigure", "ScreenPixelsPerInch", "ScreenSize", "MonitorPositions", "Units"],
        "matlab.ui.Figure" => vec!["Number", "Name", "Color", "Position", "Units"],
        "matlab.graphics.axis.Axes" => vec!["XLim", "YLim", "XScale", "YScale", "GridLineStyle", "Position", "Units"],
        "matlab.graphics.primitive.Text" => vec!["String", "FontSize", "FontWeight", "FontName", "Color", "HorizontalAlignment", "Position", "Units"],
        "matlab.graphics.chart.decoration.ConstantLine" => vec!["InterceptAxis", "Value", "Color", "LineStyle", "LineWidth", "Label", "DisplayName"],
        "matlab.graphics.chart.decoration.ConstantRegion" => vec!["InterceptAxis", "Value", "FaceColor", "FaceAlpha", "DisplayName"],
        "matlab.graphics.chart.primitive.Line" | "matlab.graphics.primitive.Line" => {
            let _ = (it, id);
            vec!["Color", "LineStyle", "LineWidth", "Marker", "MarkerSize", "MarkerFaceColor", "XData", "YData"]
        }
        "matlab.graphics.chart.primitive.Scatter" => vec!["Marker", "MarkerEdgeColor", "MarkerFaceColor", "LineWidth", "XData", "YData", "SizeData", "CData"],
        "matlab.graphics.chart.primitive.BubbleChart" => vec!["MarkerEdgeColor", "MarkerFaceColor", "LineWidth", "XData", "YData", "SizeData", "CData"],
        "matlab.graphics.chart.primitive.Bar" => vec!["BarLayout", "BarWidth", "FaceColor", "EdgeColor", "BaseValue", "XData", "YData"],
        "matlab.graphics.chart.primitive.ErrorBar" => vec!["Color", "LineStyle", "LineWidth", "Marker", "XData", "YData", "YNegativeDelta", "YPositiveDelta"],
        "matlab.graphics.primitive.Patch" => vec!["FaceColor", "FaceAlpha", "EdgeColor", "LineStyle", "Faces", "Vertices"],
        "matlab.graphics.axis.decorator.NumericRuler" => vec!["Limits", "Scale", "Exponent", "TickValues", "TickLabelFormat"],
        "matlab.graphics.illustration.Legend" => vec!["String", "Location", "Orientation", "FontSize", "Position", "Units"],
        c if crate::charts::is_chart(c) => crate::charts::short_props(c, it, id),
        c => props_of(c).iter().map(|p| p.0).filter(|n| !n.ends_with("Mode")).take(6).collect(),
    }
}

/// The text `name = h` shows.
/// The heading MATLAB shows for one graphics object: `Line (data1)`, `Figure (1: Name)`,
/// `DataTip (X 3, Y 3)`.
fn object_title(it: &mut Interpreter, id: u64, t: Target) -> String {
    let class = class_of(it, t);
    match t {
        Target::Root => "Graphics Root".to_string(),
        Target::Figure(f) => {
            let n = it.figures[f].id;
            match stored(it, id, "Name").map(|v| v.text().unwrap_or_default()).filter(|s| !s.is_empty()) {
                Some(nm) => format!("Figure ({n}: {nm})"),
                None => format!("Figure ({n})"),
            }
        }
        _ if crate::charts::is_chart(class) => crate::charts::display_title(it, id, short(class)),
        _ if class.ends_with(".Text") => {
            let s = get_prop(it, id, "String").ok().and_then(|v| v.text().ok()).unwrap_or_default();
            if s.is_empty() { "Text".to_string() } else { format!("Text ({s})") }
        }
        // MATLAB names the object by its Tag, else its DisplayName: `Line (data1)`.
        _ => {
            let label = ["Tag", "DisplayName"].iter().find_map(|p| get_prop(it, id, p).ok().and_then(|v| v.text().ok()).filter(|s| !s.is_empty()));
            match label {
                Some(l) => format!("{} ({l})", short(class)),
                None => short(class).to_string(),
            }
        }
    }
}

pub fn display(it: &mut Interpreter, name: &str, v: &Value) -> R<String> {
    let ids = ids_of(v).unwrap_or_default();
    let class = match v {
        Value::Object(o) => o.class.name.clone(),
        _ => String::new(),
    };
    if ids.len() != 1 {
        let dims = match v {
            Value::Object(o) => match o.props.borrow().first() {
                Some(Value::Num(m)) => (m.rows(), m.cols()),
                _ => (ids.len(), 1),
            },
            _ => (ids.len(), 1),
        };
        let shape = if ids.is_empty() { "0×0".to_string() } else { format!("{}×{}", dims.0, dims.1) };
        // Elements with a label line it up in a column after the widest class name.
        let titles: Vec<(String, String)> = ids
            .iter()
            .map(|&id| match resolve(it, id) {
                Some(t) => {
                    let title = object_title(it, id, t);
                    match title.split_once(" (") {
                        Some((c, rest)) => (c.to_string(), format!("({rest}")),
                        None => (title, String::new()),
                    }
                }
                None => ("GraphicsPlaceholder".to_string(), String::new()),
            })
            .collect();
        let width = titles.iter().map(|(c, _)| c.chars().count()).max().unwrap_or(0) + 4;
        // A row of unlabeled objects is shown on one line.
        if dims.0 == 1 && dims.1 > 1 && titles.iter().all(|(_, l)| l.is_empty()) {
            let line: Vec<&str> = titles.iter().map(|(c, _)| c.as_str()).collect();
            return Ok(format!("{name} = 
  {shape} {} array:

    {}", short(&class), line.join("    ")));
        }
        let names: String = titles.iter().map(|(c, l)| if l.is_empty() { format!("  {c}\n") } else { format!("  {c:<width$}{l}\n") }).collect();
        return Ok(format!("{name} = \n  {shape} {} array:\n\n{names}", short(&class)).trim_end().to_string());
    }
    let id = ids[0];
    let Some(t) = resolve(it, id) else {
        return Ok(format!("{name} = \n  handle to deleted {}", short(&class)));
    };
    let title = object_title(it, id, t);
    let mut s = StructArray::scalar();
    for p in short_props(&class, it, id) {
        let v = get_prop(it, id, p)?;
        s.set(p, v);
    }
    let body = crate::display::struct_fields_body(&s, it.format);
    Ok(format!("{name} = \n  {title} with properties:\n\n{body}\n  Use GET to show all properties"))
}

// ---- builtins --------------------------------------------------------------------------------

fn get_all(it: &mut Interpreter, id: u64) -> R<Value> {
    let t = resolve(it, id).ok_or_else(|| LangError::new("Invalid or deleted object.", 0))?;
    let class = class_of(it, t);
    let mut s = StructArray::scalar();
    for (n, _) in props_of(class) {
        s.set(n, get_prop(it, id, n)?);
    }
    Ok(Value::Struct(s))
}

fn set_builtin(it: &mut Interpreter, a: &[Value]) -> Ret {
    let ids = target_ids(it, arg(a, 0, "set")?)?;
    let mut pairs: Vec<(String, Value)> = Vec::new();
    let mut i = 1;
    while i < a.len() {
        match &a[i] {
            Value::Struct(s) => {
                for (k, f) in s.fields.iter().enumerate() {
                    pairs.push((f.clone(), s.elems[0][k].clone()));
                }
                i += 1;
            }
            Value::Cell(names) if matches!(a.get(i + 1), Some(Value::Cell(_))) => {
                let Value::Cell(vals) = &a[i + 1] else { unreachable!() };
                for (k, n) in names.data.iter().enumerate() {
                    pairs.push((n.text()?, vals.data.get(k).cloned().unwrap_or_else(Value::empty)));
                }
                i += 2;
            }
            v => {
                let n = v.text()?;
                let Some(val) = a.get(i + 1) else {
                    return rt(format!("Invalid parameter/value pair arguments ('{n}')."));
                };
                pairs.push((n, val.clone()));
                i += 2;
            }
        }
    }
    for id in ids {
        for (n, v) in &pairs {
            let name = prop_name(it, id, n)?;
            set_prop(it, id, &name, v.clone())?;
        }
    }
    none()
}

fn get_builtin(it: &mut Interpreter, a: &[Value]) -> Ret {
    let ids = target_ids(it, arg(a, 0, "get")?)?;
    let names: Option<Vec<String>> = match a.get(1) {
        None => None,
        Some(Value::Cell(c)) => Some(c.data.iter().map(|x| x.text()).collect::<R<_>>()?),
        Some(v) => Some(vec![v.text()?]),
    };
    let single_name = matches!(a.get(1), Some(v) if !matches!(v, Value::Cell(_)));
    let mut rows: Vec<Vec<Value>> = Vec::new();
    for &id in &ids {
        match &names {
            None => rows.push(vec![get_all(it, id)?]),
            Some(ns) => {
                let mut r = Vec::new();
                for n in ns {
                    let name = prop_name(it, id, n)?;
                    r.push(get_prop(it, id, &name)?);
                }
                rows.push(r);
            }
        }
    }
    if rows.len() == 1 && (single_name || names.is_none()) {
        return one(rows.pop().unwrap().pop().unwrap());
    }
    // Several handles or a cell of names: an m×n cell.
    let (m, n) = (rows.len(), rows.first().map(|r| r.len()).unwrap_or(0));
    let mut data = vec![Value::empty(); m * n];
    for (i, r) in rows.into_iter().enumerate() {
        for (j, v) in r.into_iter().enumerate() {
            data[j * m + i] = v;
        }
    }
    one(Value::Cell(CellArray::new(m, n, data)))
}

/// Every handle below `id` (children first-to-last), including parts when `all`.
pub(crate) fn descendants(it: &mut Interpreter, id: u64, out: &mut Vec<u64>) {
    match resolve(it, id) {
        Some(Target::Root) => {
            let figs: Vec<u64> = it.figures.iter().map(|f| f.id as u64).collect();
            for f in figs {
                out.push(f);
                descendants(it, f, out);
            }
        }
        Some(Target::Figure(f)) => {
            for a in 0..it.figures[f].axes.len() {
                let ax = axes_id(it, f, a);
                out.push(ax);
                descendants(it, ax, out);
            }
        }
        Some(Target::Axes(f, a)) => {
            for s in it.figures[f].axes[a].series.iter().rev() {
                out.push(s.id);
            }
        }
        _ => {}
    }
}

fn findobj(it: &mut Interpreter, a: &[Value]) -> Ret {
    let (roots, rest) = match a.first() {
        Some(v) if is_gfx(v) => (ids_of(v).unwrap(), &a[1..]),
        // 0 is the graphics root.
        Some(Value::Num(m)) if m.kind != Kind::Char && m.numel() == 1 && m.data[0] == 0.0 => (vec![ROOT], &a[1..]),
        _ => (vec![ROOT], a),
    };
    let mut cands = Vec::new();
    for r in roots {
        cands.push(r);
        descendants(it, r, &mut cands);
    }
    let mut keep = Vec::new();
    'next: for id in cands {
        let mut i = 0;
        while i + 1 < rest.len() {
            let n = rest[i].text()?;
            if n.starts_with('-') {
                i += 1;
                continue;
            }
            let Some(t) = resolve(it, id) else { continue 'next };
            let Some(name) = canonical(class_of(it, t), &n) else { continue 'next };
            let v = get_prop(it, id, &name)?;
            let want = &rest[i + 1];
            let same = match (&v, want) {
                (a, b) if a.is_text() && (b.is_text() || matches!(b, Value::Str(_))) => a.text()?.eq_ignore_ascii_case(&b.text()?),
                (a, b) => crate::builtins::values_equal(a, b),
            };
            if !same {
                continue 'next;
            }
            i += 2;
        }
        keep.push(id);
    }
    one(handles(it, &keep))
}

fn delete_handles(it: &mut Interpreter, ids: &[u64]) -> R<()> {
    // A chart's parts and an object's data tips go with it.
    let mut all: Vec<u64> = Vec::new();
    let mut todo: Vec<u64> = ids.to_vec();
    while let Some(id) = todo.pop() {
        if !all.contains(&id) {
            all.push(id);
            todo.extend(crate::charts::dependents(it, id));
        }
    }
    for &id in &all {
        match resolve(it, id) {
            Some(Target::Series(f, a, s)) => {
                it.figures[f].axes[a].series.remove(s);
                mark(it, f);
            }
            Some(Target::Axes(f, a)) => {
                let ax = &mut it.figures[f].axes[a];
                *ax = Axes { id: 0, ..Axes::default() };
                mark(it, f);
            }
            Some(Target::Figure(f)) => {
                it.figures.remove(f);
                it.current_figure = if it.figures.is_empty() { None } else { Some(it.figures.len() - 1) };
            }
            Some(Target::Part(f, a, Part::Legend)) => {
                it.figures[f].axes[a].legend = None;
                mark(it, f);
            }
            Some(Target::FigObj(f, k)) => {
                it.figures[f].objects.remove(k);
                mark(it, f);
            }
            _ => {}
        }
        it.gfx_store.remove(&id);
    }
    Ok(())
}

/// Copies the stored properties of object `from` (and of its parts, for axes) to `to`.
fn copy_store(it: &mut Interpreter, from: u64, to: u64, slots: u64) {
    for k in 0..slots {
        if let Some(v) = it.gfx_store.get(&(from + k)).cloned() {
            it.gfx_store.insert(to + k, v);
        }
    }
}

/// A copy of a plotted object with a new handle, its color fixed to the one it shows now.
fn series_copy(it: &mut Interpreter, f: usize, a: usize, s: usize) -> Series {
    let mut c = it.figures[f].axes[a].series[s].clone();
    let mut color = None;
    if c.color.is_none() {
        if let Ok(Value::Num(m)) = get_prop(it, c.id, "Color") {
            if m.data.len() == 3 {
                c.color = Some(to_u8([m.data[0], m.data[1], m.data[2]]));
                color = Some(Value::Num(m));
            }
        }
    }
    let id = alloc(it);
    copy_store(it, c.id, id, 1);
    if let Some(v) = color {
        if stored(it, id, "Color").is_none() {
            store(it, id, "Color", v);
        }
    }
    c.id = id;
    c
}

/// A copy of axes `a` of figure `f` with new handles for it and its objects.
fn axes_copy(it: &mut Interpreter, f: usize, a: usize) -> Axes {
    let old = axes_id(it, f, a);
    let series: Vec<Series> = (0..it.figures[f].axes[a].series.len()).map(|s| series_copy(it, f, a, s)).collect();
    let mut c = it.figures[f].axes[a].clone();
    let id = alloc(it);
    copy_store(it, old, id, SLOTS);
    c.id = id;
    c.overlay_of = 0;
    c.series = series;
    c
}

/// `copyobj(h, parent)`: copies of objects into new parents, one per child-parent pair.
fn copyobj(it: &mut Interpreter, a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    let kids = target_ids(it, &a[0])?;
    let parents = target_ids(it, &a[1])?;
    if kids.len() > 1 && parents.len() > 1 && kids.len() != parents.len() {
        return rt("Parent and child input vectors must be equal length.");
    }
    let n = kids.len().max(parents.len());
    let pair = |i: usize| (kids[if kids.len() == 1 { 0 } else { i }], parents[if parents.len() == 1 { 0 } else { i }]);
    let mut seen: Vec<(u64, u64)> = Vec::new();
    let mut plan: Vec<(Target, Target)> = Vec::new();
    for i in 0..n {
        let (k, p) = pair(i);
        let (Some(kt), Some(pt)) = (resolve(it, k), resolve(it, p)) else {
            return rt("Invalid or deleted object.");
        };
        let (kc, pc) = (short(class_of(it, kt)), short(class_of(it, pt)));
        let ok = matches!((kt, pt), (Target::Series(..), Target::Axes(..)) | (Target::Axes(..), Target::Figure(_)) | (Target::Figure(_), Target::Root));
        if !ok {
            if let (Target::Figure(_), Target::Axes(..)) = (kt, pt) {
                return rt(format!("{pc} cannot be a parent."));
            }
            return rt(format!("{kc} cannot be a child of {pc}."));
        }
        if seen.contains(&(k, p)) {
            return rt("Cannot copy the same object more than once to the same parent. Try calling COPYOBJ twice instead.");
        }
        seen.push((k, p));
        plan.push((kt, pt));
    }
    // Copy everything first, so that an object copied into its own parent is copied once.
    let mut made: Vec<Option<u64>> = vec![None; n];
    let mut add_series: Vec<(usize, u64, Series)> = Vec::new();
    let mut add_axes: Vec<(usize, u64, Axes)> = Vec::new();
    let mut add_figs: Vec<(usize, Figure)> = Vec::new();
    for (i, &(kt, pt)) in plan.iter().enumerate() {
        match (kt, pt) {
            (Target::Series(f, ax, s), Target::Axes(..)) => {
                let c = series_copy(it, f, ax, s);
                made[i] = Some(c.id);
                add_series.push((i, pair(i).1, c));
            }
            (Target::Axes(f, ax), Target::Figure(_)) => {
                let c = axes_copy(it, f, ax);
                made[i] = Some(c.id);
                add_axes.push((i, pair(i).1, c));
            }
            (Target::Figure(f), _) => {
                let mut fig = it.figures[f].clone();
                let axes: Vec<Axes> = (0..fig.axes.len()).map(|ax| axes_copy(it, f, ax)).collect();
                fig.axes = axes;
                for o in &mut fig.objects {
                    let id = alloc(it);
                    copy_store(it, o.id, id, 1);
                    o.id = id;
                }
                let mut num = 1;
                while it.figures.iter().any(|g| g.id == num) {
                    num += 1;
                }
                copy_store(it, fig.id as u64, num as u64, 1);
                fig.id = num;
                it.next_figure_id = it.next_figure_id.max(num + 1);
                made[i] = Some(num as u64);
                add_figs.push((i, fig));
            }
            _ => {}
        }
    }
    // Objects copied together keep their stacking: the first is on top.
    for (_, p, c) in add_series.into_iter().rev() {
        if let Some(Target::Axes(f, ax)) = resolve(it, p) {
            it.figures[f].axes[ax].series.push(c);
            mark(it, f);
        }
    }
    for (_, p, c) in add_axes.into_iter().rev() {
        if let Some(Target::Figure(f)) = resolve(it, p) {
            it.figures[f].axes.push(c);
            mark(it, f);
        }
    }
    for (_, fig) in add_figs {
        let id = fig.id;
        it.figures.push(fig);
        it.current_figure = Some(it.figures.len() - 1);
        it.mark_figure_dirty(id);
    }
    let ids: Vec<u64> = made.into_iter().flatten().collect();
    one(handles(it, &ids))
}

/// `delete(h)` for graphics handles; `None` when the argument is not one.
pub fn delete(it: &mut Interpreter, a: &[Value]) -> Option<Ret> {
    let ids = ids_of(a.first()?)?;
    Some(delete_handles(it, &ids).and_then(|_| none()))
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    match name {
        "set" => set_builtin(it, a),
        // Figures are never off screen here, so there is none to bring back.
        "findfigs" => none(),
        "copyobj" => copyobj(it, a),
        // handle(h) of graphics objects is the objects themselves.
        "handle" => {
            let v = arg(a, 0, name)?;
            if ids_of(v).is_none() {
                return rt(format!("Undefined function 'handle' for input arguments of type '{}'.", v.class_name()));
            }
            one(v.clone())
        }
        "get" => get_builtin(it, a),
        "isgraphics" | "ishandle" | "ishghandle" => {
            let v = arg(a, 0, name)?;
            let ids: Vec<Option<u64>> = match ids_of(v) {
                Some(ids) => ids.into_iter().map(Some).collect(),
                None => match v {
                    Value::Num(m) if m.kind != Kind::Char => m.data.iter().map(|&x| (x >= 0.0 && x.fract() == 0.0).then_some(x as u64)).collect(),
                    _ => return one(Value::bool(false)),
                },
            };
            let want = a.get(1).map(|t| t.text()).transpose()?;
            let data: Vec<f64> = ids
                .iter()
                .map(|id| {
                    let ok = id.and_then(|id| resolve(it, id).map(|t| (id, t))).is_some_and(|(id, t)| match &want {
                        Some(ty) => get_prop(it, id, "Type").ok().and_then(|v| v.text().ok()).is_some_and(|s| s.eq_ignore_ascii_case(ty)),
                        None => { let _ = t; true }
                    });
                    if ok { 1.0 } else { 0.0 }
                })
                .collect();
            // An array of graphics objects keeps its shape.
            let dims = match v {
                Value::Object(o) if ids_of(v).is_some() => match o.props.borrow().first() {
                    Some(Value::Num(m)) if m.numel() == data.len() => m.dims.clone(),
                    _ => vec![data.len(), 1],
                },
                _ => v.dims(),
            };
            one(Value::Num(Matrix::from_dims(dims, data).cast_to(Kind::Logical)))
        }
        "isvalid" => {
            let v = arg(a, 0, name)?;
            match ids_of(v) {
                Some(ids) => {
                    let data: Vec<f64> = ids.iter().map(|&id| if resolve(it, id).is_some() { 1.0 } else { 0.0 }).collect();
                    let n = data.len();
                    one(Value::Num(Matrix::new(n, if n == 0 { 0 } else { 1 }, data).cast_to(Kind::Logical)))
                }
                None => one(Value::bool(matches!(v, Value::Object(_)))),
            }
        }
        "groot" => one(handle(it, ROOT)),
        // gobjects(n) is n-by-n, gobjects(m, n, ...) m-by-n, gobjects() 0-by-0.
        "gobjects" => {
            let (r, c) = match a.len() {
                0 => (0, 0),
                1 => match a[0].to_matrix()?.data.as_slice() {
                    [n] => (*n as usize, *n as usize),
                    [m, n, ..] => (*m as usize, *n as usize),
                    _ => (0, 0),
                },
                _ => (a[0].scalar()? as usize, a[1].scalar()? as usize),
            };
            let m = Matrix::new(r, c, vec![PLACEHOLDER as f64; r * c]);
            one(Value::Object(Object { class: class_def("matlab.graphics.GraphicsPlaceholder"), props: Rc::new(RefCell::new(vec![Value::Num(m)])) }))
        }
        "findobj" | "findall" => findobj(it, a),
        "allchild" => {
            let ids = target_ids(it, arg(a, 0, name)?)?;
            let mut out = Vec::new();
            for id in ids {
                match resolve(it, id) {
                    Some(Target::Axes(f, ai)) => out.extend(it.figures[f].axes[ai].series.iter().rev().map(|s| s.id)),
                    Some(Target::Figure(_) | Target::Root) => {
                        let v = get_prop(it, id, "Children")?;
                        out.extend(ids_of(&v).unwrap_or_default());
                    }
                    _ => {}
                }
            }
            one(handles(it, &out))
        }
        "ancestor" => {
            let ids = target_ids(it, arg(a, 0, name)?)?;
            let types = texts(arg(a, 1, name)?)?;
            let mut cur = ids.first().copied();
            while let Some(id) = cur {
                let ty = get_prop(it, id, "Type")?.text()?;
                if types.iter().any(|t| t.eq_ignore_ascii_case(&ty)) {
                    return one(handle(it, id));
                }
                cur = if id == ROOT { None } else { ids_of(&get_prop(it, id, "Parent")?).and_then(|p| p.first().copied()) };
            }
            one(handles(it, &[]))
        }
        "axes" => {
            if let Some(ids) = a.first().and_then(ids_of) {
                if let Some(Target::Axes(f, ax)) = ids.first().and_then(|&id| resolve(it, id)) {
                    it.current_figure = Some(f);
                    it.figures[f].current_axes = ax;
                    return one(handle(it, ids[0]));
                }
                // axes(fig, ...): the figure's current axes.
                if let Some(Target::Figure(f)) = ids.first().and_then(|&id| resolve(it, id)) {
                    it.current_figure = Some(f);
                }
            }
            let f = it.current_figure_index();
            let ax = it.figures[f].current_axes;
            let id = axes_id(it, f, ax);
            let start = if a.first().is_some_and(is_gfx) { 1 } else { 0 };
            let mut rest = vec![handle(it, id)];
            rest.extend_from_slice(&a[start..]);
            if rest.len() > 1 {
                set_builtin(it, &rest)?;
            }
            one(handle(it, id))
        }
        "cla" => {
            let target = match a.first().and_then(ids_of) {
                Some(ids) => match ids.first().and_then(|&id| resolve(it, id)) {
                    Some(Target::Axes(f, ax)) => (f, ax),
                    _ => return rt("Invalid or deleted object."),
                },
                None => {
                    let f = it.current_figure_index();
                    (f, it.figures[f].current_axes)
                }
            };
            let reset = a.iter().any(|v| v.text().map(|t| t == "reset").unwrap_or(false));
            let (f, ax) = target;
            let axes = &mut it.figures[f].axes[ax];
            let removed: Vec<u64> = axes.series.iter().map(|s| s.id).collect();
            if reset {
                *axes = Axes { id: axes.id, overlay_of: axes.overlay_of, ..Axes::default() };
            } else {
                axes.series.clear();
                axes.legend = None;
            }
            for id in removed {
                it.gfx_store.remove(&id);
            }
            mark(it, f);
            none()
        }
        _ => Ok(None),
    }
}

/// Properties of polar axes computed from the figure model.
fn polar_prop(name: &str) -> bool {
    matches!(
        name,
        "ThetaLim" | "RLim" | "ThetaLimMode" | "RLimMode" | "ThetaTick" | "RTick" | "ThetaTickLabel" | "RTickLabel" | "ThetaTickMode" | "RTickMode"
            | "ThetaDir" | "ThetaZeroLocation" | "RAxisLocation" | "RTickLabelRotation" | "Type"
    )
}

// ---- histogram bins ---------------------------------------------------------------------------

const HISTOGRAM: &str = "matlab.graphics.chart.primitive.Histogram";

fn is_histogram(it: &Interpreter, id: u64) -> bool {
    matches!(resolve(it, id), Some(t @ Target::Series(..)) if class_of(it, t) == HISTOGRAM)
}

/// Bins the histogram's data with `histcounts(data, opts...)` and updates the bars and the
/// bin properties.
pub(crate) fn histogram_bin(it: &mut Interpreter, id: u64, opts: Vec<Value>) -> R<()> {
    let data = stored(it, id, "Data").ok_or_else(|| LangError::new("The histogram has no data.", 0))?;
    let mut args = vec![data];
    args.extend(opts);
    let out = it.call_function("histcounts", args, 2)?;
    let counts = out[0].to_matrix()?.data;
    let edges = out[1].to_matrix()?.data;
    let Some(Target::Series(f, a, s)) = resolve(it, id) else { return rt("Invalid or deleted object.") };
    let n = counts.len();
    let centers: Vec<f64> = (0..n).map(|k| (edges[k] + edges[k + 1]) / 2.0).collect();
    let width = if n > 0 { (edges[n] - edges[0]) / n as f64 } else { 1.0 };
    {
        let sr = &mut it.figures[f].axes[a].series[s];
        sr.x = centers;
        sr.y = counts.clone();
        sr.bar_width = Some(width);
    }
    mark(it, f);
    let widths: Vec<f64> = (0..n).map(|k| edges[k + 1] - edges[k]).collect();
    let uniform = widths.windows(2).all(|w| (w[0] - w[1]).abs() <= 1e-12 * w[0].abs().max(1.0));
    store(it, id, "Values", Value::Num(Matrix::row(counts.clone())));
    store(it, id, "BinCounts", Value::Num(Matrix::row(counts)));
    store(it, id, "BinEdges", Value::Num(Matrix::row(edges.clone())));
    store(it, id, "NumBins", Value::num(n as f64));
    store(it, id, "BinWidth", if uniform { Value::num(widths.first().copied().unwrap_or(1.0)) } else { Value::chars("nonuniform") });
    store(it, id, "BinLimits", Value::Num(Matrix::row(vec![edges[0], edges[n]])));
    Ok(())
}

/// A binning property set on a histogram: rebins (None when it is another property).
fn histogram_set(it: &mut Interpreter, id: u64, name: &str, v: &Value) -> Option<R<()>> {
    if !matches!(name, "NumBins" | "BinWidth" | "BinEdges" | "BinLimits" | "BinMethod") || !is_histogram(it, id) {
        return None;
    }
    let mut opts = Vec::new();
    // Explicit limits are kept when the number or width of bins changes.
    if matches!(name, "NumBins" | "BinWidth") {
        if let Some(Value::Num(m)) = stored(it, id, "BinLimitsMode").filter(|v| v.text().map(|t| t == "manual").unwrap_or(false)).and(stored(it, id, "BinLimits")) {
            opts.push(Value::chars("BinLimits"));
            opts.push(Value::Num(m));
        }
    }
    match name {
        "NumBins" => opts.insert(0, v.clone()),
        "BinEdges" => {
            opts.insert(0, v.clone());
            store(it, id, "BinLimitsMode", Value::chars("manual"));
        }
        other => {
            opts.push(Value::chars(other));
            opts.push(v.clone());
            if other == "BinLimits" {
                store(it, id, "BinLimitsMode", Value::chars("manual"));
            }
        }
    }
    Some(histogram_bin(it, id, opts))
}

/// morebins / fewerbins: about 10% more or fewer bins, keeping manual limits.
pub(crate) fn histogram_more_fewer(it: &mut Interpreter, h: &Value, more: bool) -> R<Value> {
    let ids = ids_of(h).ok_or_else(|| LangError::new("Input must be a histogram object.", 0))?;
    let mut last = 0.0;
    for id in ids {
        if !is_histogram(it, id) {
            return rt("Input must be a histogram object.");
        }
        let n = stored(it, id, "NumBins").map(|v| v.scalar()).transpose()?.unwrap_or(1.0);
        let k = if more { (n * 1.1).ceil().max(n + 1.0) } else { (n * 0.9).floor().min(n - 1.0).max(1.0) };
        let mut opts = vec![Value::num(k)];
        if stored(it, id, "BinLimitsMode").and_then(|v| v.text().ok()).as_deref() == Some("manual") {
            if let Some(l) = stored(it, id, "BinLimits") {
                opts.push(Value::chars("BinLimits"));
                opts.push(l);
            }
        }
        histogram_bin(it, id, opts)?;
        last = k;
    }
    Ok(Value::num(last))
}
