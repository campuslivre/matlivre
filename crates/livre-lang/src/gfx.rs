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
use crate::graphics::{self, Axes, LineStyle, Marker, Series, SeriesKind};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &["set", "get", "isgraphics", "ishandle", "ishghandle", "groot", "gobjects", "findobj", "findall", "allchild", "ancestor", "axes", "cla", "isvalid"];

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
}

const PARTS: [Part; 9] = [Part::Title, Part::XLabel, Part::YLabel, Part::ZLabel, Part::Subtitle, Part::XAxis, Part::YAxis, Part::ZAxis, Part::Legend];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Root,
    Figure(usize),
    Axes(usize, usize),
    Series(usize, usize, usize),
    Part(usize, usize, Part),
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
    }
}

fn class_of(it: &Interpreter, t: Target) -> &'static str {
    match t {
        Target::Root => "matlab.ui.Root",
        Target::Figure(_) => "matlab.ui.Figure",
        Target::Axes(..) => "matlab.graphics.axis.Axes",
        Target::Series(f, a, s) => series_class(&it.figures[f].axes[a].series[s]),
        Target::Part(_, _, Part::XAxis | Part::YAxis | Part::ZAxis) => "matlab.graphics.axis.decorator.NumericRuler",
        Target::Part(_, _, Part::Legend) => "matlab.graphics.illustration.Legend",
        Target::Part(..) => "matlab.graphics.primitive.Text",
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
        _ => "matlab.graphics.Graphics",
    };
    let data: Vec<f64> = ids.iter().map(|&i| i as f64).collect();
    let m = if ids.is_empty() { Matrix::new(0, 0, Vec::new()) } else { Matrix::new(data.len(), 1, data) };
    Value::Object(Object { class: class_def(class), props: Rc::new(RefCell::new(vec![Value::Num(m)])) })
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
    t.by_class.get(class).map(|v| v.as_slice()).unwrap_or(&[])
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
fn color_arg(v: &Value) -> R<Option<([u8; 3], [f64; 3])>> {
    if v.is_text() || matches!(v, Value::Str(_)) {
        let t = v.text()?.to_lowercase();
        if t == "none" {
            return Ok(None);
        }
        if let Some(hex) = t.strip_prefix('#') {
            let n = u32::from_str_radix(hex, 16).map_err(|_| LangError::new("Invalid color value.", 0))?;
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

fn marker_name(m: Option<Marker>) -> &'static str {
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

fn parse_marker(t: &str) -> Option<Marker> {
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

fn cell_col(items: &[String]) -> Value {
    Value::Cell(CellArray::col(items.iter().map(|s| Value::chars(s)).collect()))
}

// ---- get / set -------------------------------------------------------------------------------

fn stored(it: &Interpreter, id: u64, name: &str) -> Option<Value> {
    it.gfx_store.get(&id).and_then(|m| m.iter().find(|(n, _)| n == name)).map(|(_, v)| v.clone())
}

fn store(it: &mut Interpreter, id: u64, name: &str, v: Value) {
    let m = it.gfx_store.entry(id).or_default();
    match m.iter_mut().find(|(n, _)| n == name) {
        Some(e) => e.1 = v,
        None => m.push((name.to_string(), v)),
    }
}

/// Ticks of an axis: set values, or the automatic ones within the limits.
fn tick_values(ticks: &graphics::Ticks, lo: f64, hi: f64) -> Vec<f64> {
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
fn get_prop(it: &mut Interpreter, id: u64, name: &str) -> R<Value> {
    let t = resolve(it, id).ok_or_else(|| LangError::new("Invalid or deleted object.", 0))?;
    let class = class_of(it, t);
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
            _ => return Ok(None),
        },
        Target::Figure(f) => match name {
            "Number" => Value::num(it.figures[f].id as f64),
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
        Target::Axes(f, a) => {
            let ax = &it.figures[f].axes[a];
            let ((x0, x1), (y0, y1)) = ax.limits();
            match name {
                "XLim" => row(&[x0, x1]),
                "YLim" => row(&[y0, y1]),
                "ZLim" => {
                    let (z0, z1) = ax.zlim.unwrap_or((-1.0, 1.0));
                    row(&[z0, z1])
                }
                "XLimMode" => Value::chars(if ax.xlim.is_some() { "manual" } else { "auto" }),
                "YLimMode" => Value::chars(if ax.ylim.is_some() { "manual" } else { "auto" }),
                "XScale" => Value::chars(if ax.xlog { "log" } else { "linear" }),
                "YScale" => Value::chars(if ax.ylog { "log" } else { "linear" }),
                "XGrid" | "YGrid" => on_off_value(ax.grid),
                "XTick" => row(&tick_values(&ax.xticks, x0, x1)),
                "YTick" => row(&tick_values(&ax.yticks, y0, y1)),
                "XTickLabel" => cell_col(&tick_labels(&ax.xticks, &tick_values(&ax.xticks, x0, x1))),
                "YTickLabel" => cell_col(&tick_labels(&ax.yticks, &tick_values(&ax.yticks, y0, y1))),
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
                "Position" | "InnerPosition" if stored(it, id, name).is_none() => row(&[0.13, 0.11, 0.775, 0.815]),
                "OuterPosition" if stored(it, id, name).is_none() => row(&[0.0, 0.0, 1.0, 1.0]),
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
                    let ids: Vec<u64> = ax.series.iter().rev().map(|s| s.id).collect();
                    handles(it, &ids)
                }
                "Parent" => handle(it, it.figures[f].id as u64),
                _ => return Ok(None),
            }
        }
        Target::Series(f, a, si) => {
            let s = &it.figures[f].axes[a].series[si];
            match name {
                "XData" if s.kind == SeriesKind::BarH => row(&s.x),
                "XData" => row(&s.x),
                "YData" => row(&s.y),
                "ZData" => row(&[]),
                "Color" | "FaceColor" if s.kind != SeriesKind::Text => match stored(it, id, name) {
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
                (Part::XAxis | Part::YAxis, _) => {
                    let x = p == Part::XAxis;
                    let ((x0, x1), (y0, y1)) = ax.limits();
                    let (lo, hi, ticks, log) = if x { (x0, x1, &ax.xticks, ax.xlog) } else { (y0, y1, &ax.yticks, ax.ylog) };
                    match name {
                        "Limits" => row(&[lo, hi]),
                        "TickValues" => row(&tick_values(ticks, lo, hi)),
                        "TickLabels" => cell_col(&tick_labels(ticks, &tick_values(ticks, lo, hi))),
                        "TickLabelFormat" => Value::chars(ticks.format.as_deref().unwrap_or("%g")),
                        "TickLabelRotation" => Value::num(ticks.angle as f64),
                        "Scale" => Value::chars(if log { "log" } else { "linear" }),
                        "Label" => {
                            let base = axes_id(it, f, a);
                            handle(it, base + if x { Part::XLabel } else { Part::YLabel } as u64)
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
fn set_prop(it: &mut Interpreter, id: u64, name: &str, v: Value) -> R<()> {
    let auto = v.text().map(|t| t == "auto").unwrap_or(false);
    let mut save: Option<Value> = None;
    let t = resolve(it, id).ok_or_else(|| LangError::new("Invalid or deleted object.", 0))?;
    let mut keep = true;
    match t {
        Target::Root | Target::Figure(_) => {
            if let (Target::Figure(f), "Name") = (t, name) {
                mark(it, f);
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
                "XGrid" | "YGrid" | "ZGrid" => ax.grid = on_off(&v)?,
                "XTick" => ax.xticks.values = Some(v.to_matrix()?.data),
                "YTick" => ax.yticks.values = Some(v.to_matrix()?.data),
                "XTickMode" if auto => ax.xticks.values = None,
                "YTickMode" if auto => ax.yticks.values = None,
                "XTickLabel" => ax.xticks.labels = Some(texts(&v)?),
                "YTickLabel" => ax.yticks.labels = Some(texts(&v)?),
                "XTickLabelMode" if auto => ax.xticks.labels = None,
                "YTickLabelMode" if auto => ax.yticks.labels = None,
                "XTickLabelRotation" => ax.xticks.angle = v.scalar()? as f32,
                "YTickLabelRotation" => ax.yticks.angle = v.scalar()? as f32,
                "FontSize" => ax.font_size = Some(v.scalar()? as f32),
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
                "XData" => s.x = v.to_matrix()?.data,
                "YData" => s.y = v.to_matrix()?.data,
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
                    Part::ZLabel => ax.zlabel = text,
                    Part::Legend => ax.legend = Some(texts(&v)?),
                    _ => {}
                }
                keep = false;
            } else if matches!(p, Part::XAxis | Part::YAxis) {
                let x = p == Part::XAxis;
                let ax = &mut it.figures[f].axes[a];
                let ticks = if x { &mut ax.xticks } else { &mut ax.yticks };
                keep = false;
                match name {
                    "TickLabelFormat" => ticks.format = Some(v.text()?),
                    "TickValues" => ticks.values = Some(v.to_matrix()?.data),
                    "TickLabels" => ticks.labels = Some(texts(&v)?),
                    "TickLabelRotation" => ticks.angle = v.scalar()? as f32,
                    "Limits" => {
                        let l = limits_arg(&v)?;
                        if x { ax.xlim = l } else { ax.ylim = l }
                    }
                    "Scale" => {
                        let log = v.text()? == "log";
                        if x { ax.xlog = log } else { ax.ylog = log }
                    }
                    "FontSize" => ax.font_size = Some(v.scalar()? as f32),
                    "Label" if is_gfx(&v) => {}
                    _ => keep = true,
                }
            } else if p == Part::Legend && name == "Visible" && !on_off(&v)? {
                it.figures[f].axes[a].legend = None;
                keep = false;
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
        "matlab.graphics.chart.primitive.Line" => {
            let _ = (it, id);
            vec!["Color", "LineStyle", "LineWidth", "Marker", "MarkerSize", "MarkerFaceColor", "XData", "YData"]
        }
        "matlab.graphics.chart.primitive.Scatter" => vec!["Marker", "MarkerEdgeColor", "MarkerFaceColor", "SizeData", "LineWidth", "XData", "YData"],
        "matlab.graphics.chart.primitive.Bar" => vec!["BarLayout", "BarWidth", "FaceColor", "EdgeColor", "BaseValue", "XData", "YData"],
        "matlab.graphics.chart.primitive.ErrorBar" => vec!["Color", "LineStyle", "LineWidth", "Marker", "XData", "YData", "YNegativeDelta", "YPositiveDelta"],
        "matlab.graphics.primitive.Patch" => vec!["FaceColor", "FaceAlpha", "EdgeColor", "LineStyle", "Faces", "Vertices"],
        "matlab.graphics.axis.decorator.NumericRuler" => vec!["Limits", "Scale", "Exponent", "TickValues", "TickLabelFormat"],
        "matlab.graphics.illustration.Legend" => vec!["String", "Location", "Orientation", "FontSize", "Position", "Units"],
        c => props_of(c).iter().map(|p| p.0).filter(|n| !n.ends_with("Mode")).take(6).collect(),
    }
}

/// The text `name = h` shows.
pub fn display(it: &mut Interpreter, name: &str, v: &Value) -> R<String> {
    let ids = ids_of(v).unwrap_or_default();
    let class = match v {
        Value::Object(o) => o.class.name.clone(),
        _ => String::new(),
    };
    if ids.len() != 1 {
        let shape = if ids.is_empty() { "0×0".to_string() } else { format!("{}×1", ids.len()) };
        let names: String = ids.iter().map(|&id| format!("  {}\n", resolve(it, id).map(|t| short(class_of(it, t))).unwrap_or("GraphicsPlaceholder"))).collect();
        return Ok(format!("{name} = \n  {shape} {} array:\n\n{names}", short(&class)).trim_end().to_string());
    }
    let id = ids[0];
    let Some(t) = resolve(it, id) else {
        return Ok(format!("{name} = \n  handle to deleted {}", short(&class)));
    };
    let title = match t {
        Target::Root => "Graphics Root".to_string(),
        Target::Figure(f) => {
            let n = it.figures[f].id;
            match stored(it, id, "Name").map(|v| v.text().unwrap_or_default()).filter(|s| !s.is_empty()) {
                Some(nm) => format!("Figure ({n}: {nm})"),
                None => format!("Figure ({n})"),
            }
        }
        _ if class.ends_with(".Text") => {
            let s = get_prop(it, id, "String")?.text().unwrap_or_default();
            if s.is_empty() { "Text".to_string() } else { format!("Text ({s})") }
        }
        _ => short(&class).to_string(),
    };
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
fn descendants(it: &mut Interpreter, id: u64, out: &mut Vec<u64>) {
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
    for &id in ids {
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
            _ => {}
        }
        it.gfx_store.remove(&id);
    }
    Ok(())
}

/// `delete(h)` for graphics handles; `None` when the argument is not one.
pub fn delete(it: &mut Interpreter, a: &[Value]) -> Option<Ret> {
    let ids = ids_of(a.first()?)?;
    Some(delete_handles(it, &ids).and_then(|_| none()))
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    match name {
        "set" => set_builtin(it, a),
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
            let dims = v.dims();
            one(Value::Num(Matrix::from_dims(if ids_of(v).is_some() { vec![data.len(), 1] } else { dims }, data).cast_to(Kind::Logical)))
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
        "gobjects" => {
            let n = match a.first() {
                Some(v) => v.scalar()? as usize,
                None => 0,
            };
            let ids = vec![PLACEHOLDER; n];
            one(handles(it, &ids))
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
                *axes = Axes { id: axes.id, ..Axes::default() };
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
