//! Charts: `heatmap`, `boxchart`, `swarmchart`, `swarmchart3`, bubble charts (`bubblechart`,
//! `bubblechart3`, `polarbubblechart`, `bubblesize`, `bubblelim`, `bubblelegend`),
//! `binscatter`, `piechart`, `donutchart`, `bubblecloud`, `parallelplot`, `stackedplot`,
//! `scatterhistogram`, `wordcloud` and `geobubble`, data tips (`datatip`), plus `yyaxis` and
//! `datetick`.
//!
//! A chart is one graphics object that keeps its properties (in `gfx_store`) and is drawn
//! with parts (patches, lines, text) placed in its axes. Setting a property rebuilds the parts.

use crate::builtins::{arg, none, one, Ret};
use crate::builtins_plot::{apply_props, handles_out, select_axes};
use crate::error::{rt, LangError};
use crate::gfx::{self, Target, COLOR_ORDER};
use crate::graphics::{Axes, LineStyle, Marker, Series, SeriesKind, Ticks, XAxisKind, Yy, YySide};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "heatmap", "boxchart", "swarmchart", "swarmchart3", "bubblechart", "bubblechart3", "polarbubblechart", "bubblesize", "bubblelim", "bubblelegend",
    "binscatter", "piechart", "donutchart", "bubblecloud", "parallelplot", "stackedplot", "scatterhistogram", "wordcloud", "geobubble", "yyaxis", "datetick", "sortx", "sorty",
    "highlight", "labelnode", "labeledge", "layout", "datatip", "plotyy",
];

const HEATMAP: &str = "matlab.graphics.chart.HeatmapChart";
const BOX: &str = "matlab.graphics.chart.primitive.BoxChart";
const BUBBLE: &str = "matlab.graphics.chart.primitive.BubbleChart";
const BINSCATTER: &str = "matlab.graphics.chart.primitive.Binscatter";
const PIE: &str = "matlab.graphics.chart.PieChart";
const DONUT: &str = "matlab.graphics.chart.DonutChart";
const CLOUD: &str = "matlab.graphics.chart.BubbleCloud";
const PARALLEL: &str = "matlab.graphics.chart.ParallelCoordinatesPlot";
const STACKED: &str = "matlab.graphics.chart.StackedLineChart";
const SCATTERHIST: &str = "matlab.graphics.chart.ScatterHistogramChart";
const WORDCLOUD: &str = "matlab.graphics.chart.WordCloudChart";
const GEOBUBBLE: &str = "matlab.graphics.chart.GeographicBubbleChart";
const BUBBLELEGEND: &str = "matlab.graphics.illustration.BubbleLegend";
const GRAPHPLOT: &str = "matlab.graphics.chart.primitive.GraphPlot";
const DATATIP: &str = "matlab.graphics.datatip.DataTip";

const CHARTS: &[&str] = &[HEATMAP, BOX, BINSCATTER, PIE, DONUT, CLOUD, PARALLEL, STACKED, SCATTERHIST, WORDCLOUD, GEOBUBBLE, BUBBLELEGEND, GRAPHPLOT, DATATIP];

/// Hidden store keys: the parts a chart is drawn with, and bookkeeping flags.
const PARTS: &str = "\u{1}parts";
const AUTO_TITLE: &str = "\u{1}autotitle";
const MANUAL_CLIM: &str = "\u{1}clim";
const LEGEND_TITLE: &str = "\u{1}title";
const BUBBLE_SIZE: &str = "\u{1}bubblesize";
const BUBBLE_LIM: &str = "\u{1}bubblelim";

/// Whether objects of `class` are charts kept by this module.
pub fn is_chart(class: &str) -> bool {
    CHARTS.contains(&class)
}

/// Charts that are children of axes rather than of the figure.
pub fn in_axes(class: &str) -> bool {
    matches!(class, BOX | BINSCATTER | BUBBLELEGEND | GRAPHPLOT)
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "heatmap" => heatmap(it, a, nargout),
        "sortx" => sort_heatmap(it, a, true),
        "sorty" => sort_heatmap(it, a, false),
        "highlight" => highlight(it, a),
        "labelnode" => label_graph(it, a, true),
        "labeledge" => label_graph(it, a, false),
        "layout" => graph_layout(it, a),
        "datatip" => datatip(it, a, nargout),
        "boxchart" => boxchart(it, a, nargout),
        "swarmchart" => swarmchart(it, a, nargout, false),
        "swarmchart3" => swarmchart(it, a, nargout, true),
        "bubblechart" => bubblechart(it, a, nargout, 0),
        "bubblechart3" => bubblechart(it, a, nargout, 1),
        "polarbubblechart" => bubblechart(it, a, nargout, 2),
        "bubblesize" => bubble_setting(it, a, nargout, true),
        "bubblelim" => bubble_setting(it, a, nargout, false),
        "bubblelegend" => bubblelegend(it, a, nargout),
        "binscatter" => binscatter(it, a, nargout),
        "piechart" => piechart(it, a, nargout, false),
        "donutchart" => piechart(it, a, nargout, true),
        "bubblecloud" => bubblecloud(it, a, nargout),
        "parallelplot" => parallelplot(it, a, nargout),
        "stackedplot" => stackedplot(it, a, nargout),
        "scatterhistogram" => scatterhistogram(it, a, nargout),
        "wordcloud" => wordcloud(it, a, nargout),
        "geobubble" => geobubble(it, a, nargout),
        "yyaxis" => yyaxis(it, a),
        "plotyy" => plotyy(it, a, nargout),
        "datetick" => datetick(it, a),
        _ => Ok(None),
    }
}

// ---- values ----------------------------------------------------------------------------------

fn is_name(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.kind == Kind::Char && m.rows() <= 1) || matches!(v, Value::Str(s) if s.data.len() == 1)
}

fn is_numeric(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.kind != Kind::Char)
}

/// Name-value pairs.
fn name_values(a: &[Value]) -> R<Vec<(String, Value)>> {
    if a.len() % 2 != 0 {
        return rt("Invalid parameter/value pair arguments.");
    }
    a.chunks(2).map(|p| Ok((p[0].text()?, p[1].clone()))).collect()
}

/// Removes option `name` (any case) from `props`.
fn take(props: &mut Vec<(String, Value)>, name: &str) -> Option<Value> {
    let i = props.iter().position(|(k, _)| k.eq_ignore_ascii_case(name))?;
    Some(props.remove(i).1)
}

fn nums(v: &Value) -> Vec<f64> {
    v.to_matrix().map(|m| m.data).unwrap_or_default()
}

fn text(v: &Value) -> String {
    v.text().unwrap_or_default()
}

/// The elements of a text or numeric array as text.
fn texts(it: &mut Interpreter, v: &Value) -> R<Vec<String>> {
    Ok(match v {
        Value::Cell(c) => c.data.iter().map(|x| if x.numel() == 0 { Ok(String::new()) } else { x.text() }).collect::<R<_>>()?,
        Value::Str(s) => s.data.iter().map(|x| x.clone().unwrap_or_default()).collect(),
        Value::Num(m) if m.kind == Kind::Char => (0..m.rows()).map(|r| (0..m.cols()).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or(' ')).collect::<String>().trim_end().to_string()).collect(),
        Value::Num(m) => m.data.iter().map(|&x| crate::display::fmt_g(x, 5)).collect(),
        other => match it.call_function("string", vec![other.clone()], 1)?.pop() {
            Some(Value::Str(s)) => s.data.into_iter().map(|x| x.unwrap_or_default()).collect(),
            _ => vec![other.text()?],
        },
    })
}

fn cell_col(items: &[String]) -> Value {
    Value::Cell(CellArray::col(items.iter().map(|s| Value::chars(s)).collect()))
}

fn cell_row(items: &[String]) -> Value {
    Value::Cell(CellArray::row(items.iter().map(|s| Value::chars(s)).collect()))
}

fn str_row(items: &[String]) -> Value {
    Value::Str(StrArray::row(items.to_vec()))
}

fn row(v: Vec<f64>) -> Value {
    Value::Num(Matrix::row(v))
}

fn col(v: Vec<f64>) -> Value {
    Value::Num(Matrix::col(v))
}

/// An empty 1×0 double.
fn empty_row() -> Value {
    Value::Num(Matrix::new(1, 0, Vec::new()))
}

fn on(v: &Value) -> bool {
    matches!(text(v).to_lowercase().as_str(), "on" | "true") || matches!(v, Value::Num(m) if m.kind != Kind::Char && m.data.first() == Some(&1.0))
}

fn rgb_u8(c: &[f64]) -> Option<[u8; 3]> {
    (c.len() >= 3).then(|| [0, 1, 2].map(|k| (c[k].clamp(0.0, 1.0) * 255.0).round() as u8))
}

/// An RGB property (a triplet, or a color name / short name).
fn color_of(it: &mut Interpreter, v: &Value, fallback: [f64; 3]) -> [u8; 3] {
    if let Value::Num(m) = v {
        if m.kind != Kind::Char && m.numel() >= 3 {
            return rgb_u8(&m.data).unwrap();
        }
    }
    if v.is_text() {
        if let Ok(r) = it.call_function("validatecolor", vec![v.clone()], 1) {
            if let Some(c) = r.first().map(nums).and_then(|c| rgb_u8(&c)) {
                return c;
            }
        }
    }
    rgb_u8(&fallback).unwrap()
}

/// Rows of an n×3 color matrix.
fn color_rows(v: &Value) -> Vec<[f64; 3]> {
    match v.to_matrix() {
        Ok(m) if m.cols() == 3 => (0..m.rows()).map(|r| [m.get(r, 0), m.get(r, 1), m.get(r, 2)]).collect(),
        _ => COLOR_ORDER.to_vec(),
    }
}

/// Group numbers (0-based, `None` for missing) and group names of a grouping variable:
/// categories in order, sorted numbers or sorted text.
fn groups(it: &mut Interpreter, v: &Value) -> R<(Vec<Option<usize>>, Vec<String>)> {
    if crate::containers::is_categorical(v) {
        let cats = it.call_function("categories", vec![v.clone()], 1)?;
        let names = texts(it, &cats[0])?;
        let codes = nums(&it.call_function("double", vec![v.clone()], 1)?[0]);
        return Ok((codes.iter().map(|&c| (c.is_finite() && c >= 1.0).then(|| c as usize - 1)).collect(), names));
    }
    if is_numeric(v) {
        let data = nums(v);
        let mut u: Vec<f64> = data.iter().copied().filter(|x| x.is_finite()).collect();
        u.sort_by(|a, b| a.partial_cmp(b).unwrap());
        u.dedup();
        let names = u.iter().map(|&x| crate::display::fmt_g(x, 5)).collect();
        return Ok((data.iter().map(|x| u.iter().position(|y| y == x)).collect(), names));
    }
    let items = texts(it, v)?;
    let mut u: Vec<String> = items.iter().filter(|s| !s.is_empty()).cloned().collect();
    u.sort();
    u.dedup();
    Ok((items.iter().map(|s| u.iter().position(|y| y == s)).collect(), u))
}

/// A variable of a table chart's source table.
fn table_var(t: &crate::table::Table, name: &str) -> R<Value> {
    t.var(name).cloned().ok_or_else(|| LangError::new(format!("'{name}' is not a variable in the source table."), 0))
}

/// MATLAB's default quantile: linear interpolation with p(k) = (k - 0.5)/n.
fn quantile(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    let pos = (n as f64 * p + 0.5).clamp(1.0, n as f64);
    let lo = pos.floor() as usize;
    let f = pos - lo as f64;
    if lo >= n { sorted[n - 1] } else { sorted[lo - 1] + f * (sorted[lo] - sorted[lo - 1]) }
}

// ---- chart objects ---------------------------------------------------------------------------

/// The object a chart's properties live on: invisible text at no position.
fn anchor(class: &'static str) -> Series {
    let mut s = Series::new(SeriesKind::Text, vec![f64::NAN], vec![f64::NAN]);
    s.class_hint = Some(class);
    s.marker = None;
    s
}

/// A chart that replaces the current axes (or, with `own_figure`, all the figure's axes).
fn new_chart(it: &mut Interpreter, class: &'static str, own_figure: bool) -> u64 {
    let f = it.current_figure_index();
    let fig = &mut it.figures[f];
    let mut old: Vec<u64> = Vec::new();
    if own_figure {
        let keep = fig.current_axes;
        for (i, ax) in fig.axes.iter().enumerate() {
            if i != keep {
                old.extend(ax.series.iter().map(|s| s.id));
            }
        }
        let ax = fig.axes.swap_remove(keep);
        fig.axes = vec![ax];
        fig.grid = (1, 1);
        fig.current_axes = 0;
    }
    let a = fig.current_axes;
    let ax = &mut fig.axes[a];
    old.extend(ax.series.iter().map(|s| s.id));
    *ax = Axes { id: ax.id, overlay_of: ax.overlay_of, ..Axes::default() };
    for id in old {
        it.gfx_store.remove(&id);
    }
    let id = gfx::alloc(it);
    let mut s = anchor(class);
    s.id = id;
    it.figures[f].axes[a].series.push(s);
    id
}

/// A chart in the current axes (keeping its contents when held).
fn new_axes_chart(it: &mut Interpreter, class: &'static str) -> u64 {
    it.add_series(vec![anchor(class)], XAxisKind::Numeric)[0]
}

fn put(it: &mut Interpreter, id: u64, props: Vec<(&str, Value)>) {
    for (n, v) in props {
        gfx::store(it, id, n, v);
    }
}

fn get(it: &mut Interpreter, id: u64, name: &str) -> Value {
    gfx::get_prop_raw(it, id, name).unwrap_or_else(|_| Value::empty())
}

fn prop_texts(it: &mut Interpreter, id: u64, name: &str) -> R<Vec<String>> {
    let v = get(it, id, name);
    texts(it, &v)
}

fn prop_color(it: &mut Interpreter, id: u64, name: &str, fallback: [f64; 3]) -> [u8; 3] {
    let v = get(it, id, name);
    color_of(it, &v, fallback)
}

fn flag(it: &Interpreter, id: u64, name: &str) -> bool {
    gfx::stored(it, id, name).is_some_and(|v| v.scalar().unwrap_or(0.0) != 0.0)
}

fn class_at(it: &Interpreter, id: u64) -> Option<&'static str> {
    match gfx::resolve(it, id) {
        Some(Target::Series(f, a, s)) => it.figures[f].axes[a].series[s].class_hint,
        _ => None,
    }
}

fn locate(it: &Interpreter, id: u64) -> Option<(usize, usize)> {
    match gfx::resolve(it, id) {
        Some(Target::Series(f, a, _)) => Some((f, a)),
        _ => None,
    }
}

fn has_table(it: &Interpreter, id: u64) -> bool {
    matches!(gfx::stored(it, id, "SourceTable"), Some(Value::Table(_)))
}

/// Rebuilds the parts a chart is drawn with.
fn redraw(it: &mut Interpreter, id: u64) -> R<()> {
    let (Some((f, a)), Some(class)) = (locate(it, id), class_at(it, id)) else { return Ok(()) };
    let old: Vec<u64> = gfx::stored(it, id, PARTS).map(|v| nums(&v).iter().map(|&x| x as u64).collect()).unwrap_or_default();
    for ax in it.figures[f].axes.iter_mut() {
        ax.series.retain(|s| !old.contains(&s.id));
    }
    for pid in &old {
        it.gfx_store.remove(pid);
    }
    let parts = match class {
        HEATMAP => heatmap_parts(it, id, f, a)?,
        BOX => box_parts(it, id, f, a)?,
        BINSCATTER => binscatter_parts(it, id, f, a)?,
        PIE | DONUT => pie_parts(it, id, f, a, class == DONUT)?,
        CLOUD => cloud_parts(it, id, f, a)?,
        PARALLEL => parallel_parts(it, id, f, a)?,
        STACKED => stacked_parts(it, id, f)?,
        SCATTERHIST => scatterhist_parts(it, id, f)?,
        WORDCLOUD => wordcloud_parts(it, id, f, a)?,
        GEOBUBBLE => geobubble_parts(it, id, f, a)?,
        GRAPHPLOT => graph_parts(it, id, f, a)?,
        DATATIP => datatip_parts(it, id, a)?,
        _ => Vec::new(),
    };
    let mut ids = Vec::with_capacity(parts.len());
    for (ai, mut s) in parts {
        let pid = gfx::alloc(it);
        s.id = pid;
        ids.push(pid as f64);
        it.figures[f].axes[ai].series.push(s);
    }
    gfx::store(it, id, PARTS, row(ids));
    let fid = it.figures[f].id;
    it.mark_figure_dirty(fid);
    Ok(())
}

/// Sets a chart property (already validated by `set`) and redraws the chart.
pub fn set(it: &mut Interpreter, id: u64, name: &str, v: Value) -> R<()> {
    let class = class_at(it, id).unwrap_or("");
    let v = match (class, name) {
        (HEATMAP, "XData" | "YData" | "XDisplayData" | "YDisplayData" | "XDisplayLabels" | "YDisplayLabels") => {
            let t = texts(it, &v)?;
            cell_col(&t)
        }
        (PIE | DONUT, "Labels" | "Names") => {
            let t = texts(it, &v)?;
            str_row(&t)
        }
        (GRAPHPLOT, "NodeLabel" | "EdgeLabel") => {
            let t = if v.numel() == 0 { Vec::new() } else { texts(it, &v)? };
            if t.is_empty() { Value::Cell(CellArray::new(0, 0, Vec::new())) } else { cell_row(&t) }
        }
        (GRAPHPLOT, "NodeColor" | "EdgeColor") if matches!(&v, Value::Num(m) if m.kind == Kind::Char) || matches!(&v, Value::Str(_)) => match gfx::color_arg(&v)? {
            Some(c) => row(c.1.to_vec()),
            None => v,
        },
        (GRAPHPLOT, "Marker") => match &v {
            Value::Cell(c) => Value::Cell(CellArray::row(c.data.iter().map(|m| Value::chars(&marker_long(&text(m)))).collect())),
            _ => v,
        },
        _ => v,
    };
    gfx::store(it, id, name, v);
    match (class, name) {
        (HEATMAP, "ColorMethod" | "ColorVariable" | "XVariable" | "YVariable" | "SourceTable") if has_table(it, id) => {
            heatmap_from_table(it, id)?;
            heatmap_display(it, id);
        }
        (HEATMAP, "ColorData" | "XData" | "YData") => heatmap_display(it, id),
        (HEATMAP, "ColorLimits") => gfx::store(it, id, MANUAL_CLIM, Value::num(1.0)),
        (_, "Title") => gfx::store(it, id, AUTO_TITLE, Value::num(0.0)),
        (PIE | DONUT, "Labels") => gfx::store(it, id, "LabelsMode", Value::chars("manual")),
        (PIE | DONUT, "Names") => {
            gfx::store(it, id, "NamesMode", Value::chars("manual"));
            pie_labels(it, id)?;
        }
        (PIE | DONUT, "Data") => {
            pie_data(it, id)?;
            pie_labels(it, id)?;
        }
        (BINSCATTER, "NumBins") => {
            gfx::store(it, id, "NumBinsMode", Value::chars("manual"));
            binscatter_bins(it, id)?;
        }
        (DATATIP, "DataIndex") => {
            let k = v_scalar(it, id, "DataIndex");
            place_tip(it, id, k)?;
        }
        (DATATIP, "Location") => gfx::store(it, id, "LocationMode", Value::chars("manual")),
        _ => {}
    }
    redraw(it, id)
}

/// The properties `h = ...` shows for a chart.
pub fn short_props(class: &str, it: &Interpreter, id: u64) -> Vec<&'static str> {
    let table = has_table(it, id);
    match class {
        HEATMAP if table => vec!["SourceTable", "XVariable", "YVariable", "ColorVariable", "ColorMethod"],
        HEATMAP => vec!["XData", "YData", "ColorData"],
        BOX => vec!["XData", "YData"],
        DATATIP => vec!["DataIndex", "Location", "Parent"],
        BINSCATTER => vec!["NumBins", "XBinEdges", "YBinEdges", "Values", "XLimits", "YLimits", "FaceAlpha"],
        PIE | DONUT => vec!["ColorOrder", "FaceAlpha", "EdgeColor", "Labels", "Data", "Names"],
        CLOUD if table => vec!["SourceTable", "SizeVariable", "LabelVariable", "GroupVariable"],
        CLOUD => vec!["SizeData", "LabelData", "GroupData"],
        PARALLEL if table => vec!["SourceTable", "CoordinateVariables", "GroupVariable"],
        PARALLEL => vec!["Data", "CoordinateData", "GroupData"],
        STACKED if table => vec!["SourceTable", "DisplayVariables", "Color", "LineStyle", "LineWidth", "Marker", "MarkerSize"],
        STACKED => vec!["XData", "YData", "DisplayLabels", "Color", "LineStyle", "LineWidth", "Marker", "MarkerSize"],
        SCATTERHIST if table => vec!["SourceTable", "XVariable", "YVariable", "GroupVariable"],
        SCATTERHIST => vec!["XData", "YData", "GroupData"],
        WORDCLOUD if table => vec!["SourceTable", "WordVariable", "SizeVariable", "MaxDisplayWords"],
        WORDCLOUD => vec!["WordData", "SizeData", "MaxDisplayWords"],
        GRAPHPLOT => vec!["NodeColor", "MarkerSize", "Marker", "EdgeColor", "LineWidth", "LineStyle", "NodeLabel", "EdgeLabel", "XData", "YData", "ZData"],
        GEOBUBBLE => vec!["Basemap", "MapLayout", "LatitudeData", "LongitudeData", "SizeData", "ColorData"],
        _ => vec!["Location", "Style", "FontSize", "Position", "Units"],
    }
}

/// The header of a chart's display: its class, with its title in parentheses.
pub fn display_title(it: &mut Interpreter, id: u64, short: &str) -> String {
    let title = match class_at(it, id) {
        Some(DATATIP) => tip_label(it, id),
        Some(BUBBLELEGEND) => gfx::stored(it, id, LEGEND_TITLE).map(|v| text(&v)).unwrap_or_default(),
        _ => gfx::stored(it, id, "Title").map(|v| texts(it, &v).map(|t| t.join(" ")).unwrap_or_default()).unwrap_or_default(),
    };
    if title.is_empty() { short.to_string() } else { format!("{short} ({title})") }
}

/// Applies name-value options through `set` and returns the chart handle.
fn finish(it: &mut Interpreter, ids: &[u64], props: &[(String, Value)], nargout: usize) -> Ret {
    for &id in ids {
        redraw(it, id)?;
    }
    apply_props(it, ids, props)?;
    handles_out(it, ids, nargout)
}

/// A text label part.
fn label(x: f64, y: f64, s: &str, color: [u8; 3], size: Option<f32>) -> Series {
    let mut t = Series::new(SeriesKind::Text, vec![x], vec![y]);
    t.label = s.to_string();
    t.centered = true;
    t.color = Some(color);
    t.font_size = size;
    t.marker = None;
    t
}

/// A filled polygon part.
fn polygon(x: Vec<f64>, y: Vec<f64>, fill: [u8; 3], alpha: f32, edge: Option<[u8; 3]>) -> Series {
    let mut p = Series::new(SeriesKind::Patch, x, y);
    p.color = Some(fill);
    p.face_alpha = alpha;
    p.edge_color = edge;
    p.line_style = if edge.is_some() { LineStyle::Solid } else { LineStyle::None };
    p.marker = None;
    p
}

fn line(x: Vec<f64>, y: Vec<f64>, color: [u8; 3], width: f32) -> Series {
    let mut l = Series::new(SeriesKind::Line, x, y);
    l.color = Some(color);
    l.line_width = width;
    l.marker = None;
    l
}

/// Hidden axes with equal data units (pie charts, bubble clouds, word clouds).
fn bare_axes(ax: &mut Axes, xlim: (f64, f64), ylim: (f64, f64)) {
    ax.visible_axes = Some(false);
    ax.data_aspect = Some([1.0, 1.0, 1.0]);
    ax.xlim = Some(xlim);
    ax.ylim = Some(ylim);
}

// ---- heatmap ---------------------------------------------------------------------------------

fn heatmap(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = select_axes(it, a)?;
    let id;
    let mut props;
    if let Some(Value::Table(_)) = a.first() {
        let (xvar, yvar) = (arg(a, 1, "heatmap")?.text()?, arg(a, 2, "heatmap")?.text()?);
        props = name_values(&a[3..])?;
        let cvar = take(&mut props, "ColorVariable").map(|v| text(&v)).unwrap_or_default();
        let method = take(&mut props, "ColorMethod").map(|v| text(&v).to_lowercase()).unwrap_or_else(|| if cvar.is_empty() { "count".into() } else { "mean".into() });
        id = new_chart(it, HEATMAP, false);
        put(it, id, vec![
            ("SourceTable", a[0].clone()),
            ("XVariable", Value::chars(&xvar)),
            ("YVariable", Value::chars(&yvar)),
            ("ColorVariable", Value::chars(&cvar)),
            ("ColorMethod", Value::chars(&method)),
            ("XLabel", Value::chars(&xvar)),
            ("YLabel", Value::chars(&yvar)),
            (AUTO_TITLE, Value::num(1.0)),
        ]);
        heatmap_from_table(it, id)?;
    } else {
        let npos = if a.len() >= 3 && is_numeric(&a[2]) { 3 } else { 1 };
        let cdata = arg(a, npos - 1, "heatmap")?.to_matrix()?;
        let (m, n) = (cdata.rows(), cdata.cols());
        let (xd, yd) = if npos == 3 {
            (texts(it, &a[0])?, texts(it, &a[1])?)
        } else {
            ((1..=n).map(|k| k.to_string()).collect(), (1..=m).map(|k| k.to_string()).collect())
        };
        if xd.len() != n || yd.len() != m {
            return rt("Length of x-values and y-values must match the size of color data.");
        }
        props = name_values(&a[npos..])?;
        id = new_chart(it, HEATMAP, false);
        put(it, id, vec![("XData", cell_col(&xd)), ("YData", cell_col(&yd)), ("ColorData", Value::Num(Matrix::new(m, n, cdata.data))), ("ColorMethod", Value::chars("none"))]);
    }
    heatmap_display(it, id);
    finish(it, &[id], &props, nargout)
}

/// Color data of a table heatmap: the table rows grouped by the x and y variables.
fn heatmap_from_table(it: &mut Interpreter, id: u64) -> R<()> {
    let Some(Value::Table(t)) = gfx::stored(it, id, "SourceTable") else { return Ok(()) };
    let (xvar, yvar, cvar) = (text(&get(it, id, "XVariable")), text(&get(it, id, "YVariable")), text(&get(it, id, "ColorVariable")));
    let method = text(&get(it, id, "ColorMethod")).to_lowercase();
    let (xi, xnames) = groups(it, &table_var(&t, &xvar)?)?;
    let (yi, ynames) = groups(it, &table_var(&t, &yvar)?)?;
    let values = if cvar.is_empty() { vec![1.0; t.nrows] } else { nums(&table_var(&t, &cvar)?) };
    let (nx, ny) = (xnames.len(), ynames.len());
    let mut cells: Vec<Vec<f64>> = vec![Vec::new(); nx * ny];
    for k in 0..t.nrows {
        if let (Some(Some(x)), Some(Some(y))) = (xi.get(k), yi.get(k)) {
            cells[x * ny + y].push(values[k]);
        }
    }
    let data: Vec<f64> = cells.iter().map(|c| {
        let v: Vec<f64> = c.iter().copied().filter(|x| !x.is_nan()).collect();
        match method.as_str() {
            "count" => c.len() as f64,
            "sum" => v.iter().sum(),
            _ if v.is_empty() => f64::NAN,
            "mean" => v.iter().sum::<f64>() / v.len() as f64,
            "median" => {
                let mut s = v.clone();
                s.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let n = s.len();
                if n % 2 == 1 { s[n / 2] } else { (s[n / 2 - 1] + s[n / 2]) / 2.0 }
            }
            "min" => v.iter().copied().fold(f64::INFINITY, f64::min),
            "max" => v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            _ => *v.last().unwrap(),
        }
    }).collect();
    put(it, id, vec![("XData", cell_col(&xnames)), ("YData", cell_col(&ynames)), ("ColorData", Value::Num(Matrix::new(ny, nx, data)))]);
    if flag(it, id, AUTO_TITLE) {
        let what = match method.as_str() {
            "mean" => "Mean",
            "median" => "Median",
            "sum" => "Sum",
            "min" => "Minimum",
            "max" => "Maximum",
            _ => "",
        };
        let title = if method == "count" { format!("Count of {yvar} vs. {xvar}") } else if what.is_empty() { String::new() } else { format!("{what} of {cvar}") };
        gfx::store(it, id, "Title", Value::chars(&title));
    }
    Ok(())
}

/// The displayed data and labels, and automatic color limits, from the heatmap's data.
fn heatmap_display(it: &mut Interpreter, id: u64) {
    let (xd, yd, cd) = (get(it, id, "XData"), get(it, id, "YData"), get(it, id, "ColorData"));
    let ends = |v: &Value| match v {
        Value::Cell(c) if !c.data.is_empty() => Value::Cell(CellArray::row(vec![c.data[0].clone(), c.data[c.data.len() - 1].clone()])),
        _ => Value::Cell(CellArray::row(Vec::new())),
    };
    put(it, id, vec![
        ("XDisplayData", xd.clone()),
        ("YDisplayData", yd.clone()),
        ("XDisplayLabels", xd.clone()),
        ("YDisplayLabels", yd.clone()),
        ("XLimits", ends(&xd)),
        ("YLimits", ends(&yd)),
        ("ColorDisplayData", cd.clone()),
    ]);
    if !flag(it, id, MANUAL_CLIM) {
        let finite: Vec<f64> = nums(&cd).into_iter().filter(|x| x.is_finite()).collect();
        let (lo, hi) = (finite.iter().copied().fold(f64::INFINITY, f64::min), finite.iter().copied().fold(f64::NEG_INFINITY, f64::max));
        let lim = if !lo.is_finite() { vec![0.0, 1.0] } else if lo == hi { vec![lo - 0.5, hi + 0.5] } else { vec![lo, hi] };
        gfx::store(it, id, "ColorLimits", row(lim));
    }
}

fn heatmap_parts(it: &mut Interpreter, id: u64, f: usize, a: usize) -> R<Vec<(usize, Series)>> {
    let c = get(it, id, "ColorDisplayData").to_matrix()?;
    let (m, n) = (c.rows(), c.cols());
    let xl = prop_texts(it, id, "XDisplayLabels")?;
    let yl = prop_texts(it, id, "YDisplayLabels")?;
    let lim = nums(&get(it, id, "ColorLimits"));
    let map = color_rows(&get(it, id, "Colormap"));
    let fmt = text(&get(it, id, "CellLabelFormat"));
    let missing = text(&get(it, id, "MissingDataLabel"));
    let missing_color = prop_color(it, id, "MissingDataColor", [0.15, 0.15, 0.15]);
    let label_color = get(it, id, "CellLabelColor");
    let fixed_label = (text(&label_color) != "auto").then(|| color_of(it, &label_color, [0.0, 0.0, 0.0]));
    let (title, xlabel, ylabel) = (prop_texts(it, id, "Title")?.join("\n"), text(&get(it, id, "XLabel")), text(&get(it, id, "YLabel")));
    let colorbar = on(&get(it, id, "ColorbarVisible"));
    let (lo, hi) = (lim.first().copied().unwrap_or(0.0), lim.get(1).copied().unwrap_or(1.0));
    let ax = &mut it.figures[f].axes[a];
    ax.title = title;
    ax.xlabel = xlabel;
    ax.ylabel = ylabel;
    ax.y_reverse = true;
    ax.colormap = Some(map.clone());
    ax.clim = Some((lo, hi));
    ax.colorbar = colorbar;
    ax.grid = false;
    ax.xticks = Ticks { values: Some((1..=n).map(|k| k as f64).collect()), labels: Some(xl), ..Ticks::default() };
    ax.yticks = Ticks { values: Some((1..=m).map(|k| k as f64).collect()), labels: Some(yl), ..Ticks::default() };
    ax.xlim = Some((0.5, n as f64 + 0.5));
    ax.ylim = Some((0.5, m as f64 + 0.5));
    let mut parts = Vec::new();
    if m == 0 || n == 0 {
        return Ok(parts);
    }
    let mut img = Series::new(SeriesKind::Image, vec![1.0, n as f64], vec![1.0, m as f64]);
    img.grid = (m, n);
    img.channels = 1;
    img.c = c.data.clone();
    img.marker = None;
    parts.push((a, img));
    for j in 0..n {
        for i in 0..m {
            let v = c.get(i, j);
            let (x, y) = ((j + 1) as f64, (i + 1) as f64);
            if v.is_nan() {
                parts.push((a, polygon(vec![x - 0.5, x + 0.5, x + 0.5, x - 0.5], vec![y - 0.5, y - 0.5, y + 0.5, y + 0.5], missing_color, 1.0, None)));
            }
            let s = if v.is_nan() { missing.clone() } else { crate::format::sprintf(&fmt, &[Value::num(v)]).unwrap_or_default() };
            let color = fixed_label.unwrap_or_else(|| {
                let bg = if v.is_nan() { [0.15; 3] } else {
                    let t = if hi > lo { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.5 };
                    map[((t * (map.len() - 1) as f64).round() as usize).min(map.len() - 1)]
                };
                let lum = 0.299 * bg[0] + 0.587 * bg[1] + 0.114 * bg[2];
                if lum < 0.5 { [255, 255, 255] } else { [38, 38, 38] }
            });
            parts.push((a, label(x, y, &s, color, None)));
        }
    }
    Ok(parts)
}

// ---- boxchart --------------------------------------------------------------------------------

fn boxchart(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = select_axes(it, a)?;
    let npos = a.iter().position(is_name).unwrap_or(a.len()).min(2);
    let mut props = name_values(&a[npos..])?;
    let by_color = take(&mut props, "GroupByColor");
    let ydata = arg(a, npos.max(1) - 1, "boxchart")?.clone();
    let ym = ydata.to_matrix()?;
    let xdata = if npos >= 2 {
        a[0].clone()
    } else if ym.rows() == 1 || ym.cols() == 1 {
        // One box: MATLAB labels it with the category '1'.
        it.call_function("categorical", vec![Value::Num(Matrix::from_dims(ym.dims.clone(), vec![1.0; ym.numel()]))], 1)?.remove(0)
    } else {
        row((1..=ym.cols()).map(|k| k as f64).collect())
    };
    let existing = it.current_figure().axes_mut().series.iter().filter(|s| s.class_hint == Some(BOX)).count();
    let held = it.current_figure().axes_mut().hold;
    let mut ids = Vec::new();
    match by_color {
        Some(g) => {
            let (gi, names) = groups(it, &g)?;
            let xs = is_numeric(&xdata).then(|| nums(&xdata));
            for (k, name) in names.iter().enumerate() {
                let pick: Vec<usize> = (0..gi.len()).filter(|&i| gi[i] == Some(k)).collect();
                let yk: Vec<f64> = pick.iter().map(|&i| ym.data.get(i).copied().unwrap_or(f64::NAN)).collect();
                let xk = match &xs {
                    Some(x) if x.len() == gi.len() => col(pick.iter().map(|&i| x[i]).collect()),
                    _ => xdata.clone(),
                };
                let id = if k == 0 { new_axes_chart(it, BOX) } else { it.add_decoration(vec![anchor(BOX)])[0] };
                put(it, id, vec![("XData", xk), ("YData", col(yk)), ("DisplayName", Value::chars(name)), ("\u{1}dodge", row(vec![k as f64, names.len() as f64]))]);
                box_colors(it, id, k);
                ids.push(id);
            }
        }
        None => {
            let id = new_axes_chart(it, BOX);
            put(it, id, vec![("XData", xdata), ("YData", ydata)]);
            box_colors(it, id, if held { existing } else { 0 });
            ids.push(id);
        }
    }
    for &id in &ids {
        redraw(it, id)?;
    }
    apply_props(it, &ids, &props)?;
    if nargout == 0 {
        return none();
    }
    let v = gfx::handles(it, &ids);
    if let Value::Object(o) = &v {
        o.props.borrow_mut()[0] = col(ids.iter().map(|&i| i as f64).collect());
    }
    one(v)
}

fn box_colors(it: &mut Interpreter, id: u64, k: usize) {
    let v = row(COLOR_ORDER[k % COLOR_ORDER.len()].to_vec());
    put(it, id, vec![("BoxFaceColor", v.clone()), ("BoxEdgeColor", v.clone()), ("BoxMedianLineColor", v.clone()), ("MarkerColor", v), ("SeriesIndex", Value::num(k as f64 + 1.0))]);
}

fn box_parts(it: &mut Interpreter, id: u64, f: usize, a: usize) -> R<Vec<(usize, Series)>> {
    let (xv, yv) = (get(it, id, "XData"), get(it, id, "YData"));
    let ym = yv.to_matrix()?;
    // The y values of each box and its position along the category axis.
    let mut boxes: Vec<(f64, Vec<f64>)> = Vec::new();
    let mut cat_labels: Option<Vec<String>> = None;
    if is_numeric(&xv) && ym.rows() > 1 && ym.cols() > 1 && xv.numel() == ym.cols() {
        let xs = nums(&xv);
        for j in 0..ym.cols() {
            boxes.push((xs[j], (0..ym.rows()).map(|i| ym.get(i, j)).collect()));
        }
    } else {
        let pos: Vec<Option<f64>> = if crate::containers::is_categorical(&xv) {
            let (gi, names) = groups(it, &xv)?;
            cat_labels = Some(names);
            gi.iter().map(|g| g.map(|k| k as f64 + 1.0)).collect()
        } else {
            nums(&xv).into_iter().map(Some).collect()
        };
        for (i, &y) in ym.data.iter().enumerate() {
            // A scalar x places every value in one box.
            let p = if pos.len() == 1 { pos[0] } else { pos.get(i).copied().flatten() };
            let Some(p) = p else { continue };
            match boxes.iter_mut().find(|b| b.0 == p) {
                Some(b) => b.1.push(y),
                None => boxes.push((p, vec![y])),
            }
        }
    }
    boxes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let horizontal = text(&get(it, id, "Orientation")) == "horizontal";
    let spacing = boxes.windows(2).map(|w| w[1].0 - w[0].0).fold(1.0f64, f64::min).max(1e-9);
    let dodge = nums(&gfx::stored(it, id, "\u{1}dodge").unwrap_or_else(|| row(vec![0.0, 1.0])));
    let (slot, count) = (dodge[0], dodge[1].max(1.0));
    let width = get(it, id, "BoxWidth").scalar().unwrap_or(0.5) * spacing / count;
    let shift = (slot - (count - 1.0) / 2.0) * width;
    let face = prop_color(it, id, "BoxFaceColor", COLOR_ORDER[0]);
    let edge = prop_color(it, id, "BoxEdgeColor", COLOR_ORDER[0]);
    let median = prop_color(it, id, "BoxMedianLineColor", COLOR_ORDER[0]);
    let whisker = prop_color(it, id, "WhiskerLineColor", [0.1412; 3]);
    let marker_color = prop_color(it, id, "MarkerColor", COLOR_ORDER[0]);
    let alpha = get(it, id, "BoxFaceAlpha").scalar().unwrap_or(0.2) as f32;
    let lw = get(it, id, "LineWidth").scalar().unwrap_or(1.0) as f32;
    let marker = crate::graphics::parse_line_spec(&text(&get(it, id, "MarkerStyle"))).and_then(|p| p.2);
    let msize = get(it, id, "MarkerSize").scalar().unwrap_or(6.0) as f32;
    let orient = |p: Vec<f64>, v: Vec<f64>| if horizontal { (v, p) } else { (p, v) };
    let mut parts = Vec::new();
    for (p, vals) in &boxes {
        let mut v: Vec<f64> = vals.iter().copied().filter(|x| x.is_finite()).collect();
        if v.is_empty() {
            continue;
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let (q1, q2, q3) = (quantile(&v, 0.25), quantile(&v, 0.5), quantile(&v, 0.75));
        let iqr = q3 - q1;
        let (lo_f, hi_f) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
        let wlo = v.iter().copied().find(|&x| x >= lo_f).unwrap_or(q1);
        let whi = v.iter().rev().copied().find(|&x| x <= hi_f).unwrap_or(q3);
        let c = p + shift;
        let (x0, x1) = (c - width / 2.0, c + width / 2.0);
        let (bx, by) = orient(vec![x0, x1, x1, x0], vec![q1, q1, q3, q3]);
        parts.push((a, polygon(bx, by, face, alpha, Some(edge))));
        let (mx, my) = orient(vec![x0, x1], vec![q2, q2]);
        parts.push((a, line(mx, my, median, lw * 1.5)));
        for (from, to) in [(q3, whi), (q1, wlo)] {
            let (wx, wy) = orient(vec![c, c], vec![from, to]);
            parts.push((a, line(wx, wy, whisker, lw)));
        }
        let out: Vec<f64> = v.iter().copied().filter(|&x| x < lo_f || x > hi_f).collect();
        if !out.is_empty() {
            let (ox, oy) = orient(vec![c; out.len()], out);
            let mut s = Series::new(SeriesKind::Scatter, ox, oy);
            s.marker = marker.or(Some(Marker::Circle));
            s.marker_size = msize;
            s.color = Some(marker_color);
            parts.push((a, s));
        }
    }
    if let Some(names) = cat_labels {
        let ax = &mut it.figures[f].axes[a];
        let t = if horizontal { &mut ax.yticks } else { &mut ax.xticks };
        t.values = Some((1..=names.len()).map(|k| k as f64).collect());
        t.labels = Some(names);
    }
    Ok(parts)
}

// ---- swarmchart ------------------------------------------------------------------------------

fn swarmchart(it: &mut Interpreter, a: &[Value], nargout: usize, three: bool) -> Ret {
    let r = it.call_function(if three { "scatter3" } else { "scatter" }, a.to_vec(), 1)?;
    let ids = r.first().and_then(gfx::ids_of).unwrap_or_default();
    for &id in &ids {
        gfx::store(it, id, "XJitter", Value::chars("density"));
        if three {
            gfx::store(it, id, "YJitter", Value::chars("density"));
        }
        let Some(Target::Series(f, ai, si)) = gfx::resolve(it, id) else { continue };
        let s = &mut it.figures[f].axes[ai].series[si];
        let z = if three { s.z.clone() } else { Vec::new() };
        let (dx, dy) = jitter(&s.x, &s.y, &z, 0.9);
        s.dx = dx;
        s.dy = dy;
    }
    handles_out(it, &ids, nargout)
}

/// Swarm offsets: points sharing a position spread across `width` of the spacing, in
/// proportion to the density of their values (`'density'` jitter).
fn jitter(x: &[f64], y: &[f64], z: &[f64], width: f64) -> (Vec<f64>, Vec<f64>) {
    let three = !z.is_empty();
    let vals: &[f64] = if three { z } else { y };
    let key = |i: usize| if three { (x[i], y[i]) } else { (x[i], 0.0) };
    let mut keys: Vec<(f64, f64)> = Vec::new();
    for i in 0..x.len() {
        if !keys.contains(&key(i)) {
            keys.push(key(i));
        }
    }
    let spacing = |v: &[f64]| {
        let mut v: Vec<f64> = v.iter().copied().filter(|a| a.is_finite()).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.dedup();
        let s = v.windows(2).map(|w| w[1] - w[0]).fold(f64::INFINITY, f64::min);
        if s.is_finite() { s } else { 1.0 }
    };
    let (sx, sy) = (spacing(x), if three { spacing(y) } else { 1.0 });
    let mut density = vec![0.0; x.len()];
    let mut rank = vec![0usize; x.len()];
    for k in &keys {
        let mut idx: Vec<usize> = (0..x.len()).filter(|&i| key(i) == *k).collect();
        let v: Vec<f64> = idx.iter().map(|&i| vals[i]).collect();
        let n = v.len() as f64;
        let mean = v.iter().sum::<f64>() / n;
        let sd = (v.iter().map(|a| (a - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
        let h = if sd > 0.0 { 1.06 * sd * n.powf(-0.2) } else { 1.0 };
        for &i in &idx {
            density[i] = v.iter().map(|&b| (-0.5 * ((vals[i] - b) / h).powi(2)).exp()).sum::<f64>();
        }
        idx.sort_by(|&a, &b| vals[a].partial_cmp(&vals[b]).unwrap_or(std::cmp::Ordering::Equal));
        for (r, &i) in idx.iter().enumerate() {
            rank[i] = r;
        }
    }
    let top = density.iter().copied().fold(0.0, f64::max).max(1e-12);
    // A low-discrepancy spread in [-1, 1] by rank, scaled by the relative density.
    let spread = |i: usize, phase: f64| (((rank[i] as f64 + 1.0) * 0.618_033_988_75 + phase).fract() * 2.0 - 1.0) * density[i] / top;
    let dx = (0..x.len()).map(|i| spread(i, 0.0) * width * sx / 2.0).collect();
    let dy = if three { (0..x.len()).map(|i| spread(i, 0.5) * width * sy / 2.0).collect() } else { Vec::new() };
    (dx, dy)
}

// ---- bubble charts ---------------------------------------------------------------------------

fn bubblechart(it: &mut Interpreter, a: &[Value], nargout: usize, kind: usize) -> Ret {
    let a = select_axes(it, a)?;
    let ncoord = if kind == 1 { 3 } else { 2 };
    let mut npos = a.iter().position(is_name).unwrap_or(a.len());
    if npos < ncoord + 1 {
        return rt("Not enough input arguments.");
    }
    // A color name or spec right after the sizes, followed by Name,Value pairs ('r', 'MarkerFaceAlpha', 0.5).
    if npos == ncoord + 1 && a.len() > npos && (a.len() - npos - 1) % 2 == 0 {
        npos += 1;
    }
    let sz = a[ncoord].to_matrix()?;
    // Drawn as filled circles sized from SizeData; a color, when given, goes to scatter.
    let mut args: Vec<Value> = a[..ncoord].to_vec();
    if npos > ncoord + 1 {
        args.push(Value::num(36.0));
        args.push(a[ncoord + 1].clone());
    }
    let props = name_values(&a[npos..])?;
    let r = it.call_function(["scatter", "scatter3", "polarscatter"][kind], args, 1)?;
    let ids = r.first().and_then(gfx::ids_of).unwrap_or_default();
    for &id in &ids {
        let mut n = 1;
        if let Some(Target::Series(f, ai, si)) = gfx::resolve(it, id) {
            let s = &mut it.figures[f].axes[ai].series[si];
            s.class_hint = Some(BUBBLE);
            n = s.x.len();
        }
        let sizes = if sz.numel() == 1 { Value::Num(Matrix::new(1, n, vec![sz.data[0]; n])) } else { Value::Num(sz.clone()) };
        put(it, id, vec![("SizeData", sizes), ("MarkerFaceAlpha", Value::num(0.6)), ("MarkerFaceColor", Value::chars("flat")), ("MarkerEdgeColor", Value::chars("flat"))]);
    }
    if let Some((f, ai)) = ids.first().and_then(|&id| locate(it, id)) {
        refresh_bubbles(it, f, ai);
    }
    apply_props(it, &ids, &props)?;
    handles_out(it, &ids, nargout)
}

/// The bubble size range (diameters in points) and size limits of axes.
fn bubble_scale(it: &mut Interpreter, f: usize, a: usize) -> ([f64; 2], [f64; 2]) {
    let ax_id = gfx::axes_id(it, f, a);
    let pair = |v: Option<Value>| v.map(|v| nums(&v)).filter(|v| v.len() == 2).map(|v| [v[0], v[1]]);
    let range = pair(gfx::stored(it, ax_id, BUBBLE_SIZE)).unwrap_or([3.0, 50.0]);
    if let Some(l) = pair(gfx::stored(it, ax_id, BUBBLE_LIM)) {
        return (range, l);
    }
    let ids: Vec<u64> = it.figures[f].axes[a].series.iter().filter(|s| s.class_hint == Some(BUBBLE)).map(|s| s.id).collect();
    let all: Vec<f64> = ids.iter().flat_map(|&id| gfx::stored(it, id, "SizeData").map(|v| nums(&v)).unwrap_or_default()).filter(|x| x.is_finite()).collect();
    let lo = all.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (range, if lo.is_finite() { [lo, hi] } else { [0.0, 1.0] })
}

/// Marker diameters of the bubbles in axes: their sizes mapped by area onto the size range.
fn refresh_bubbles(it: &mut Interpreter, f: usize, a: usize) {
    let (range, lim) = bubble_scale(it, f, a);
    let ids: Vec<u64> = it.figures[f].axes[a].series.iter().filter(|s| s.class_hint == Some(BUBBLE)).map(|s| s.id).collect();
    for id in ids {
        let sizes = gfx::stored(it, id, "SizeData").map(|v| nums(&v)).unwrap_or_default();
        let alpha = gfx::stored(it, id, "MarkerFaceAlpha").and_then(|v| v.scalar().ok()).unwrap_or(0.6) as f32;
        let d: Vec<f32> = sizes.iter().map(|&v| {
            let t = if lim[1] > lim[0] { ((v - lim[0]) / (lim[1] - lim[0])).clamp(0.0, 1.0) } else { 1.0 };
            (range[0].powi(2) + t * (range[1].powi(2) - range[0].powi(2))).sqrt() as f32
        }).collect();
        if let Some(Target::Series(f, ai, si)) = gfx::resolve(it, id) {
            let s = &mut it.figures[f].axes[ai].series[si];
            s.sizes = d;
            s.face_alpha = alpha;
        }
    }
    let fid = it.figures[f].id;
    it.mark_figure_dirty(fid);
}

/// `bubblesize` (size range) and `bubblelim` (size limits): query or set.
fn bubble_setting(it: &mut Interpreter, a: &[Value], nargout: usize, size: bool) -> Ret {
    let a = select_axes(it, a)?;
    let f = it.current_figure_index();
    let ai = it.figures[f].current_axes;
    let ax_id = gfx::axes_id(it, f, ai);
    let key = if size { BUBBLE_SIZE } else { BUBBLE_LIM };
    let current = |it: &mut Interpreter| {
        let (range, lim) = bubble_scale(it, f, ai);
        row(if size { range.to_vec() } else { lim.to_vec() })
    };
    let Some(v) = a.first() else { return one(current(it)) };
    match text(v).to_lowercase().as_str() {
        "auto" if !size => {
            it.gfx_store.entry(ax_id).or_default().retain(|(k, _)| k != key);
        }
        "manual" if !size => {
            let lim = current(it);
            gfx::store(it, ax_id, key, lim);
        }
        _ => {
            let m = v.to_matrix()?;
            if m.numel() != 2 || !(m.data[1] > m.data[0]) {
                return rt("Value must be a 2-element vector of increasing values.");
            }
            gfx::store(it, ax_id, key, row(m.data));
        }
    }
    refresh_bubbles(it, f, ai);
    if nargout > 0 {
        return one(current(it));
    }
    none()
}

fn bubblelegend(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = select_axes(it, a)?;
    let (title, rest) = if a.len() % 2 == 1 { (texts(it, &a[0])?.join("\n"), &a[1..]) } else { (String::new(), a) };
    let props = name_values(rest)?;
    let id = it.add_decoration(vec![anchor(BUBBLELEGEND)])[0];
    let mut t = Series::new(SeriesKind::Text, vec![f64::NAN], vec![f64::NAN]);
    t.label = title.clone();
    t.visible = false;
    t.marker = None;
    let tid = it.add_decoration(vec![t])[0];
    let th = gfx::handle(it, tid);
    put(it, id, vec![
        ("Title", th),
        (LEGEND_TITLE, Value::chars(&title)),
        ("NumBubbles", Value::num(3.0)),
        ("Style", Value::chars("vertical")),
        ("Location", Value::chars("northeast")),
        ("LimitLabels", cell_col(&[String::new(), String::new(), String::new()])),
    ]);
    apply_props(it, &[id], &props)?;
    handles_out(it, &[id], nargout)
}

// ---- binscatter ------------------------------------------------------------------------------

fn binscatter(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = select_axes(it, a)?;
    let npos = a.iter().position(is_name).unwrap_or(a.len());
    let (x, y) = (arg(a, 0, "binscatter")?.to_matrix()?, arg(a, 1, "binscatter")?.to_matrix()?);
    if x.numel() != y.numel() {
        return rt("X and Y must be the same size.");
    }
    let mut props = name_values(&a[npos.max(2)..])?;
    let bins = if npos >= 3 { Some(a[2].clone()) } else { take(&mut props, "NumBins") };
    let id = new_axes_chart(it, BINSCATTER);
    let nb = match &bins {
        Some(b) => {
            let m = nums(b);
            if m.len() == 1 { vec![m[0], m[0]] } else { m }
        }
        None => {
            let k = (x.numel() as f64).cbrt().floor().max(1.0);
            vec![k, k]
        }
    };
    put(it, id, vec![
        ("XData", Value::Num(x)),
        ("YData", Value::Num(y)),
        ("NumBins", row(nb)),
        ("NumBinsMode", Value::chars(if bins.is_some() { "manual" } else { "auto" })),
    ]);
    binscatter_bins(it, id)?;
    finish(it, &[id], &props, nargout)
}

/// Edges from the smallest value over `n` equal bins that just include the largest one.
fn bin_edges(v: &[f64], n: usize) -> Vec<f64> {
    let finite: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    let (mut lo, mut hi) = (finite.iter().copied().fold(f64::INFINITY, f64::min), finite.iter().copied().fold(f64::NEG_INFINITY, f64::max));
    if !lo.is_finite() {
        (lo, hi) = (0.0, 1.0);
    } else if lo == hi {
        (lo, hi) = (lo - 0.5, hi + 0.5);
    }
    let w = (hi - lo) / n as f64 * (1.0 + 1e-4);
    (0..=n).map(|k| lo + k as f64 * w).collect()
}

fn binscatter_bins(it: &mut Interpreter, id: u64) -> R<()> {
    let (x, y) = (nums(&get(it, id, "XData")), nums(&get(it, id, "YData")));
    let nb = nums(&get(it, id, "NumBins"));
    let (nx, ny) = (nb.first().copied().unwrap_or(1.0).max(1.0) as usize, nb.get(1).copied().unwrap_or(1.0).max(1.0) as usize);
    let (ex, ey) = (bin_edges(&x, nx), bin_edges(&y, ny));
    let find = |e: &[f64], v: f64| (v.is_finite() && v >= e[0] && v <= e[e.len() - 1]).then(|| e.windows(2).position(|w| v < w[1]).unwrap_or(e.len() - 2));
    let mut values = vec![0.0; nx * ny];
    for (&a, &b) in x.iter().zip(&y) {
        if let (Some(i), Some(j)) = (find(&ex, a), find(&ey, b)) {
            values[j * nx + i] += 1.0;
        }
    }
    put(it, id, vec![
        ("Values", Value::Num(Matrix::new(nx, ny, values))),
        ("XLimits", row(vec![ex[0], ex[nx]])),
        ("YLimits", row(vec![ey[0], ey[ny]])),
        ("XBinEdges", row(ex)),
        ("YBinEdges", row(ey)),
    ]);
    Ok(())
}

fn binscatter_parts(it: &mut Interpreter, id: u64, f: usize, a: usize) -> R<Vec<(usize, Series)>> {
    let v = get(it, id, "Values").to_matrix()?;
    let (ex, ey) = (nums(&get(it, id, "XBinEdges")), nums(&get(it, id, "YBinEdges")));
    let alpha = get(it, id, "FaceAlpha").scalar().unwrap_or(1.0) as f32;
    let show_empty = on(&get(it, id, "ShowEmptyBins"));
    let top = v.data.iter().copied().fold(0.0, f64::max).max(1.0);
    let ax = &mut it.figures[f].axes[a];
    let map = ax.colormap.clone().unwrap_or_else(|| crate::graphics::colormap("parula", 256).unwrap_or_default());
    ax.colorbar = true;
    ax.clim = Some((0.0, top));
    let mut parts = Vec::new();
    for j in 0..v.cols() {
        for i in 0..v.rows() {
            let c = v.get(i, j);
            if c == 0.0 && !show_empty {
                continue;
            }
            let k = ((c / top) * (map.len() - 1) as f64).round() as usize;
            let fill = rgb_u8(&map[k.min(map.len() - 1)]).unwrap();
            let (x0, x1, y0, y1) = (ex[i], ex[i + 1], ey[j], ey[j + 1]);
            parts.push((a, polygon(vec![x0, x1, x1, x0], vec![y0, y0, y1, y1], fill, alpha, None)));
        }
    }
    Ok(parts)
}

// ---- piechart, donutchart --------------------------------------------------------------------

fn piechart(it: &mut Interpreter, a: &[Value], nargout: usize, donut: bool) -> Ret {
    let a = select_axes(it, a)?;
    let mut put_later: Vec<(&str, Value)> = Vec::new();
    let (data, names, rest) = if let Some(Value::Table(t)) = a.first() {
        let dvar = arg(a, 1, "piechart")?.text()?;
        let nvar = (a.len() % 2 == 1 && a.len() >= 3).then(|| a[2].text()).transpose()?;
        let data = table_var(t, &dvar)?;
        let names = nvar.as_ref().map(|n| table_var(t, n).and_then(|v| texts(it, &v))).transpose()?;
        put_later.extend([("SourceTable", a[0].clone()), ("DataVariable", Value::chars(&dvar)), ("NamesVariable", Value::chars(nvar.as_deref().unwrap_or("")))]);
        (data, names, &a[if nvar.is_some() { 3 } else { 2 }..])
    } else {
        let data = arg(a, 0, "piechart")?.clone();
        let positional_names = a.len() >= 2 && (a.len() - 2) % 2 == 0 && matches!(&a[1], Value::Cell(_) | Value::Str(_)) && !is_name(&a[1]);
        let names = if positional_names { Some(texts(it, &a[1])?) } else { None };
        (data, names, &a[if positional_names { 2 } else { 1 }..])
    };
    let props = name_values(rest)?;
    let data = if crate::containers::is_categorical(&data) {
        data
    } else {
        let m = data.to_matrix()?;
        if m.data.iter().any(|&v| v < 0.0) {
            return rt("Data must be nonnegative.");
        }
        row(m.data)
    };
    let id = new_chart(it, if donut { DONUT } else { PIE }, false);
    put(it, id, put_later);
    put(it, id, vec![("Data", data), ("LabelsMode", Value::chars("auto")), ("NamesMode", Value::chars(if names.is_some() { "manual" } else { "auto" })), ("ExplodedWedges", empty_row())]);
    if let Some(n) = &names {
        gfx::store(it, id, "Names", str_row(n));
    }
    pie_data(it, id)?;
    pie_labels(it, id)?;
    finish(it, &[id], &props, nargout)
}

/// Proportions (and category counts) of a pie chart's data, and its automatic names.
fn pie_data(it: &mut Interpreter, id: u64) -> R<()> {
    let d = get(it, id, "Data");
    let categorical = crate::containers::is_categorical(&d);
    let (vals, counts) = if categorical {
        let c = nums(&it.call_function("countcats", vec![d.clone()], 1)?[0]);
        (c.clone(), row(c))
    } else {
        (nums(&d), empty_row())
    };
    if text(&get(it, id, "NamesMode")) == "auto" {
        let names: Vec<String> = if categorical {
            let cats = it.call_function("categories", vec![d.clone()], 1)?;
            texts(it, &cats[0])?
        } else {
            (1..=vals.len()).map(|k| format!("data{k}")).collect()
        };
        gfx::store(it, id, "Names", str_row(&names));
    }
    let total: f64 = vals.iter().filter(|v| v.is_finite()).sum();
    let props: Vec<f64> = vals.iter().map(|&v| if total > 0.0 && total < 1.0 { v } else if total > 0.0 { v / total } else { 0.0 }).collect();
    put(it, id, vec![("Proportions", row(props)), ("CategoryCounts", counts), ("\u{1}categorical", Value::num(categorical as u8 as f64))]);
    Ok(())
}

/// Automatic wedge labels: percentages, after the names when names were given.
fn pie_labels(it: &mut Interpreter, id: u64) -> R<()> {
    if text(&get(it, id, "LabelsMode")) == "manual" {
        return Ok(());
    }
    let named = text(&get(it, id, "NamesMode")) == "manual" || flag(it, id, "\u{1}categorical");
    let names = prop_texts(it, id, "Names")?;
    let labels: Vec<String> = nums(&get(it, id, "Proportions")).iter().enumerate().map(|(k, &p)| {
        let pct = format!("{}%", crate::format::sprintf("%.3g", &[Value::num(100.0 * p)]).unwrap_or_default());
        if named { format!("{} ({pct})", names.get(k).cloned().unwrap_or_default()) } else { pct }
    }).collect();
    gfx::store(it, id, "Labels", str_row(&labels));
    Ok(())
}

fn pie_parts(it: &mut Interpreter, id: u64, f: usize, a: usize, donut: bool) -> R<Vec<(usize, Series)>> {
    let props = nums(&get(it, id, "Proportions"));
    let labels = prop_texts(it, id, "Labels")?;
    let colors = color_rows(&get(it, id, "ColorOrder"));
    let alpha = get(it, id, "FaceAlpha").scalar().unwrap_or(0.6) as f32;
    let edge = prop_color(it, id, "EdgeColor", [0.15; 3]);
    let font = prop_color(it, id, "FontColor", [0.15; 3]);
    let start = get(it, id, "StartAngle").scalar().unwrap_or(0.0);
    let sign = if text(&get(it, id, "Direction")) == "counterclockwise" { 1.0 } else { -1.0 };
    let exploded = get(it, id, "ExplodedWedges");
    let is_exploded = |k: usize| match &exploded {
        Value::Num(m) if m.kind == Kind::Logical => m.data.get(k).is_some_and(|&v| v != 0.0),
        v => nums(v).contains(&(k as f64 + 1.0)),
    };
    let inner = if donut { get(it, id, "InnerRadius").scalar().unwrap_or(0.6) } else { 0.0 };
    let title = prop_texts(it, id, "Title")?.join("\n");
    let ax = &mut it.figures[f].axes[a];
    bare_axes(ax, (-1.45, 1.45), (-1.3, 1.3));
    ax.title = title;
    let mut parts = Vec::new();
    let mut t = 90.0 - start;
    for (k, &p) in props.iter().enumerate() {
        let span = p * 360.0;
        let (a0, a1) = (t, t + sign * span);
        t = a1;
        let mid = ((a0 + a1) / 2.0).to_radians();
        let (ox, oy) = if is_exploded(k) { (0.1 * mid.cos(), 0.1 * mid.sin()) } else { (0.0, 0.0) };
        let steps = ((span / 3.0).ceil() as usize).max(2);
        let arc = |r: f64| -> Vec<(f64, f64)> { (0..=steps).map(|i| (a0 + (a1 - a0) * i as f64 / steps as f64).to_radians()).map(|th| (ox + r * th.cos(), oy + r * th.sin())).collect() };
        let outer = arc(1.0);
        if span > 0.0 {
            let fill = rgb_u8(&colors[k % colors.len()]).unwrap();
            let xs = |p: &[(f64, f64)]| p.iter().map(|q| q.0).collect::<Vec<f64>>();
            let ys = |p: &[(f64, f64)]| p.iter().map(|q| q.1).collect::<Vec<f64>>();
            if inner > 0.0 {
                // A ring wedge is not convex: fill it as quads (NaN-separated faces), then outline it.
                let ring = arc(inner);
                let (mut fx, mut fy) = (Vec::new(), Vec::new());
                for i in 0..steps {
                    for q in [outer[i], outer[i + 1], ring[i + 1], ring[i]] {
                        fx.push(q.0);
                        fy.push(q.1);
                    }
                    fx.push(f64::NAN);
                    fy.push(f64::NAN);
                }
                parts.push((a, polygon(fx, fy, fill, alpha, None)));
                let mut edge_pts = outer.clone();
                edge_pts.extend(ring.iter().rev());
                edge_pts.push(outer[0]);
                parts.push((a, line(xs(&edge_pts), ys(&edge_pts), edge, 0.5)));
            } else {
                let mut pts = outer.clone();
                pts.push((ox, oy));
                parts.push((a, polygon(xs(&pts), ys(&pts), fill, alpha, Some(edge))));
            }
        }
        if let Some(l) = labels.get(k) {
            // Outside the wedge, pushed out by half the label's width at the sides.
            let half = 0.028 * l.chars().count() as f64;
            parts.push((a, label(ox + 1.12 * mid.cos() + mid.cos() * half, oy + 1.12 * mid.sin(), l, font, None)));
        }
    }
    Ok(parts)
}

// ---- bubblecloud -----------------------------------------------------------------------------

fn bubblecloud(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = select_axes(it, a)?;
    let mut later: Vec<(&str, Value)> = Vec::new();
    let (sizes, labels, groups_v, rest) = if let Some(Value::Table(t)) = a.first() {
        let mut vars = Vec::new();
        let mut i = 1;
        while i < a.len() && vars.len() < 3 && is_name(&a[i]) && t.names.contains(&text(&a[i])) {
            vars.push(text(&a[i]));
            i += 1;
        }
        let var = |k: usize| vars.get(k).map(|n| table_var(t, n)).transpose();
        let sizes = var(0)?.ok_or_else(|| LangError::new("Specify the size variable.", 0))?;
        let (labels, groups_v) = (var(1)?, var(2)?);
        later.extend([
            ("SourceTable", a[0].clone()),
            ("SizeVariable", Value::chars(vars.first().map(String::as_str).unwrap_or(""))),
            ("LabelVariable", Value::chars(vars.get(1).map(String::as_str).unwrap_or(""))),
            ("GroupVariable", Value::chars(vars.get(2).map(String::as_str).unwrap_or(""))),
        ]);
        (sizes, labels, groups_v, &a[i..])
    } else {
        let npos = a.iter().position(is_name).unwrap_or(a.len()).min(3);
        (arg(a, 0, "bubblecloud")?.clone(), a.get(1).filter(|_| npos > 1).cloned(), a.get(2).filter(|_| npos > 2).cloned(), &a[npos..])
    };
    let props = name_values(rest)?;
    let sz = nums(&sizes);
    let as_row = |v: Value| match v {
        Value::Cell(c) => Value::Cell(CellArray::row(c.data)),
        Value::Str(s) => Value::Str(StrArray::from_dims(vec![1, s.data.len()], s.data)),
        Value::Num(m) if m.kind != Kind::Char => Value::Num(Matrix::row(m.data)),
        other => other,
    };
    let id = new_chart(it, CLOUD, false);
    put(it, id, later);
    put(it, id, vec![
        ("SizeData", row(sz.clone())),
        ("LabelData", labels.map(as_row).unwrap_or_else(|| Value::Cell(CellArray::row(Vec::new())))),
        ("GroupData", groups_v.map(as_row).unwrap_or_else(empty_row)),
    ]);
    finish(it, &[id], &props, nargout)
}

/// Circles of the given radii packed around the origin, largest first.
fn pack_circles(r: &[f64]) -> Vec<(f64, f64)> {
    let mut order: Vec<usize> = (0..r.len()).collect();
    order.sort_by(|&a, &b| r[b].partial_cmp(&r[a]).unwrap_or(std::cmp::Ordering::Equal));
    let mut at = vec![(0.0, 0.0); r.len()];
    let mut placed: Vec<usize> = Vec::new();
    let gap = 0.02;
    for &i in &order {
        if placed.is_empty() {
            placed.push(i);
            continue;
        }
        let mut best: Option<((f64, f64), f64)> = None;
        for &j in &placed {
            for k in 0..72 {
                let th = k as f64 * std::f64::consts::PI / 36.0;
                let d = r[i] + r[j] + gap;
                let c = (at[j].0 + d * th.cos(), at[j].1 + d * th.sin());
                let free = placed.iter().all(|&q| ((c.0 - at[q].0).powi(2) + (c.1 - at[q].1).powi(2)).sqrt() >= r[i] + r[q] + gap - 1e-9);
                let score = c.0.hypot(c.1 * 1.3);
                if free && best.map_or(true, |b| score < b.1) {
                    best = Some((c, score));
                }
            }
        }
        at[i] = best.map(|b| b.0).unwrap_or((0.0, 0.0));
        placed.push(i);
    }
    at
}

fn cloud_parts(it: &mut Interpreter, id: u64, f: usize, a: usize) -> R<Vec<(usize, Series)>> {
    let sz = nums(&get(it, id, "SizeData"));
    let labels = prop_texts(it, id, "LabelData")?;
    let gv = get(it, id, "GroupData");
    let gi = if gv.numel() > 0 { groups(it, &gv)?.0 } else { vec![Some(0); sz.len()] };
    let colors = color_rows(&get(it, id, "ColorOrder"));
    let alpha = get(it, id, "FaceAlpha").scalar().unwrap_or(0.6) as f32;
    let edge = prop_color(it, id, "EdgeColor", [0.0; 3]);
    let font = prop_color(it, id, "FontColor", [0.0; 3]);
    let title = prop_texts(it, id, "Title")?.join("\n");
    let top = sz.iter().copied().filter(|v| v.is_finite()).fold(0.0, f64::max).max(1e-12);
    let radii: Vec<f64> = sz.iter().map(|&s| (s.max(0.0) / top).sqrt()).collect();
    let at = pack_circles(&radii);
    let mut parts = Vec::new();
    let (mut x0, mut x1, mut y0, mut y1) = (-1.0f64, 1.0f64, -1.0f64, 1.0f64);
    for (k, &r) in radii.iter().enumerate() {
        let (cx, cy) = at[k];
        (x0, x1, y0, y1) = (x0.min(cx - r), x1.max(cx + r), y0.min(cy - r), y1.max(cy + r));
        let th: Vec<f64> = (0..48).map(|i| i as f64 * std::f64::consts::TAU / 48.0).collect();
        let fill = rgb_u8(&colors[gi.get(k).copied().flatten().unwrap_or(0) % colors.len()]).unwrap();
        parts.push((a, polygon(th.iter().map(|t| cx + r * t.cos()).collect(), th.iter().map(|t| cy + r * t.sin()).collect(), fill, alpha, Some(edge))));
        if let Some(l) = labels.get(k) {
            parts.push((a, label(cx, cy, l, font, None)));
        }
    }
    let ax = &mut it.figures[f].axes[a];
    bare_axes(ax, (x0 - 0.1, x1 + 0.1), (y0 - 0.1, y1 + 0.1));
    ax.title = title;
    Ok(parts)
}

// ---- parallelplot ----------------------------------------------------------------------------

/// A char matrix with one row per item, padded with blanks.
fn char_rows(items: &[String]) -> Value {
    let w = items.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let rows: Vec<Vec<char>> = items.iter().map(|s| s.chars().chain(std::iter::repeat(' ')).take(w).collect()).collect();
    let data: Vec<f64> = (0..w).flat_map(|j| rows.iter().map(move |r| r[j] as u32 as f64)).collect();
    let mut m = Matrix::new(items.len(), w, data);
    m.kind = Kind::Char;
    Value::Num(m)
}

fn parallelplot(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = select_axes(it, a)?;
    let id;
    let mut props = name_values(arg(a, 0, "parallelplot").map(|_| &a[1..])?)?;
    if let Value::Table(t) = &a[0] {
        let vars = match take(&mut props, "CoordinateVariables") {
            Some(v) => texts(it, &v)?,
            None => t.names.clone(),
        };
        let gvar = take(&mut props, "GroupVariable").map(|v| text(&v)).unwrap_or_default();
        id = new_chart(it, PARALLEL, false);
        put(it, id, vec![
            ("SourceTable", a[0].clone()),
            ("CoordinateVariables", cell_row(&vars)),
            ("CoordinateTickLabels", cell_col(&vars)),
            ("GroupVariable", Value::chars(&gvar)),
        ]);
    } else {
        let m = a[0].to_matrix()?;
        let n = m.cols();
        let group = take(&mut props, "GroupData");
        id = new_chart(it, PARALLEL, false);
        let ticks: Vec<String> = (1..=n).map(|k| k.to_string()).collect();
        put(it, id, vec![
            ("Data", Value::Num(m)),
            ("CoordinateData", row((1..=n).map(|k| k as f64).collect())),
            ("CoordinateTickLabels", char_rows(&ticks)),
            ("GroupData", group.unwrap_or_else(Value::empty)),
        ]);
    }
    finish(it, &[id], &props, nargout)
}

/// The data columns of a parallel coordinates plot and the group of each row.
fn parallel_data(it: &mut Interpreter, id: u64) -> R<(Vec<Vec<f64>>, Vec<Option<usize>>)> {
    if let Some(Value::Table(t)) = gfx::stored(it, id, "SourceTable") {
        let vars = prop_texts(it, id, "CoordinateVariables")?;
        let mut cols = Vec::new();
        for v in &vars {
            let c = table_var(&t, v)?;
            cols.push(if is_numeric(&c) { nums(&c) } else { groups(it, &c)?.0.iter().map(|g| g.map_or(f64::NAN, |k| k as f64 + 1.0)).collect() });
        }
        let gvar = text(&get(it, id, "GroupVariable"));
        let g = if gvar.is_empty() { vec![None; t.nrows] } else { groups(it, &table_var(&t, &gvar)?)?.0 };
        return Ok((cols, g));
    }
    let m = get(it, id, "Data").to_matrix()?;
    let cols = (0..m.cols()).map(|j| (0..m.rows()).map(|i| m.get(i, j)).collect()).collect();
    let gv = get(it, id, "GroupData");
    let g = if gv.numel() > 0 { groups(it, &gv)?.0 } else { vec![None; m.rows()] };
    Ok((cols, g))
}

fn parallel_parts(it: &mut Interpreter, id: u64, f: usize, a: usize) -> R<Vec<(usize, Series)>> {
    let (cols, g) = parallel_data(it, id)?;
    let ticks = prop_texts(it, id, "CoordinateTickLabels")?;
    let norm = text(&get(it, id, "DataNormalization")).to_lowercase();
    let color = prop_color(it, id, "Color", COLOR_ORDER[0]);
    let lw = get(it, id, "LineWidth").scalar().unwrap_or(1.0) as f32;
    let (title, xl, yl) = (prop_texts(it, id, "Title")?.join("\n"), text(&get(it, id, "CoordinateLabel")), text(&get(it, id, "DataLabel")));
    let scaled: Vec<Vec<f64>> = cols.iter().map(|c| {
        let finite: Vec<f64> = c.iter().copied().filter(|v| v.is_finite()).collect();
        let (lo, hi) = (finite.iter().copied().fold(f64::INFINITY, f64::min), finite.iter().copied().fold(f64::NEG_INFINITY, f64::max));
        let mean = finite.iter().sum::<f64>() / finite.len().max(1) as f64;
        let sd = (finite.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (finite.len() as f64 - 1.0).max(1.0)).sqrt();
        c.iter().map(|&v| match norm.as_str() {
            "range" if hi > lo => (v - lo) / (hi - lo),
            "range" => 0.5,
            "zscore" if sd > 0.0 => (v - mean) / sd,
            "center" => v - mean,
            "scale" if sd > 0.0 => v / sd,
            _ => v,
        }).collect()
    }).collect();
    let n = scaled.len();
    let rows = scaled.first().map_or(0, |c| c.len());
    let mut parts = Vec::new();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..rows {
        let y: Vec<f64> = scaled.iter().map(|c| c[i]).collect();
        y.iter().filter(|v| v.is_finite()).for_each(|&v| (lo, hi) = (lo.min(v), hi.max(v)));
        let c = match g.get(i).copied().flatten() {
            Some(k) => rgb_u8(&COLOR_ORDER[k % COLOR_ORDER.len()]).unwrap(),
            None => color,
        };
        parts.push((a, line((1..=n).map(|k| k as f64).collect(), y, c, lw)));
    }
    if !lo.is_finite() {
        (lo, hi) = (0.0, 1.0);
    }
    for k in 1..=n {
        parts.push((a, line(vec![k as f64, k as f64], vec![lo, hi], [128, 128, 128], 1.0)));
    }
    let ax = &mut it.figures[f].axes[a];
    ax.xticks = Ticks { values: Some((1..=n).map(|k| k as f64).collect()), labels: Some(ticks), ..Ticks::default() };
    ax.xlim = Some((0.8, n as f64 + 0.2));
    ax.title = title;
    ax.xlabel = xl;
    ax.ylabel = yl;
    Ok(parts)
}

// ---- stackedplot -----------------------------------------------------------------------------

fn stackedplot(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = select_axes(it, a)?;
    let props;
    let id;
    if let Some(Value::Table(t)) = a.first() {
        let positional = a.len() >= 2 && (a.len() - 2) % 2 == 0 && !is_name(&a[1]) || (a.len() == 2);
        let vars = if positional { texts(it, &a[1])? } else { t.names.iter().filter(|n| t.var(n).is_some_and(is_numeric)).cloned().collect() };
        props = name_values(&a[if positional { 2 } else { 1 }..])?;
        let xlabel = if t.row_times.is_some() { t.dim_names[0].clone() } else { "Row".to_string() };
        id = new_chart(it, STACKED, true);
        put(it, id, vec![
            ("SourceTable", a[0].clone()),
            ("DisplayVariables", cell_row(&vars)),
            ("DisplayLabels", cell_col(&vars)),
            ("XLabel", Value::chars(&xlabel)),
            ("XVariable", Value::chars("")),
        ]);
    } else {
        let npos = a.iter().position(is_name).unwrap_or(a.len()).min(2);
        props = name_values(&a[npos..])?;
        let mut y = arg(a, npos.max(1) - 1, "stackedplot")?.to_matrix()?;
        if y.rows() == 1 {
            y = Matrix::col(y.data);
        }
        let x = if npos == 2 { Matrix::row(a[0].to_matrix()?.data) } else { Matrix::row((1..=y.rows()).map(|k| k as f64).collect()) };
        let labels: Vec<String> = (1..=y.cols()).map(|k| format!("Column {k}")).collect();
        id = new_chart(it, STACKED, true);
        put(it, id, vec![("XData", Value::Num(x)), ("YData", Value::Num(y)), ("DisplayLabels", cell_col(&labels))]);
    }
    finish(it, &[id], &props, nargout)
}

fn stacked_parts(it: &mut Interpreter, id: u64, f: usize) -> R<Vec<(usize, Series)>> {
    let (x, kind);
    let mut ys: Vec<Vec<f64>> = Vec::new();
    if let Some(Value::Table(t)) = gfx::stored(it, id, "SourceTable") {
        (x, kind) = match &t.row_times {
            Some(rt) => {
                let (m, k) = crate::builtins_plot::x_numeric(rt)?;
                (m.data, k)
            }
            None => ((1..=t.nrows).map(|k| k as f64).collect(), XAxisKind::Numeric),
        };
        for v in prop_texts(it, id, "DisplayVariables")? {
            ys.push(nums(&table_var(&t, &v)?));
        }
    } else {
        (x, kind) = (nums(&get(it, id, "XData")), XAxisKind::Numeric);
        let y = get(it, id, "YData").to_matrix()?;
        ys = (0..y.cols()).map(|j| (0..y.rows()).map(|i| y.get(i, j)).collect()).collect();
    }
    let labels = prop_texts(it, id, "DisplayLabels")?;
    let (xlabel, title) = (text(&get(it, id, "XLabel")), prop_texts(it, id, "Title")?.join("\n"));
    let color = prop_color(it, id, "Color", COLOR_ORDER[0]);
    let lw = get(it, id, "LineWidth").scalar().unwrap_or(0.5) as f32;
    let style = crate::graphics::parse_line_spec(&text(&get(it, id, "LineStyle"))).and_then(|p| p.1).unwrap_or(LineStyle::Solid);
    let marker = crate::graphics::parse_line_spec(&text(&get(it, id, "Marker"))).and_then(|p| p.2);
    let msize = get(it, id, "MarkerSize").scalar().unwrap_or(6.0) as f32;
    let k = ys.len().max(1);
    let fig = &mut it.figures[f];
    fig.axes.truncate(1);
    while fig.axes.len() < k {
        fig.axes.push(Axes::default());
    }
    fig.grid = (k, 1);
    let mut parts = Vec::new();
    for (j, y) in ys.into_iter().enumerate() {
        let ax = &mut fig.axes[j];
        ax.ylabel = labels.get(j).cloned().unwrap_or_default();
        ax.xlabel = if j == k - 1 { xlabel.clone() } else { String::new() };
        ax.title = if j == 0 { title.clone() } else { String::new() };
        ax.x_kind = kind;
        let mut l = line(x.clone(), y, color, lw);
        l.line_style = style;
        l.marker = marker;
        l.marker_size = msize;
        parts.push((j, l));
    }
    Ok(parts)
}

// ---- scatterhistogram ------------------------------------------------------------------------

fn scatterhistogram(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = select_axes(it, a)?;
    let (x, y, mut props, later) = if let Some(Value::Table(t)) = a.first() {
        let (xv, yv) = (arg(a, 1, "scatterhistogram")?.text()?, arg(a, 2, "scatterhistogram")?.text()?);
        let props = name_values(&a[3..])?;
        let later = vec![("SourceTable", a[0].clone()), ("XVariable", Value::chars(&xv)), ("YVariable", Value::chars(&yv)), ("XLabel", Value::chars(&xv)), ("YLabel", Value::chars(&yv))];
        (nums(&table_var(t, &xv)?), nums(&table_var(t, &yv)?), props, later)
    } else {
        let (x, y) = (nums(arg(a, 0, "scatterhistogram")?), nums(arg(a, 1, "scatterhistogram")?));
        (x, y, name_values(&a[2..])?, Vec::new())
    };
    if x.len() != y.len() {
        return rt("X and Y must have the same number of elements.");
    }
    let group = take(&mut props, "GroupData");
    let nbins = take(&mut props, "NumBins").map(|v| nums(&v));
    let mut bins = Vec::new();
    let mut widths = Vec::new();
    for (k, v) in [&x, &y].into_iter().enumerate() {
        let mut args = vec![col(v.clone())];
        if let Some(nb) = &nbins {
            args.push(Value::num(nb[k.min(nb.len() - 1)]));
        }
        let r = it.call_function("histcounts", args, 2)?;
        let e = nums(&r[1]);
        bins.push((e.len() - 1) as f64);
        widths.push(if e.len() > 1 { e[1] - e[0] } else { 1.0 });
    }
    let id = new_chart(it, SCATTERHIST, true);
    put(it, id, later);
    put(it, id, vec![
        ("XData", col(x)),
        ("YData", col(y)),
        ("GroupData", group.unwrap_or_else(Value::empty)),
        ("NumBins", col(bins)),
        ("BinWidths", col(widths)),
    ]);
    finish(it, &[id], &props, nargout)
}

fn scatterhist_parts(it: &mut Interpreter, id: u64, f: usize) -> R<Vec<(usize, Series)>> {
    let (x, y) = (nums(&get(it, id, "XData")), nums(&get(it, id, "YData")));
    let nb = nums(&get(it, id, "NumBins"));
    let mut hist = Vec::new();
    for (k, v) in [&x, &y].into_iter().enumerate() {
        let r = it.call_function("histcounts", vec![col(v.clone()), Value::num(nb.get(k).copied().unwrap_or(10.0))], 2)?;
        hist.push((nums(&r[0]), nums(&r[1])));
    }
    let bars = text(&get(it, id, "HistogramDisplayStyle")) == "bar";
    let color = prop_color(it, id, "Color", COLOR_ORDER[0]);
    let (xl, yl, title) = (text(&get(it, id, "XLabel")), text(&get(it, id, "YLabel")), prop_texts(it, id, "Title")?.join("\n"));
    let msize = get(it, id, "MarkerSize").scalar().unwrap_or(36.0).sqrt() as f32;
    let fig = &mut it.figures[f];
    fig.axes.truncate(1);
    while fig.axes.len() < 4 {
        fig.axes.push(Axes::default());
    }
    fig.grid = (2, 2);
    fig.axes[0].title = title;
    fig.axes[1].visible_axes = Some(false);
    fig.axes[2].xlabel = xl;
    fig.axes[2].ylabel = yl;
    let mut parts = Vec::new();
    // Histograms: stairs (or bars) of x above the scatter plot and of y to its right.
    let steps = |counts: &[f64], edges: &[f64], horizontal: bool| -> Series {
        let mut a = Vec::new();
        let mut b = Vec::new();
        for (i, &c) in counts.iter().enumerate() {
            a.extend([edges[i], edges[i + 1]]);
            b.extend([c, c]);
        }
        let mut l = if horizontal { line(b, a, color, 1.0) } else { line(a, b, color, 1.0) };
        if bars {
            l.kind = SeriesKind::Area;
        }
        l
    };
    parts.push((0, steps(&hist[0].0, &hist[0].1, false)));
    let mut s = Series::new(SeriesKind::Scatter, x, y);
    s.color = Some(color);
    s.marker_size = msize;
    parts.push((2, s));
    parts.push((3, steps(&hist[1].0, &hist[1].1, true)));
    Ok(parts)
}

// ---- wordcloud -------------------------------------------------------------------------------

fn wordcloud(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = select_axes(it, a)?;
    let (words, sizes, rest, later): (Vec<String>, Vec<f64>, &[Value], Vec<(&str, Value)>) = if let Some(Value::Table(t)) = a.first() {
        let (wv, sv) = (arg(a, 1, "wordcloud")?.text()?, arg(a, 2, "wordcloud")?.text()?);
        let words = texts(it, &table_var(t, &wv)?)?;
        let later = vec![("SourceTable", a[0].clone()), ("WordVariable", Value::chars(&wv)), ("SizeVariable", Value::chars(&sv)), ("Title", Value::chars(&sv))];
        (words, nums(&table_var(t, &sv)?), &a[3..], later)
    } else if a.len() >= 2 && is_numeric(&a[1]) {
        (texts(it, &a[0])?, nums(&a[1]), &a[2..], Vec::new())
    } else {
        // Word counts, largest first (ties in category order).
        let v = arg(a, 0, "wordcloud")?;
        let (names, counts) = if crate::containers::is_categorical(v) {
            let cats = it.call_function("categories", vec![v.clone()], 1)?;
            (texts(it, &cats[0])?, nums(&it.call_function("countcats", vec![v.clone()], 1)?[0]))
        } else {
            let items = texts(it, v)?;
            let mut names: Vec<String> = Vec::new();
            let mut counts: Vec<f64> = Vec::new();
            for w in items.into_iter().filter(|w| !w.is_empty()) {
                match names.iter().position(|n| *n == w) {
                    Some(k) => counts[k] += 1.0,
                    None => {
                        names.push(w);
                        counts.push(1.0);
                    }
                }
            }
            (names, counts)
        };
        let mut order: Vec<usize> = (0..names.len()).filter(|&k| counts[k] > 0.0).collect();
        order.sort_by(|&p, &q| counts[q].partial_cmp(&counts[p]).unwrap_or(std::cmp::Ordering::Equal));
        (order.iter().map(|&k| names[k].clone()).collect(), order.iter().map(|&k| counts[k]).collect(), &a[1..], Vec::new())
    };
    if words.len() != sizes.len() {
        return rt("Number of words and sizes must match.");
    }
    let props = name_values(rest)?;
    let id = new_chart(it, WORDCLOUD, false);
    put(it, id, later);
    put(it, id, vec![("WordData", str_row(&words)), ("SizeData", row(sizes))]);
    finish(it, &[id], &props, nargout)
}

fn wordcloud_parts(it: &mut Interpreter, id: u64, f: usize, a: usize) -> R<Vec<(usize, Series)>> {
    let words = prop_texts(it, id, "WordData")?;
    let sizes = nums(&get(it, id, "SizeData"));
    let max_words = get(it, id, "MaxDisplayWords").scalar().unwrap_or(100.0).max(0.0) as usize;
    let color = prop_color(it, id, "Color", [0.251; 3]);
    let highlight = prop_color(it, id, "HighlightColor", [0.851, 0.3255, 0.098]);
    let power = get(it, id, "SizePower").scalar().unwrap_or(0.5);
    let title = prop_texts(it, id, "Title")?.join("\n");
    let mut order: Vec<usize> = (0..words.len().min(sizes.len())).filter(|&k| sizes[k] > 0.0).collect();
    order.sort_by(|&p, &q| sizes[q].partial_cmp(&sizes[p]).unwrap_or(std::cmp::Ordering::Equal));
    order.truncate(max_words);
    let top = order.first().map_or(1.0, |&k| sizes[k]);
    let n_high = (order.len() / 10).max(1);
    // Words placed along a spiral without overlapping, in pixels of a figure-sized canvas
    // (data units are pixels; text is drawn at 1.2 px per point).
    let (w, h) = (380.0, 250.0);
    let mut boxes: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut parts = Vec::new();
    for (rank, &k) in order.iter().enumerate() {
        let fs = 8.0 + 22.0 * (sizes[k] / top).powf(power);
        let px = fs * 1.2;
        let (bw, bh) = (0.62 * px * words[k].chars().count() as f64 + 6.0, px * 1.2);
        let mut t = 0.0f64;
        let spot = loop {
            let (cx, cy) = (1.4 * 2.0 * t * t.cos(), 2.0 * t * t.sin());
            let b = (cx - bw / 2.0, cy - bh / 2.0, cx + bw / 2.0, cy + bh / 2.0);
            let free = boxes.iter().all(|o| b.2 <= o.0 || b.0 >= o.2 || b.3 <= o.1 || b.1 >= o.3);
            if free && b.0 >= -w / 2.0 && b.2 <= w / 2.0 && b.1 >= -h / 2.0 && b.3 <= h / 2.0 {
                break Some(b);
            }
            t += 0.15;
            if t > 200.0 {
                break None;
            }
        };
        let Some(b) = spot else { continue };
        boxes.push(b);
        let c = if rank < n_high { highlight } else { color };
        parts.push((a, label((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0, &words[k], c, Some(fs as f32))));
    }
    let ax = &mut it.figures[f].axes[a];
    bare_axes(ax, (-w / 2.0, w / 2.0), (-h / 2.0, h / 2.0));
    ax.title = title;
    Ok(parts)
}

// ---- geobubble -------------------------------------------------------------------------------

fn geobubble(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = select_axes(it, a)?;
    let (lat, lon, sz, cd, mut props, later) = if let Some(Value::Table(t)) = a.first() {
        let (lv, nv) = (arg(a, 1, "geobubble")?.text()?, arg(a, 2, "geobubble")?.text()?);
        let mut props = name_values(&a[3..])?;
        let sv = take(&mut props, "SizeVariable").map(|v| text(&v)).unwrap_or_default();
        let cv = take(&mut props, "ColorVariable").map(|v| text(&v)).unwrap_or_default();
        let sz = if sv.is_empty() { None } else { Some(nums(&table_var(t, &sv)?)) };
        let cd = if cv.is_empty() { None } else { Some(table_var(t, &cv)?) };
        let later = vec![("SourceTable", a[0].clone()), ("LatitudeVariable", Value::chars(&lv)), ("LongitudeVariable", Value::chars(&nv)), ("SizeVariable", Value::chars(&sv)), ("ColorVariable", Value::chars(&cv))];
        (nums(&table_var(t, &lv)?), nums(&table_var(t, &nv)?), sz, cd, props, later)
    } else {
        let npos = a.iter().position(is_name).unwrap_or(a.len()).min(4);
        let sz = a.get(2).filter(|_| npos > 2).map(nums);
        let cd = a.get(3).filter(|_| npos > 3).cloned();
        (nums(arg(a, 0, "geobubble")?), nums(arg(a, 1, "geobubble")?), sz, cd, name_values(&a[npos..])?, Vec::new())
    };
    if lat.len() != lon.len() {
        return rt("Latitude and longitude data must have the same number of elements.");
    }
    let sizes = sz.unwrap_or_else(|| vec![1.0; lat.len()]);
    let pad = |v: &[f64]| {
        let (lo, hi) = (v.iter().copied().fold(f64::INFINITY, f64::min), v.iter().copied().fold(f64::NEG_INFINITY, f64::max));
        let d = if hi > lo { (hi - lo) * 0.025 } else { 1.0 };
        row(vec![lo - d, hi + d])
    };
    let lim = |v: &[f64]| row(vec![v.iter().copied().fold(f64::INFINITY, f64::min), v.iter().copied().fold(f64::NEG_INFINITY, f64::max)]);
    let title = take(&mut props, "Title");
    let id = new_chart(it, GEOBUBBLE, false);
    put(it, id, later);
    put(it, id, vec![
        ("LatitudeData", col(lat.clone())),
        ("LongitudeData", col(lon.clone())),
        ("SizeData", col(sizes.clone())),
        ("ColorData", cd.unwrap_or_else(Value::empty)),
        ("SizeLimits", lim(&sizes)),
        ("LatitudeLimits", pad(&lat)),
        ("LongitudeLimits", pad(&lon)),
        ("Title", title.unwrap_or_else(|| Value::chars(""))),
    ]);
    finish(it, &[id], &props, nargout)
}

fn geobubble_parts(it: &mut Interpreter, id: u64, f: usize, a: usize) -> R<Vec<(usize, Series)>> {
    let (lat, lon, sz) = (nums(&get(it, id, "LatitudeData")), nums(&get(it, id, "LongitudeData")), nums(&get(it, id, "SizeData")));
    let range = nums(&get(it, id, "BubbleWidthRange"));
    let lim = nums(&get(it, id, "SizeLimits"));
    let cd = get(it, id, "ColorData");
    let colors = color_rows(&get(it, id, "BubbleColorList"));
    let title = prop_texts(it, id, "Title")?.join("\n");
    let (la, lo) = (nums(&get(it, id, "LatitudeLimits")), nums(&get(it, id, "LongitudeLimits")));
    let gi = if cd.numel() > 0 { groups(it, &cd)?.0 } else { vec![Some(0); lat.len()] };
    let (r0, r1) = (range.first().copied().unwrap_or(5.0), range.get(1).copied().unwrap_or(20.0));
    let (l0, l1) = (lim.first().copied().unwrap_or(0.0), lim.get(1).copied().unwrap_or(1.0));
    let palette = if colors.len() > 1 { colors } else if cd.numel() > 0 { COLOR_ORDER.to_vec() } else { colors };
    let mut parts = Vec::new();
    for g in 0..palette.len() {
        let pick: Vec<usize> = (0..lat.len()).filter(|&i| gi.get(i).copied().flatten().unwrap_or(0) % palette.len() == g).collect();
        if pick.is_empty() {
            continue;
        }
        let mut s = Series::new(SeriesKind::Scatter, pick.iter().map(|&i| lon[i]).collect(), pick.iter().map(|&i| lat[i]).collect());
        s.color = rgb_u8(&palette[g]);
        s.face_alpha = 0.6;
        s.sizes = pick.iter().map(|&i| {
            let t = if l1 > l0 { ((sz[i] - l0) / (l1 - l0)).clamp(0.0, 1.0) } else { 1.0 };
            (r0 * r0 + t * (r1 * r1 - r0 * r0)).sqrt() as f32
        }).collect();
        parts.push((a, s));
    }
    let ax = &mut it.figures[f].axes[a];
    ax.title = title;
    ax.xlabel = "Longitude".into();
    ax.ylabel = "Latitude".into();
    if la.len() == 2 && lo.len() == 2 {
        ax.xlim = Some((lo[0], lo[1]));
        ax.ylim = Some((la[0], la[1]));
    }
    ax.grid = true;
    Ok(parts)
}

// ---- yyaxis ----------------------------------------------------------------------------------

fn yyaxis(it: &mut Interpreter, a: &[Value]) -> Ret {
    let a = select_axes(it, a)?;
    let right = match arg(a, 0, "yyaxis")?.text()?.to_lowercase().as_str() {
        "left" => false,
        "right" => true,
        _ => return rt("Specify 'left' or 'right'."),
    };
    let f = it.current_figure_index();
    let ax = it.figures[f].axes_mut();
    if ax.yy.is_none() {
        // Existing plots stay on the left, whose axis is then black.
        let left = if ax.series.is_empty() { COLOR_ORDER[0] } else { [0.0, 0.0, 0.0] };
        ax.yy = Some(Box::new(Yy { right_active: false, other: YySide::default(), colors: [left, COLOR_ORDER[1]] }));
    }
    ax.switch_side(right);
    let fid = it.figures[f].id;
    it.mark_figure_dirty(fid);
    none()
}

// ---- plotyy ----------------------------------------------------------------------------------

/// A 1-2-5 step m * 10^e, rounded as the decimal it is.
fn yy_value((m, e): (f64, i32)) -> f64 {
    if e < 0 { m / 10f64.powi(-e) } else { m * 10f64.powi(e) }
}

/// MATLAB's tick spacing for default-size axes: the finest 1-2-5 step giving at most 11
/// intervals.
fn yy_step(range: f64) -> (f64, i32) {
    let mut e = (range / 11.0).log10().floor() as i32;
    loop {
        for m in [1.0, 2.0, 5.0] {
            if range / yy_value((m, e)) <= 11.0 + 1e-9 {
                return (m, e);
            }
        }
        e += 1;
    }
}

/// The next coarser 1-2-5 step.
fn yy_coarser((m, e): (f64, i32)) -> (f64, i32) {
    match m as i32 {
        1 => (2.0, e),
        2 => (5.0, e),
        _ => (1.0, e + 1),
    }
}

/// Limits rounded out to multiples of `s`, and their number of intervals.
fn yy_fit((lo, hi): (f64, f64), s: f64) -> (f64, f64, usize) {
    let (a, b) = ((lo / s).floor(), (hi / s).ceil());
    (a * s, b * s, (b - a).round().max(1.0) as usize)
}

/// The data range of plotyy axes for its y ticks (a constant spans one either side).
fn yy_range(ax: &Axes) -> (f64, f64) {
    let ys = ax.series.iter().flat_map(|s| s.y.iter().copied()).filter(|v| v.is_finite());
    let (lo, hi) = ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| (l.min(v), h.max(v)));
    if lo == hi {
        return (lo - 1.0, hi + 1.0);
    }
    ax.bounds().1
}

/// `[ax, h1, h2] = plotyy(x1, y1, x2, y2[, fun1[, fun2]])`: y1 against x1 on the left y axis and
/// y2 against x2 on a right y axis over the same box. The two axes get as many ticks (at most
/// eight), both starting from MATLAB's default ticks and coarsening the axis with more
/// intervals until they agree.
fn plotyy(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 4 {
        return rt("Not enough input arguments.");
    }
    let fun1 = a.get(4).cloned().unwrap_or_else(|| Value::chars("plot"));
    let fun2 = a.get(5).cloned().unwrap_or_else(|| fun1.clone());
    let call = |it: &mut Interpreter, f: &Value, x: &Value, y: &Value| -> R<Value> {
        let out = match f {
            Value::Func(fv) => it.call_value(fv, vec![x.clone(), y.clone()], 1)?,
            v => it.call_function(&v.text()?, vec![x.clone(), y.clone()], 1)?,
        };
        Ok(out.into_iter().next().unwrap_or_else(Value::empty))
    };
    let f = it.current_figure_index();
    let base = it.figures[f].current_axes;
    let base_id = gfx::axes_id(it, f, base);
    // A new plotyy replaces the right axes of an earlier one.
    if let Some(k) = it.figures[f].axes.iter().position(|x| x.overlay_of == base_id) {
        let old = it.figures[f].axes.remove(k);
        it.gfx_store.remove(&old.id);
        for s in &old.series {
            it.gfx_store.remove(&s.id);
        }
    }
    let h1 = call(it, &fun1, &a[0], &a[1])?;
    let n1 = it.figures[f].axes[base].series.len();
    it.figures[f].axes.push(Axes { overlay_of: base_id, ..Axes::default() });
    let over = it.figures[f].axes.len() - 1;
    let over_id = gfx::axes_id(it, f, over);
    it.figures[f].current_axes = over;
    let h2 = call(it, &fun2, &a[2], &a[3]);
    it.figures[f].current_axes = base;
    let h2 = h2?;
    // The right axes continue the color order.
    let ids: Vec<u64> = it.figures[f].axes[over].series.iter().map(|s| s.id).collect();
    for (j, id) in ids.iter().enumerate() {
        let c = COLOR_ORDER[(n1 + j) % COLOR_ORDER.len()];
        if let Some(Target::Series(f, x, s)) = gfx::resolve(it, *id) {
            it.figures[f].axes[x].series[s].color = Some(c.map(|v| (v * 255.0).round() as u8));
        }
        put(it, *id, vec![("Color", Value::Num(Matrix::row(c.to_vec())))]);
    }
    // Shared x limits; y ticks aligned for linear axes.
    let (l, r) = (&it.figures[f].axes[base], &it.figures[f].axes[over]);
    let (lx, rx) = (l.limits().0, r.limits().0);
    let xlim = (lx.0.min(rx.0), lx.1.max(rx.1));
    let aligned = (!l.ylog && !r.ylog).then(|| {
        let (d1, d2) = (yy_range(l), yy_range(r));
        let (mut s1, mut s2) = (yy_step(d1.1 - d1.0), yy_step(d2.1 - d2.0));
        loop {
            let (n1, n2) = (yy_fit(d1, yy_value(s1)).2, yy_fit(d2, yy_value(s2)).2);
            if n1 == n2 && n1 <= 7 {
                break;
            }
            if n1 >= n2 {
                s1 = yy_coarser(s1);
            }
            if n2 >= n1 {
                s2 = yy_coarser(s2);
            }
        }
        (yy_fit(d1, yy_value(s1)), yy_value(s1), yy_fit(d2, yy_value(s2)), yy_value(s2))
    });
    for (k, ax) in [base, over].into_iter().enumerate() {
        let axes = &mut it.figures[f].axes[ax];
        axes.xlim = Some(xlim);
        if let Some((f1, s1, f2, s2)) = aligned {
            let ((lo, hi, n), s) = if k == 0 { (f1, s1) } else { (f2, s2) };
            axes.ylim = Some((lo, hi));
            axes.yticks.values = Some((0..=n).map(|i| {
                let v = lo + i as f64 * s;
                if v.abs() < s * 1e-9 { 0.0 } else { v }
            }).collect());
        }
    }
    // A y axis takes the color of its object when it has one only.
    let color = |it: &Interpreter, ax: usize, k: usize| match it.figures[f].axes[ax].series.as_slice() {
        [s] => gfx::stored(it, s.id, "Color").and_then(|v| v.to_matrix().ok()).filter(|m| m.numel() == 3).map(|m| [m.data[0], m.data[1], m.data[2]])
            .or(s.color.map(|c| c.map(|v| v as f64 / 255.0)))
            .unwrap_or(COLOR_ORDER[k % COLOR_ORDER.len()]),
        _ => [0.15, 0.15, 0.15],
    };
    let (c1, c2) = (color(it, base, 0), color(it, over, n1));
    put(it, base_id, vec![("YColor", Value::Num(Matrix::row(c1.to_vec()))), ("Box", Value::chars("on"))]);
    put(it, over_id, vec![
        ("YColor", Value::Num(Matrix::row(c2.to_vec()))),
        ("Color", Value::chars("none")),
        ("YAxisLocation", Value::chars("right")),
        ("Box", Value::chars("off")),
        ("HitTest", Value::chars("off")),
    ]);
    let fid = it.figures[f].id;
    it.mark_figure_dirty(fid);
    if nargout == 0 {
        return none();
    }
    let axes = gfx::handles(it, &[base_id, over_id]);
    if let Value::Object(o) = &axes {
        if let Value::Num(m) = &mut o.props.borrow_mut()[0] {
            *m = Matrix::row(m.data.clone());
        }
    }
    Ok(Some(vec![axes, h1, h2]))
}

// ---- datetick --------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, PartialOrd)]
enum Unit {
    Second,
    Minute,
    Hour,
    Day,
    Month,
    Year,
}

/// `datestr` formats by number.
const DATE_FORMATS: [&str; 32] = [
    "dd-mmm-yyyy HH:MM:SS", "dd-mmm-yyyy", "mm/dd/yy", "mmm", "m", "mm", "mm/dd", "dd", "ddd", "d", "yyyy", "yy", "mmmyy", "HH:MM:SS", "HH:MM:SS PM", "HH:MM",
    "HH:MM PM", "QQ-YY", "QQ", "dd/mm", "dd/mm/yy", "mmm.dd,yyyy HH:MM:SS", "mmm.dd,yyyy", "mm/dd/yyyy", "dd/mm/yyyy", "yy/mm/dd", "yyyy/mm/dd", "QQ-YYYY", "mmmyyyy",
    "yyyy-mm-dd", "yyyymmddTHHMMSS", "yyyy-mm-dd HH:MM:SS",
];

/// The smallest unit a date format shows.
fn format_unit(f: &str) -> Unit {
    if f.contains("SS") {
        Unit::Second
    } else if f.contains("MM") {
        Unit::Minute
    } else if f.contains("HH") {
        Unit::Hour
    } else if f.contains('d') {
        Unit::Day
    } else if f.contains('m') || f.contains("QQ") {
        Unit::Month
    } else {
        Unit::Year
    }
}

/// Days since 0000-01-00 (`datenum`) of a civil date.
fn datenum(y: i64, m: i64, d: i64) -> f64 {
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe - 719468 + 719529) as f64
}

/// The civil year and month of a `datenum`.
fn year_month(t: f64) -> (i64, i64) {
    let z = t.floor() as i64 - 719529 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + (m <= 2) as i64, m)
}

/// Date ticks spanning [lo, hi] in steps of `unit`, at most about a dozen of them.
fn date_ticks(lo: f64, hi: f64, unit: Unit) -> Vec<f64> {
    let pick = |n: f64, steps: &[i64], max: f64| steps.iter().copied().find(|&s| n / s as f64 <= max).unwrap_or(*steps.last().unwrap());
    match unit {
        Unit::Year | Unit::Month => {
            let ((y0, m0), (y1, m1)) = (year_month(lo), year_month(hi));
            let mut start = if unit == Unit::Year { y0 * 12 } else { y0 * 12 + m0 - 1 };
            let end_month = y1 * 12 + m1 - 1;
            let end = if datenum(y1, m1, 1) < hi { end_month + 1 } else { end_month };
            let end = if unit == Unit::Year { (end + 11) / 12 * 12 } else { end };
            let step = if unit == Unit::Year {
                let s = pick(((end - start) / 12) as f64, &[1, 2, 5, 10, 20, 50, 100, 200, 500, 1000], 10.0);
                start = (start / 12).div_euclid(s) * s * 12;
                s * 12
            } else {
                let s = pick((end - start) as f64, &[1, 2, 3, 4, 6, 12], 12.0);
                start -= start.rem_euclid(s);
                s
            };
            let mut out = Vec::new();
            let mut k = start;
            while k <= end {
                out.push(datenum(k.div_euclid(12), k.rem_euclid(12) + 1, 1));
                k += step;
            }
            out
        }
        _ => {
            let (per_day, steps, max): (f64, &[i64], f64) = match unit {
                Unit::Day => (1.0, &[1, 2, 5, 7, 14, 28, 56], 10.0),
                Unit::Hour => (24.0, &[1, 2, 3, 4, 6, 12, 24, 48], 5.0),
                Unit::Minute => (1440.0, &[1, 2, 5, 10, 15, 20, 30, 60, 120], 6.0),
                _ => (86400.0, &[1, 2, 5, 10, 15, 20, 30, 60, 120], 6.0),
            };
            let (a, b) = ((lo * per_day + 1e-6).floor() as i64, (hi * per_day - 1e-6).ceil() as i64);
            let s = pick((b - a) as f64, steps, max);
            let a = a - a.rem_euclid(s);
            let b = a + (b - a + s - 1).div_euclid(s) * s;
            (a..=b).step_by(s as usize).map(|v| v as f64 / per_day).collect()
        }
    }
}

fn datetick(it: &mut Interpreter, a: &[Value]) -> Ret {
    let a = select_axes(it, a)?;
    let (mut y_axis, mut format, mut keep_limits, mut keep_ticks) = (false, None::<Value>, false, false);
    for (i, v) in a.iter().enumerate() {
        match v.text().map(|t| t.to_lowercase()).ok().as_deref() {
            Some("x") if i == 0 => {}
            Some("y") if i == 0 => y_axis = true,
            Some("z") if i == 0 => return rt("datetick supports the x and y axes."),
            Some("keeplimits") => keep_limits = true,
            Some("keepticks") => keep_ticks = true,
            _ => format = Some(v.clone()),
        }
    }
    let f = it.current_figure_index();
    let ax = it.figures[f].axes_mut();
    let ((bx0, bx1), (by0, by1)) = ax.bounds();
    let ((lx0, lx1), (ly0, ly1)) = ax.limits();
    let ((dlo, dhi), (llo, lhi)) = if y_axis { ((by0, by1), (ly0, ly1)) } else { ((bx0, bx1), (lx0, lx1)) };
    let fmt = match &format {
        Some(v @ Value::Num(m)) if m.kind != Kind::Char => Some(DATE_FORMATS.get(v.scalar()? as usize).ok_or_else(|| LangError::new("Unknown date format number.", 0))?.to_string()),
        Some(v) => Some(v.text()?),
        None => None,
    };
    let r = dhi - dlo;
    let auto = if r >= 730.0 { Unit::Year } else if r > 62.0 { Unit::Month } else if r >= 1.0 { Unit::Day } else if r >= 2.0 / 24.0 { Unit::Hour } else if r >= 2.0 / 1440.0 { Unit::Minute } else { Unit::Second };
    let unit = fmt.as_deref().map(format_unit).unwrap_or(auto);
    let fmt = fmt.unwrap_or_else(|| match unit {
        Unit::Year => "yyyy",
        Unit::Month => "mmm",
        Unit::Day => "mm/dd",
        Unit::Hour | Unit::Minute => "HH:MM",
        Unit::Second => "HH:MM:SS",
    }.to_string());
    let ticks_now = if y_axis { &ax.yticks } else { &ax.xticks };
    let mut ticks = if keep_ticks { gfx::tick_values(ticks_now, llo, lhi) } else { date_ticks(dlo, dhi, unit) };
    if keep_limits {
        ticks.retain(|&t| t >= llo - 1e-9 && t <= lhi + 1e-9);
    }
    let labels = if ticks.is_empty() {
        Vec::new()
    } else {
        let s = it.call_function("datestr", vec![col(ticks.clone()), Value::chars(&fmt)], 1)?;
        texts(it, &s[0])?
    };
    let lim = if keep_limits { Some((llo, lhi)) } else { ticks.first().zip(ticks.last()).map(|(&a, &b)| (a, b)).filter(|(a, b)| b > a) };
    let ax = it.figures[f].axes_mut();
    let (t, l) = if y_axis { (&mut ax.yticks, &mut ax.ylim) } else { (&mut ax.xticks, &mut ax.xlim) };
    t.values = Some(ticks);
    t.labels = Some(labels);
    t.as_char = true;
    if lim.is_some() {
        *l = lim;
    }
    let fid = it.figures[f].id;
    it.mark_figure_dirty(fid);
    none()
}


/// `sortx(h, row, direction)` / `sorty(h, column, direction)`: reorders the displayed columns
/// (rows) of a heatmap by the values of one displayed row (column), stably, missing values
/// last; without a row (column), by the first displayed one.
fn sort_heatmap(it: &mut Interpreter, a: &[Value], x: bool) -> Ret {
    let h = a.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?;
    let ids = gfx::ids_of(h).ok_or_else(|| LangError::new("The first argument must be a heatmap.", 0))?;
    let id = *ids.first().ok_or_else(|| LangError::new("The first argument must be a heatmap.", 0))?;
    if class_at(it, id) != Some(HEATMAP) {
        return rt("The first argument must be a heatmap.");
    }
    let xd_v = get(it, id, "XDisplayData");
    let yd_v = get(it, id, "YDisplayData");
    let xd = texts(it, &xd_v)?;
    let yd = texts(it, &yd_v)?;
    let cd = get(it, id, "ColorDisplayData").to_matrix()?;
    let (nr, nc) = (yd.len(), xd.len());
    if cd.numel() != nr * nc {
        return rt("The heatmap has no color data to sort.");
    }
    let at = |r: usize, c: usize| cd.data[c * nr + r];
    let mut desc = false;
    let mut key: Option<String> = None;
    for v in &a[1..] {
        let t = match v {
            Value::Num(m) if m.kind != Kind::Char => crate::display::fmt_g(m.data.first().copied().unwrap_or(0.0), 5),
            other => texts(it, other)?.into_iter().next().unwrap_or_default(),
        };
        match t.to_lowercase().as_str() {
            "descend" => desc = true,
            "ascend" => desc = false,
            _ if key.is_none() => key = Some(t),
            _ => {}
        }
    }
    let (labels, other) = if x { (&xd, &yd) } else { (&yd, &xd) };
    // Without a row (column), the first displayed one is the key.
    let key = key.or_else(|| other.first().cloned());
    let n = labels.len();
    let mut order: Vec<usize> = (0..n).collect();
    match &key {
        Some(k) => {
            let line = other.iter().position(|l| l == k).ok_or_else(|| LangError::new(format!("'{k}' is not a displayed {}.", if x { "row" } else { "column" }), 0))?;
            let val = |i: usize| if x { at(line, i) } else { at(i, line) };
            order.sort_by(|&p, &q| {
                let (u, v) = (val(p), val(q));
                match (u.is_nan(), v.is_nan()) {
                    (true, true) => std::cmp::Ordering::Equal,
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    _ if desc => v.partial_cmp(&u).unwrap_or(std::cmp::Ordering::Equal),
                    _ => u.partial_cmp(&v).unwrap_or(std::cmp::Ordering::Equal),
                }
            });
        }
        None => {
            order.sort_by(|&p, &q| if desc { labels[q].cmp(&labels[p]) } else { labels[p].cmp(&labels[q]) });
        }
    }
    let new_labels: Vec<String> = order.iter().map(|&i| labels[i].clone()).collect();
    let mut data = vec![0.0; nr * nc];
    for r in 0..nr {
        for c in 0..nc {
            let (sr, sc) = if x { (r, order[c]) } else { (order[r], c) };
            data[c * nr + r] = at(sr, sc);
        }
    }
    let labels_v = cell_col(&new_labels);
    if x {
        put(it, id, vec![("XDisplayData", labels_v.clone()), ("XDisplayLabels", labels_v)]);
    } else {
        put(it, id, vec![("YDisplayData", labels_v.clone()), ("YDisplayLabels", labels_v)]);
    }
    put(it, id, vec![("ColorDisplayData", Value::Num(Matrix::from_dims(vec![nr, nc], data)))]);
    redraw(it, id)?;
    Ok(Some(Vec::new()))
}


// ---- graph plots -----------------------------------------------------------------------------

/// The graph a GraphPlot draws: its edges (0-based end nodes), direction and node names.
pub struct GraphData {
    pub directed: bool,
    pub edges: Vec<(usize, usize)>,
    pub names: Option<Vec<String>>,
}

const G_EDGES: &str = "\u{1}gedges";
const G_DIRECTED: &str = "\u{1}gdirected";
const G_NAMES: &str = "\u{1}gnames";

/// Line-spec options for `plot(G, 'r--o')`.
pub fn graph_linespec(spec: &str) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    if let Some((color, style, marker)) = crate::graphics::parse_line_spec(spec) {
        if let Some(c) = color {
            let v = row(c.iter().map(|&b| b as f64 / 255.0).collect());
            out.push(("NodeColor".to_string(), v.clone()));
            out.push(("EdgeColor".to_string(), v));
        }
        if let Some(s) = style {
            let t = match s {
                LineStyle::Dashed => "--",
                LineStyle::Dotted => ":",
                LineStyle::DashDot => "-.",
                LineStyle::None => "none",
                LineStyle::Solid => "-",
            };
            out.push(("LineStyle".to_string(), Value::chars(t)));
        }
        if let Some(m) = marker {
            out.push(("Marker".to_string(), Value::chars(gfx::marker_name(Some(m)))));
        }
    }
    out
}

/// MATLAB's long names for the one-letter markers.
fn marker_long(m: &str) -> String {
    match m {
        "s" => "square",
        "d" => "diamond",
        "p" => "pentagram",
        "h" => "hexagram",
        other => other,
    }
    .to_string()
}

/// `plot(G)`: a GraphPlot in the current axes with nodes at `pos`.
pub fn graphplot(it: &mut Interpreter, g: GraphData, pos: Vec<(f64, f64)>, labels: Vec<String>, props: &[(String, Value)], nargout: usize) -> Ret {
    let id = new_axes_chart(it, GRAPHPLOT);
    let m = g.edges.len();
    let mut ends = Vec::with_capacity(2 * m);
    for &(s, t) in &g.edges {
        ends.push(s as f64);
        ends.push(t as f64);
    }
    let color = row(COLOR_ORDER[0].to_vec());
    put(it, id, vec![
        (G_EDGES, Value::Num(Matrix::new(2, m, ends))),
        (G_DIRECTED, Value::num(if g.directed { 1.0 } else { 0.0 })),
        ("XData", row(pos.iter().map(|p| p.0).collect())),
        ("YData", row(pos.iter().map(|p| p.1).collect())),
        ("ZData", row(vec![0.0; pos.len()])),
        ("NodeLabel", cell_row(&labels)),
        ("NodeColor", color.clone()),
        ("EdgeColor", color),
        ("ShowArrows", Value::chars(if g.directed { "on" } else { "off" })),
    ]);
    if let Some(nm) = &g.names {
        put(it, id, vec![(G_NAMES, cell_row(nm))]);
    }
    finish(it, &[id], props, nargout)
}

fn graph_id(it: &Interpreter, h: Option<&Value>) -> R<u64> {
    let id = h.and_then(gfx::ids_of).and_then(|v| v.first().copied());
    match id {
        Some(id) if class_at(it, id) == Some(GRAPHPLOT) => Ok(id),
        _ => rt("First argument must be a GraphPlot object."),
    }
}

fn graph_edges(it: &Interpreter, id: u64) -> Vec<(usize, usize)> {
    let v = gfx::stored(it, id, G_EDGES).map(|v| nums(&v)).unwrap_or_default();
    v.chunks(2).map(|p| (p[0] as usize, p[1] as usize)).collect()
}

fn graph_nodes(it: &mut Interpreter, id: u64) -> usize {
    nums(&get(it, id, "XData")).len()
}

/// Node indices (0-based) from numbers, names or a logical mask.
fn node_ids(it: &mut Interpreter, id: u64, v: &Value) -> R<Vec<usize>> {
    let n = graph_nodes(it, id);
    let bad = || LangError::new("Nodes must be a numeric vector, a vector of node names, or a logical vector of length equal to the number of nodes.", 0);
    match v {
        Value::Num(m) if m.kind == Kind::Logical => {
            if m.numel() != n {
                return Err(bad());
            }
            Ok((0..n).filter(|&k| m.data[k] != 0.0).collect())
        }
        Value::Num(m) if m.kind != Kind::Char => m
            .data
            .iter()
            .map(|&x| if x >= 1.0 && x <= n as f64 && x.fract() == 0.0 { Ok(x as usize - 1) } else { Err(bad()) })
            .collect(),
        _ => {
            let names = match gfx::stored(it, id, G_NAMES) {
                Some(v) => texts(it, &v)?,
                None => return Err(bad()),
            };
            texts(it, v)?.iter().map(|t| names.iter().position(|x| x == t).ok_or_else(bad)).collect()
        }
    }
}

/// Edge indices from end nodes `s`, `t` (either direction for undirected graphs).
fn edge_ids(it: &mut Interpreter, id: u64, s: &Value, t: &Value) -> R<Vec<usize>> {
    let (s, t) = (node_ids(it, id, s)?, node_ids(it, id, t)?);
    let bad = || LangError::new("s and t must specify edges in the graph.", 0);
    if s.is_empty() || t.is_empty() || (s.len() != t.len() && s.len() != 1 && t.len() != 1) {
        return Err(bad());
    }
    let directed = flag(it, id, G_DIRECTED);
    let edges = graph_edges(it, id);
    let k = s.len().max(t.len());
    let mut out = Vec::new();
    for i in 0..k {
        let (a, b) = (s[i.min(s.len() - 1)], t[i.min(t.len() - 1)]);
        let mut found = false;
        for (e, &(p, q)) in edges.iter().enumerate() {
            if (p, q) == (a, b) || (!directed && (q, p) == (a, b)) {
                out.push(e);
                found = true;
            }
        }
        if !found {
            return Err(bad());
        }
    }
    Ok(out)
}

/// Per-element colors of `NodeColor` / `EdgeColor` (`None` for 'none').
fn per_colors(it: &mut Interpreter, id: u64, name: &str, n: usize) -> Vec<Option<[f64; 3]>> {
    let v = get(it, id, name);
    if let Value::Num(m) = &v {
        if m.kind != Kind::Char && m.cols() == 3 && m.rows() == n && n != 1 {
            return (0..n).map(|r| Some([m.get(r, 0), m.get(r, 1), m.get(r, 2)])).collect();
        }
    }
    let c = gfx::color_arg(&v).ok().flatten().map(|c| c.1);
    vec![c; n]
}

fn per_numbers(it: &mut Interpreter, id: u64, name: &str, n: usize) -> Vec<f64> {
    let v = nums(&get(it, id, name));
    if v.len() == n && n != 1 { v } else { vec![v.first().copied().unwrap_or(1.0); n] }
}

fn per_texts(it: &mut Interpreter, id: u64, name: &str, n: usize) -> R<Vec<String>> {
    let v = get(it, id, name);
    Ok(match &v {
        Value::Cell(c) if c.data.len() == n => texts(it, &v)?,
        _ => vec![text(&v); n],
    })
}

fn colors_value(c: &[Option<[f64; 3]>]) -> Value {
    let n = c.len();
    let mut data = vec![0.0; 3 * n];
    for (r, x) in c.iter().enumerate() {
        let x = x.unwrap_or([0.0; 3]);
        for k in 0..3 {
            data[k * n + r] = x[k];
        }
    }
    Value::Num(Matrix::new(n, 3, data))
}

/// Applies the highlight options in `props` to nodes `nodes` and edges `edges`.
fn graph_style(it: &mut Interpreter, id: u64, nodes: &[usize], edges: &[usize], props: &[(String, Value)]) -> R<()> {
    let n = graph_nodes(it, id);
    let m = graph_edges(it, id).len();
    for (k, v) in props {
        let key = k.to_ascii_lowercase();
        match key.as_str() {
            "nodecolor" | "edgecolor" => {
                let c = gfx::color_arg(v)?.map(|c| c.1).ok_or_else(|| LangError::new("Invalid color.", 0))?;
                let (prop, count, sel) = if key == "nodecolor" { ("NodeColor", n, nodes) } else { ("EdgeColor", m, edges) };
                let mut cur = per_colors(it, id, prop, count);
                for &i in sel {
                    cur[i] = Some(c);
                }
                gfx::store(it, id, prop, colors_value(&cur));
            }
            "markersize" | "linewidth" | "arrowsize" => {
                let (prop, count, sel) = match key.as_str() {
                    "markersize" => ("MarkerSize", n, nodes),
                    "linewidth" => ("LineWidth", m, edges),
                    _ => ("ArrowSize", m, edges),
                };
                let x = v.scalar()?;
                let mut cur = per_numbers(it, id, prop, count);
                for &i in sel {
                    cur[i] = x;
                }
                gfx::store(it, id, prop, row(cur));
            }
            "marker" | "linestyle" => {
                let (prop, count, sel) = if key == "marker" { ("Marker", n, nodes) } else { ("LineStyle", m, edges) };
                let x = if prop == "Marker" { marker_long(&text(v)) } else { text(v) };
                let mut cur = per_texts(it, id, prop, count)?;
                for &i in sel {
                    cur[i] = x.clone();
                }
                gfx::store(it, id, prop, cell_row(&cur));
            }
            _ => return rt(format!("'{k}' is not a valid highlight option.")),
        }
    }
    Ok(())
}

/// `highlight(h, nodes, ...)`, `highlight(h, s, t, ...)`.
fn highlight(it: &mut Interpreter, a: &[Value]) -> Ret {
    let id = graph_id(it, a.first())?;
    let nodes_v = arg(a, 1, "highlight")?;
    let edge_form = a.get(2).is_some_and(|v| !is_name(v) || matches!(v, Value::Cell(_)));
    let (nodes, edges, rest) = if edge_form {
        let e = edge_ids(it, id, nodes_v, &a[2])?;
        (Vec::new(), e, &a[3..])
    } else {
        let nodes = node_ids(it, id, nodes_v)?;
        let all = graph_edges(it, id);
        let e: Vec<usize> = (0..all.len()).filter(|&k| nodes.contains(&all[k].0) && nodes.contains(&all[k].1)).collect();
        (nodes, e, &a[2..])
    };
    let props = name_values(rest)?;
    if props.is_empty() {
        // Without options: larger markers for nodes, thicker lines for edges.
        if edge_form {
            let m = graph_edges(it, id).len();
            let mut w = per_numbers(it, id, "LineWidth", m);
            for &e in &edges {
                w[e] *= 2.0;
            }
            gfx::store(it, id, "LineWidth", row(w));
        } else {
            let n = graph_nodes(it, id);
            let mut s = per_numbers(it, id, "MarkerSize", n);
            for &k in &nodes {
                s[k] *= 2.0;
            }
            gfx::store(it, id, "MarkerSize", row(s));
        }
    } else {
        graph_style(it, id, &nodes, &edges, &props)?;
    }
    redraw(it, id)?;
    none()
}

/// `labelnode(h, nodes, labels)` / `labeledge(h, edges, labels)` / `labeledge(h, s, t, labels)`.
fn label_graph(it: &mut Interpreter, a: &[Value], node: bool) -> Ret {
    let name = if node { "labelnode" } else { "labeledge" };
    let id = graph_id(it, a.first())?;
    let (sel, labels_v) = if node {
        let v = arg(a, 1, name)?.clone();
        (node_ids(it, id, &v)?, arg(a, 2, name)?.clone())
    } else if a.len() >= 4 {
        (edge_ids(it, id, &a[1], &a[2])?, a[3].clone())
    } else {
        let m = graph_edges(it, id).len();
        let idx = nums(arg(a, 1, name)?);
        let mut sel = Vec::new();
        for x in idx {
            if x < 1.0 || x > m as f64 || x.fract() != 0.0 {
                return rt("Edge indices must be positive integers less than or equal to the number of edges.");
            }
            sel.push(x as usize - 1);
        }
        (sel, arg(a, 2, name)?.clone())
    };
    let labels = texts(it, &labels_v)?;
    if labels.is_empty() || (labels.len() != 1 && labels.len() != sel.len()) {
        return rt(if node { "Number of labels must equal number of nodes." } else { "Number of labels must equal number of edges." });
    }
    let count = if node { graph_nodes(it, id) } else { graph_edges(it, id).len() };
    let prop = if node { "NodeLabel" } else { "EdgeLabel" };
    let cur_v = get(it, id, prop);
    let mut cur = if cur_v.numel() == count { texts(it, &cur_v)? } else { vec![String::new(); count] };
    for (k, &i) in sel.iter().enumerate() {
        cur[i] = labels[k.min(labels.len() - 1)].clone();
    }
    gfx::store(it, id, prop, cell_row(&cur));
    redraw(it, id)?;
    none()
}

/// `layout(h, method, Name, Value)`.
fn graph_layout(it: &mut Interpreter, a: &[Value]) -> Ret {
    let id = graph_id(it, a.first())?;
    let method = a.get(1).map(text).unwrap_or_else(|| "auto".into());
    let props = name_values(a.get(2..).unwrap_or(&[]))?;
    let direction = props.iter().find(|(k, _)| k.eq_ignore_ascii_case("Direction")).map(|(_, v)| text(v)).unwrap_or_else(|| "down".into());
    let n = graph_nodes(it, id);
    let directed = flag(it, id, G_DIRECTED);
    let pos = crate::graphs::relayout(n, &graph_edges(it, id), directed, &method, &direction)?;
    put(it, id, vec![("XData", row(pos.iter().map(|p| p.0).collect())), ("YData", row(pos.iter().map(|p| p.1).collect()))]);
    redraw(it, id)?;
    none()
}

fn style_of(t: &str) -> LineStyle {
    match t {
        "--" => LineStyle::Dashed,
        ":" => LineStyle::Dotted,
        "-." => LineStyle::DashDot,
        "none" => LineStyle::None,
        _ => LineStyle::Solid,
    }
}

fn u8_of(c: [f64; 3]) -> [u8; 3] {
    [(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8]
}

type StyleGroup = ((Option<[u8; 3]>, u32, String), Vec<f64>, Vec<f64>);

fn graph_parts(it: &mut Interpreter, id: u64, f: usize, a: usize) -> R<Vec<(usize, Series)>> {
    let x = nums(&get(it, id, "XData"));
    let y = nums(&get(it, id, "YData"));
    let n = x.len().min(y.len());
    let edges = graph_edges(it, id);
    let m = edges.len();
    let arrows = text(&get(it, id, "ShowArrows")) == "on";
    let ecol = per_colors(it, id, "EdgeColor", m);
    let ewid = per_numbers(it, id, "LineWidth", m);
    let esty = per_texts(it, id, "LineStyle", m)?;
    let ncol = per_colors(it, id, "NodeColor", n);
    let nsize = per_numbers(it, id, "MarkerSize", n);
    let nmark = per_texts(it, id, "Marker", n)?;
    let (x0, x1) = x.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, &p| (a.0.min(p), a.1.max(p)));
    let (y0, y1) = y.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, &p| (a.0.min(p), a.1.max(p)));
    let span = (x1 - x0).max(y1 - y0).max(1.0);
    let mut parts = Vec::new();
    // Edges grouped by style, each group a polyline broken by NaN.
    let mut groups: Vec<StyleGroup> = Vec::new();
    for (k, &(s, t)) in edges.iter().enumerate() {
        if s >= n || t >= n {
            continue;
        }
        let key = (ecol[k].map(u8_of), (ewid[k] * 100.0) as u32, esty[k].clone());
        let gi = match groups.iter().position(|g| g.0 == key) {
            Some(i) => i,
            None => {
                groups.push((key, Vec::new(), Vec::new()));
                groups.len() - 1
            }
        };
        let (p, q) = ((x[s], y[s]), (x[t], y[t]));
        let g = &mut groups[gi];
        g.1.extend([p.0, q.0, f64::NAN]);
        g.2.extend([p.1, q.1, f64::NAN]);
        if arrows && s != t {
            // Arrowheads halfway along the edge (ArrowPosition 0.5).
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            let len = dx.hypot(dy).max(1e-9);
            let (ux, uy) = (dx / len, dy / len);
            let tip = ((p.0 + q.0) / 2.0 + ux * 0.02 * span, (p.1 + q.1) / 2.0 + uy * 0.02 * span);
            for side in [-1.0, 1.0] {
                let ang: f64 = 0.4 * side;
                let head = 0.04 * span;
                let (bx, by) = (-(ux * ang.cos() - uy * ang.sin()) * head, -(ux * ang.sin() + uy * ang.cos()) * head);
                g.1.extend([tip.0, tip.0 + bx, f64::NAN]);
                g.2.extend([tip.1, tip.1 + by, f64::NAN]);
            }
        }
    }
    for ((c, w, st), gx, gy) in groups {
        let Some(c) = c else { continue };
        let mut l = line(gx, gy, c, w as f32 / 100.0);
        l.line_style = style_of(&st);
        parts.push((a, l));
    }
    // Nodes grouped by color, size and marker.
    let mut ngroups: Vec<StyleGroup> = Vec::new();
    for i in 0..n {
        let key = (ncol[i].map(u8_of), (nsize[i] * 100.0) as u32, nmark[i].clone());
        match ngroups.iter_mut().find(|g| g.0 == key) {
            Some(g) => {
                g.1.push(x[i]);
                g.2.push(y[i]);
            }
            None => ngroups.push((key, vec![x[i]], vec![y[i]])),
        }
    }
    for ((c, size, mk), gx, gy) in ngroups {
        let (Some(c), Some(marker)) = (c, gfx::parse_marker(&mk)) else { continue };
        let mut s = Series::new(SeriesKind::Scatter, gx, gy);
        s.line_style = LineStyle::None;
        s.marker = Some(marker);
        s.marker_size = size as f32 / 100.0;
        s.color = Some(c);
        parts.push((a, s));
    }
    // Labels beside the nodes and at the middle of the edges.
    let dark = [0x26, 0x26, 0x26];
    let nl = get(it, id, "NodeLabel");
    let nlabels = if nl.numel() == n { texts(it, &nl)? } else { Vec::new() };
    for (i, t) in nlabels.iter().enumerate() {
        if !t.is_empty() {
            let mut s = label(x[i] + 0.02 * span, y[i] + 0.03 * span, t, dark, Some(8.0));
            s.centered = false;
            parts.push((a, s));
        }
    }
    let el = get(it, id, "EdgeLabel");
    let elabels = if el.numel() == m { texts(it, &el)? } else { Vec::new() };
    for (k, t) in elabels.iter().enumerate() {
        let (s, e) = edges[k];
        if !t.is_empty() && s < n && e < n {
            parts.push((a, label((x[s] + x[e]) / 2.0, (y[s] + y[e]) / 2.0, t, dark, Some(8.0))));
        }
    }
    if n > 0 {
        let pad = 0.1 * span;
        let ax = &mut it.figures[f].axes[a];
        ax.xlim = Some((x0 - pad, x1 + pad));
        ax.ylim = Some((y0 - pad, y1 + pad));
    }
    Ok(parts)
}

// ---- data tips -------------------------------------------------------------------------------

/// The plotted object a data tip belongs to.
const DT_PARENT: &str = "\u{1}dtparent";

/// Objects drawn for charts (their parts) and data tips: listed by the axes they are in only
/// through their chart or plotted object.
pub fn hidden_in_axes(it: &Interpreter, f: usize, a: usize) -> Vec<u64> {
    let mut out = Vec::new();
    for s in &it.figures[f].axes[a].series {
        if s.class_hint == Some(DATATIP) {
            out.push(s.id);
        }
        if s.class_hint.is_some_and(is_chart) {
            if let Some(v) = gfx::stored(it, s.id, PARTS) {
                out.extend(nums(&v).iter().map(|&x| x as u64));
            }
        }
    }
    out
}

/// The data tips of a plotted object, newest first.
pub fn datatips_of(it: &Interpreter, id: u64) -> Vec<u64> {
    let mut out = Vec::new();
    for fig in &it.figures {
        for ax in &fig.axes {
            for s in ax.series.iter().rev() {
                if s.class_hint == Some(DATATIP) && gfx::stored(it, s.id, DT_PARENT).and_then(|v| v.scalar().ok()) == Some(id as f64) {
                    out.push(s.id);
                }
            }
        }
    }
    out
}

/// Objects deleted with `id`: a chart's parts and an object's data tips.
pub fn dependents(it: &Interpreter, id: u64) -> Vec<u64> {
    let mut out: Vec<u64> = gfx::stored(it, id, PARTS).map(|v| nums(&v).iter().map(|&x| x as u64).collect()).unwrap_or_default();
    out.extend(datatips_of(it, id));
    out
}

/// The parent of a chart that is not its axes or figure (a data tip's plotted object).
pub fn special_parent(it: &Interpreter, id: u64, class: &str) -> Option<u64> {
    if class != DATATIP {
        return None;
    }
    gfx::stored(it, id, DT_PARENT).and_then(|v| v.scalar().ok()).map(|x| x as u64)
}

fn tip_number(x: f64) -> String {
    crate::display::fmt_g(x, 6)
}

/// "X 3, Y 3" (with Z for 3-D data).
fn tip_label(it: &mut Interpreter, id: u64) -> String {
    let x = get(it, id, "X").scalar().unwrap_or(f64::NAN);
    let y = get(it, id, "Y").scalar().unwrap_or(f64::NAN);
    let mut s = format!("X {}, Y {}", tip_number(x), tip_number(y));
    if gfx::stored(it, id, "\u{1}dt3d").is_some() {
        let z = get(it, id, "Z").scalar().unwrap_or(f64::NAN);
        s += &format!(", Z {}", tip_number(z));
    }
    s
}

/// The points of a plotted object that data tips snap to: (x, y, z) per data index, and
/// whether the data are 3-D.
fn tip_points(s: &Series) -> Option<(Vec<[f64; 3]>, bool)> {
    match s.kind {
        SeriesKind::Surface => {
            let n = s.z.len();
            let pts = (0..n).map(|k| [s.x.get(k).copied().unwrap_or(f64::NAN), s.y.get(k).copied().unwrap_or(f64::NAN), s.z[k]]).collect();
            Some((pts, true))
        }
        SeriesKind::Line | SeriesKind::Scatter | SeriesKind::Bar | SeriesKind::BarH | SeriesKind::Stem | SeriesKind::Stairs | SeriesKind::Area | SeriesKind::ErrorBar | SeriesKind::Patch => {
            let three = !s.z.is_empty();
            let pts = (0..s.x.len().min(s.y.len())).map(|k| [s.x[k], s.y[k], if three { s.z.get(k).copied().unwrap_or(0.0) } else { 0.0 }]).collect();
            Some((pts, three))
        }
        _ => None,
    }
}

/// Where the tip box goes: away from the top and right edges of the axes.
fn tip_location(it: &mut Interpreter, f: usize, a: usize, p: [f64; 3], three: bool) -> &'static str {
    if three {
        return "northeast";
    }
    let ((x0, x1), (y0, y1)) = it.figures[f].axes[a].limits();
    let nx = if x1 > x0 { (p[0] - x0) / (x1 - x0) } else { 0.5 };
    let ny = if y1 > y0 { (p[1] - y0) / (y1 - y0) } else { 0.5 };
    match (ny > 0.855, nx > 0.895) {
        (false, false) => "northeast",
        (false, true) => "northwest",
        (true, false) => "southeast",
        (true, true) => "southwest",
    }
}

/// Moves a data tip to data index `k` (1-based, clamped) of its object.
fn place_tip(it: &mut Interpreter, id: u64, k: f64) -> R<()> {
    let Some(pid) = special_parent(it, id, DATATIP) else { return Ok(()) };
    let Some(Target::Series(f, a, si)) = gfx::resolve(it, pid) else { return rt("Invalid argument. The object has been deleted or does not support data tips.") };
    let Some((pts, three)) = tip_points(&it.figures[f].axes[a].series[si]) else { return Ok(()) };
    if pts.is_empty() {
        return Ok(());
    }
    let k = (k.round().max(1.0) as usize).min(pts.len());
    let p = pts[k - 1];
    let mut props = vec![("DataIndex", Value::num(k as f64)), ("X", Value::num(p[0])), ("Y", Value::num(p[1])), ("Z", Value::num(if three { p[2] } else { 0.0 }))];
    if gfx::stored(it, id, "LocationMode").map(|v| text(&v) != "manual").unwrap_or(true) {
        props.push(("Location", Value::chars(tip_location(it, f, a, p, three))));
    }
    put(it, id, props);
    if three {
        put(it, id, vec![("\u{1}dt3d", Value::num(1.0))]);
    }
    Ok(())
}

/// `datatip(target, x, y[, z])` or `datatip(target, 'DataIndex', k)`: a data tip on the data
/// point of a plotted object nearest to (x, y), shown as "X x, Y y".
fn datatip(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let target = a.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?;
    let pid = gfx::ids_of(target).and_then(|v| v.first().copied());
    let unsupported = "Invalid argument. The object has been deleted or does not support data tips.";
    let Some(pid) = pid else { return rt("First argument must be a chart object that supports data tips.") };
    let (f, ax, si) = match gfx::resolve(it, pid) {
        Some(Target::Series(f, ax, si)) => (f, ax, si),
        Some(_) => return rt("First argument must be a chart object that supports data tips."),
        None => return rt(unsupported),
    };
    let s = &it.figures[f].axes[ax].series[si];
    if s.class_hint.is_some_and(|c| is_chart(c) || c == DATATIP) {
        return rt(unsupported);
    }
    let Some((pts, three)) = tip_points(s) else { return rt(unsupported) };
    // Coordinates, then name-value options.
    let mut i = 1;
    let mut coords = Vec::new();
    while i < a.len() && is_numeric(&a[i]) {
        coords.push(a[i].scalar()?);
        i += 1;
    }
    let mut props = name_values(&a[i..])?;
    let index = match take(&mut props, "DataIndex") {
        Some(v) => v.scalar()?,
        None if coords.len() >= 2 => {
            let ((x0, x1), (y0, y1)) = it.figures[f].axes[ax].limits();
            // Distances in screen proportions of the default axes (434 by 342 pixels).
            let (sx, sy) = (434.0 / (x1 - x0).abs().max(1e-300), 342.0 / (y1 - y0).abs().max(1e-300));
            let (zmin, zmax) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p[2]), hi.max(p[2])));
            let sz = if coords.len() >= 3 && zmax > zmin { 342.0 / (zmax - zmin) } else { 0.0 };
            let dist = |p: &[f64; 3]| {
                let dz = if coords.len() >= 3 { (p[2] - coords[2]) * sz } else { 0.0 };
                ((p[0] - coords[0]) * sx).powi(2) + ((p[1] - coords[1]) * sy).powi(2) + dz * dz
            };
            let mut best = 0;
            for (k, p) in pts.iter().enumerate() {
                if dist(p) < dist(&pts[best]) {
                    best = k;
                }
            }
            (best + 1) as f64
        }
        None => 1.0,
    };
    let _ = three;
    let id = gfx::alloc(it);
    let mut anchor = anchor(DATATIP);
    anchor.id = id;
    it.figures[f].axes[ax].series.push(anchor);
    put(it, id, vec![(DT_PARENT, Value::num(pid as f64)), ("Tag", Value::chars("datatip"))]);
    if let Some(loc) = take(&mut props, "Location") {
        put(it, id, vec![("Location", Value::chars(&text(&loc))), ("LocationMode", Value::chars("manual"))]);
    }
    place_tip(it, id, index)?;
    finish(it, &[id], &props, nargout)
}

/// The label part of a data tip.
fn datatip_parts(it: &mut Interpreter, id: u64, a: usize) -> R<Vec<(usize, Series)>> {
    let x = get(it, id, "X").scalar().unwrap_or(f64::NAN);
    let y = get(it, id, "Y").scalar().unwrap_or(f64::NAN);
    let size = get(it, id, "FontSize").scalar().ok().map(|v| v as f32);
    let mut lines = vec![format!("X {}", tip_number(x)), format!("Y {}", tip_number(y))];
    if gfx::stored(it, id, "\u{1}dt3d").is_some() {
        lines.push(format!("Z {}", tip_number(get(it, id, "Z").scalar().unwrap_or(f64::NAN))));
    }
    let mut t = label(x, y, &lines.join("\n"), [0, 0, 0], size);
    t.centered = false;
    if gfx::stored(it, id, "\u{1}dt3d").is_some() {
        t.z = vec![get(it, id, "Z").scalar().unwrap_or(0.0)];
    }
    Ok(vec![(a, t)])
}

fn v_scalar(it: &Interpreter, id: u64, name: &str) -> f64 {
    gfx::stored(it, id, name).and_then(|v| v.scalar().ok()).unwrap_or(1.0)
}
