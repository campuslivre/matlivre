//! Figure files: `savefig`, `hgsave`, `openfig` and `hgload`.
//!
//! A `.fig` file is a MAT-file whose `hgS_070000` variable is a tree of structs with the fields
//! `type`, `handle`, `properties`, `children` and `special`: the figure, its axes, legends and
//! colorbars, and in each axes its plotted objects and label texts (`special` holds the
//! positions of the title and x, y and z labels among the children). Figures are rebuilt with
//! the plotting functions and `set`, so opened objects behave as if they had been drawn.

use crate::error::{rt, LangError};
use crate::gfx;
use crate::graphics;
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &["savefig", "hgsave", "openfig", "hgload"];

const NODE_FIELDS: [&str; 5] = ["type", "handle", "properties", "children", "special"];

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> R<Option<Vec<Value>>> {
    match name {
        "savefig" | "hgsave" => {
            save(it, name, a)?;
            Ok(Some(Vec::new()))
        }
        "openfig" | "hgload" => {
            let file = match a.first() {
                Some(v) => v.text()?,
                None if name == "openfig" => return rt("Not enough input arguments."),
                None => return rt("Not enough input arguments."),
            };
            let fig = open(it, &file)?;
            Ok(Some(vec![fig]))
        }
        _ => Ok(None),
    }
}

/// `file` with the `.fig` extension when it has none.
fn with_fig_ext(file: &str) -> String {
    let p = std::path::Path::new(file);
    if p.extension().is_some() {
        file.to_string()
    } else {
        format!("{file}.fig")
    }
}

fn absolute(file: &str) -> String {
    let p = std::path::Path::new(file);
    if p.is_absolute() {
        file.to_string()
    } else {
        std::env::current_dir().map(|d| d.join(p).to_string_lossy().into_owned()).unwrap_or_else(|_| file.to_string())
    }
}

// ---- reading --------------------------------------------------------------------------

fn open(it: &mut Interpreter, file: &str) -> R<Value> {
    let path = if std::path::Path::new(file).exists() { file.to_string() } else { with_fig_ext(file) };
    if !std::path::Path::new(&path).exists() {
        return rt(format!("Unable to find file or directory '{}'.", absolute(&with_fig_ext(file))));
    }
    let vars = crate::builtins_data::read_mat(&path)?;
    let root = vars.into_iter().find(|(n, _)| n == "hgS_070000").and_then(|(_, v)| v);
    let Some(Value::Struct(root)) = root else {
        return rt(format!("'{file}' is not a valid figure file."));
    };
    build_figure(it, &root)
}

fn field(s: &StructArray, i: usize, name: &str) -> Option<Value> {
    s.get_at(i, name).cloned()
}

fn text_of(v: &Option<Value>) -> String {
    v.as_ref().and_then(|v| v.text().ok()).unwrap_or_default()
}

/// The property struct of node `i`.
fn props(s: &StructArray, i: usize) -> StructArray {
    match field(s, i, "properties") {
        Some(Value::Struct(p)) if p.numel() == 1 => p,
        _ => StructArray::scalar(),
    }
}

fn children(s: &StructArray, i: usize) -> StructArray {
    match field(s, i, "children") {
        Some(Value::Struct(c)) => c,
        _ => StructArray::empty_with(Vec::new()),
    }
}

fn num_of(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Num(m)) if m.data.len() == 1 => Some(m.data[0]),
        _ => None,
    }
}

fn call1(it: &mut Interpreter, name: &str, args: Vec<Value>) -> R<Value> {
    let out = it.call_function(name, args, 1)?;
    Ok(out.into_iter().next().unwrap_or_else(Value::empty))
}

/// `set(h, name, value)` for each listed property the node has; unsupported ones are skipped.
fn set_props(it: &mut Interpreter, h: &Value, p: &StructArray, names: &[&str]) {
    for &n in names {
        if let Some(v) = p.get(n) {
            let _ = it.call_function("set", vec![h.clone(), Value::chars(n), v.clone()], 0);
        }
    }
}

/// Properties whose value applies only when the file has it in manual mode.
fn set_manual(it: &mut Interpreter, h: &Value, p: &StructArray, names: &[&str]) {
    for &n in names {
        let mode = p.get(&format!("{n}Mode")).and_then(|v| v.text().ok());
        if p.get(n).is_some() && mode.as_deref().unwrap_or("manual") == "manual" {
            set_props(it, h, p, &[n]);
        }
    }
}

fn build_figure(it: &mut Interpreter, root: &StructArray) -> R<Value> {
    let fig = call1(it, "figure", Vec::new())?;
    let fp = props(root, 0);
    if let Some(cmap) = fp.get("Colormap") {
        let _ = it.call_function("colormap", vec![fig.clone(), cmap.clone()], 0);
    }
    set_props(it, &fig, &fp, &["Name"]);
    let kids = children(root, 0);
    let types: Vec<String> = (0..kids.numel()).map(|i| text_of(&field(&kids, i, "type"))).collect();
    // Axes go into a grid by their positions (subplot layouts).
    let axes: Vec<usize> = (0..kids.numel()).filter(|&i| types[i] == "axes").collect();
    let pos: Vec<[f64; 4]> = axes
        .iter()
        .map(|&i| match props(&kids, i).get("Position") {
            Some(Value::Num(m)) if m.data.len() == 4 => [m.data[0], m.data[1], m.data[2], m.data[3]],
            _ => [0.13, 0.11, 0.775, 0.815],
        })
        .collect();
    let distinct = |vals: Vec<f64>| {
        let mut out: Vec<f64> = Vec::new();
        for v in vals {
            if !out.iter().any(|&o| (o - v).abs() < 1e-3) {
                out.push(v);
            }
        }
        out.sort_by(|a, b| a.total_cmp(b));
        out
    };
    let lefts = distinct(pos.iter().map(|p| p[0]).collect());
    let mut bottoms = distinct(pos.iter().map(|p| p[1]).collect());
    bottoms.reverse();
    let (rows, cols) = (bottoms.len().max(1), lefts.len().max(1));
    let mut by_handle: Vec<(f64, Value)> = Vec::new();
    let mut last_axes: Option<Value> = None;
    let mut used: Vec<usize> = Vec::new();
    for (k, &i) in axes.iter().enumerate() {
        let r = bottoms.iter().position(|&b| (b - pos[k][1]).abs() < 1e-3).unwrap_or(0);
        let c = lefts.iter().position(|&l| (l - pos[k][0]).abs() < 1e-3).unwrap_or(0);
        let slot = r * cols + c + 1;
        let ax = if rows * cols > 1 && !used.contains(&slot) {
            used.push(slot);
            call1(it, "subplot", vec![Value::num(rows as f64), Value::num(cols as f64), Value::num(slot as f64)])?
        } else if k == 0 {
            call1(it, "axes", vec![fig.clone()])?
        } else {
            continue;
        };
        build_axes(it, &ax, &kids, i)?;
        if let Some(h) = num_of(kids.get_at(i, "handle")) {
            by_handle.push((h, ax.clone()));
        }
        last_axes = Some(ax);
    }
    let axes_for = |p: &StructArray, by: &[(f64, Value)], last: &Option<Value>| -> Option<Value> {
        match num_of(p.get("Axes")) {
            Some(h) => by.iter().find(|(x, _)| (x - h).abs() < 1e-9).map(|(_, a)| a.clone()).or_else(|| last.clone()),
            None => last.clone(),
        }
    };
    for i in 0..kids.numel() {
        let p = props(&kids, i);
        match types[i].as_str() {
            "scribe.legend" => {
                let Some(ax) = axes_for(&p, &by_handle, &last_axes) else { continue };
                let mut args = vec![ax];
                if let Some(s) = p.get("String") {
                    args.push(s.clone());
                }
                if let Some(loc) = p.get("Location") {
                    args.push(Value::chars("Location"));
                    args.push(loc.clone());
                }
                let _ = it.call_function("legend", args, 0);
            }
            "scribe.colorbar" => {
                if let Some(ax) = axes_for(&p, &by_handle, &last_axes) {
                    let _ = it.call_function("colorbar", vec![ax], 0);
                }
            }
            _ => {}
        }
    }
    let _ = it.call_function("figure", vec![fig.clone()], 0);
    Ok(fig)
}

fn build_axes(it: &mut Interpreter, ax: &Value, nodes: &StructArray, i: usize) -> R<()> {
    let p = props(nodes, i);
    let kids = children(nodes, i);
    let special: Vec<usize> = match field(nodes, i, "special") {
        Some(Value::Num(m)) => m.data.iter().map(|&x| x.max(0.0) as usize).collect(),
        _ => Vec::new(),
    };
    let _ = it.call_function("hold", vec![ax.clone(), Value::chars("on")], 0);
    // Children are listed oldest first (the reverse of the Children property).
    for k in 0..kids.numel() {
        if special.contains(&(k + 1)) {
            continue;
        }
        let t = text_of(&field(&kids, k, "type"));
        let _ = build_object(it, ax, &t, &props(&kids, k));
    }
    let held = p.get("NextPlot").and_then(|v| v.text().ok()).is_some_and(|n| n == "add");
    if !held {
        let _ = it.call_function("hold", vec![ax.clone(), Value::chars("off")], 0);
    }
    for (k, fname) in ["title", "xlabel", "ylabel", "zlabel"].iter().enumerate() {
        let Some(&idx) = special.get(k) else { continue };
        if idx == 0 || idx > kids.numel() {
            continue;
        }
        if let Some(s) = props(&kids, idx - 1).get("String") {
            let empty = s.text().map(|t| t.is_empty()).unwrap_or(false);
            if !empty {
                let _ = it.call_function(fname, vec![ax.clone(), s.clone()], 0);
            }
        }
    }
    // One grid switch here: the x and y grids decide it.
    set_props(it, ax, &p, &["View", "XGrid", "YGrid", "Box", "XDir", "YDir", "ZDir", "XScale", "YScale", "ZScale"]);
    set_manual(it, ax, &p, &["XLim", "YLim", "ZLim", "CLim", "XTick", "YTick", "ZTick", "XTickLabel", "YTickLabel", "DataAspectRatio"]);
    Ok(())
}

fn build_object(it: &mut Interpreter, ax: &Value, t: &str, p: &StructArray) -> R<()> {
    let get = |n: &str| p.get(n).cloned();
    let present = |v: &Option<Value>| v.as_ref().is_some_and(|v| v.numel() > 0);
    let (x, y, z) = (get("XData"), get("YData"), get("ZData"));
    let common = ["DisplayName", "Visible", "Tag"];
    match t {
        "graph2d.lineseries" | "line" => {
            let (Some(x), Some(y)) = (x, y) else { return Ok(()) };
            let h = match z.filter(|z| z.numel() > 0) {
                Some(z) => call1(it, "plot3", vec![ax.clone(), x, y, z])?,
                None => call1(it, "plot", vec![ax.clone(), x, y])?,
            };
            set_props(it, &h, p, &["Color", "LineStyle", "LineWidth", "Marker", "MarkerSize", "MarkerFaceColor", "MarkerEdgeColor"]);
            set_props(it, &h, p, &common);
        }
        "specgraph.scattergroup" | "scatter" => {
            let (Some(x), Some(y)) = (x, y) else { return Ok(()) };
            let h = match z.filter(|z| z.numel() > 0) {
                Some(z) => call1(it, "scatter3", vec![ax.clone(), x, y, z])?,
                None => call1(it, "scatter", vec![ax.clone(), x, y])?,
            };
            set_props(it, &h, p, &["SizeData", "CData", "Marker", "MarkerFaceColor", "MarkerEdgeColor", "LineWidth"]);
            set_props(it, &h, p, &common);
        }
        "specgraph.barseries" | "bar" => {
            let Some(y) = y else { return Ok(()) };
            let h = match x {
                Some(x) => call1(it, "bar", vec![ax.clone(), x, y])?,
                None => call1(it, "bar", vec![ax.clone(), y])?,
            };
            set_props(it, &h, p, &["FaceColor", "EdgeColor", "BarWidth", "LineWidth"]);
            set_props(it, &h, p, &common);
        }
        "specgraph.stairseries" | "stair" | "specgraph.stemseries" | "stem" | "specgraph.areaseries" | "area" => {
            let (Some(x), Some(y)) = (x, y) else { return Ok(()) };
            let f = if t.contains("stair") {
                "stairs"
            } else if t.contains("stem") {
                "stem"
            } else {
                "area"
            };
            let h = call1(it, f, vec![ax.clone(), x, y])?;
            set_props(it, &h, p, &["Color", "FaceColor", "EdgeColor", "LineStyle", "LineWidth", "Marker"]);
            set_props(it, &h, p, &common);
        }
        "graph3d.surfaceplot" | "surface" => {
            let Some(z) = z else { return Ok(()) };
            let mut args = vec![ax.clone()];
            if present(&x) && present(&y) {
                args.push(x.unwrap());
                args.push(y.unwrap());
            }
            args.push(z);
            if let Some(c) = get("CData").filter(|c| c.numel() > 0) {
                args.push(c);
            }
            let h = call1(it, "surf", args)?;
            set_props(it, &h, p, &["FaceColor", "EdgeColor", "FaceAlpha", "LineStyle"]);
            set_props(it, &h, p, &common);
        }
        "image" => {
            let Some(c) = get("CData") else { return Ok(()) };
            let mut args = vec![ax.clone()];
            if present(&x) && present(&y) {
                args.push(x.unwrap());
                args.push(y.unwrap());
            }
            args.push(c);
            let h = call1(it, "image", args)?;
            set_props(it, &h, p, &["CDataMapping", "AlphaData"]);
            set_props(it, &h, p, &common);
        }
        "text" => {
            let pos = match get("Position") {
                Some(Value::Num(m)) if m.data.len() >= 2 => m.data.clone(),
                _ => vec![0.0, 0.0],
            };
            let s = get("String").unwrap_or_else(|| Value::chars(""));
            let h = call1(it, "text", vec![ax.clone(), Value::num(pos[0]), Value::num(pos[1]), s])?;
            set_props(it, &h, p, &["Color", "FontSize", "FontWeight", "FontAngle", "HorizontalAlignment", "VerticalAlignment", "Rotation", "Interpreter"]);
            set_props(it, &h, p, &common);
        }
        _ => {}
    }
    Ok(())
}

// ---- writing --------------------------------------------------------------------------

fn save(it: &mut Interpreter, name: &str, a: &[Value]) -> R<()> {
    let (fig, file) = match a {
        [] => (None, "Untitled.fig".to_string()),
        [v] if gfx::is_gfx(v) => (Some(v.clone()), "Untitled.fig".to_string()),
        [v, ..] if !gfx::is_gfx(v) => (None, v.text()?),
        [h, f, ..] => (Some(h.clone()), f.text()?),
        _ => unreachable!(),
    };
    let fig = match fig {
        Some(h) => h,
        None => call1(it, "gcf", Vec::new())?,
    };
    let ids = gfx::ids_of(&fig).unwrap_or_default();
    let Some(&id) = ids.first() else {
        return rt(format!("Invalid figure handle passed to {name}."));
    };
    if gfx::get_prop_raw(it, id, "Type").ok().and_then(|t| t.text().ok()).as_deref() != Some("figure") {
        return rt(format!("Invalid figure handle passed to {name}."));
    }
    let tree = figure_node(it, id)?;
    let path = with_fig_ext(&file);
    crate::builtins_data::write_mat(&path, &[("hgS_070000".to_string(), Value::Struct(tree))], false)
}

/// The handle number written for object `id`: never a whole number, which would be a figure.
fn hv(id: u64) -> f64 {
    id as f64 / gfx::SLOTS as f64 + 1e-4
}

fn node(t: &str, handle: f64, props: Vec<(String, Value)>, kids: Vec<StructArray>, special: Value) -> StructArray {
    let mut p = StructArray::scalar();
    for (n, v) in props {
        p.set(&n, v);
    }
    let children = if kids.is_empty() {
        Value::empty()
    } else {
        let mut s = StructArray { dims: vec![kids.len(), 1], fields: NODE_FIELDS.iter().map(|f| f.to_string()).collect(), elems: Vec::new() };
        for k in kids {
            s.elems.push(k.elems.into_iter().next().unwrap());
        }
        Value::Struct(s)
    };
    let mut s = StructArray::scalar();
    s.set("type", Value::chars(t));
    s.set("handle", Value::num(handle));
    s.set("properties", Value::Struct(p));
    s.set("children", children);
    s.set("special", special);
    s
}

/// The listed properties of object `id` that it has.
fn read_props(it: &mut Interpreter, id: u64, names: &[&str]) -> Vec<(String, Value)> {
    names.iter().filter_map(|&n| gfx::get_prop_raw(it, id, n).ok().map(|v| (n.to_string(), v))).collect()
}

fn on_off(it: &mut Interpreter, id: u64, n: &str) -> Option<(String, Value)> {
    let v = gfx::get_prop_raw(it, id, n).ok()?;
    let t = v.text().ok().or_else(|| match &v {
        Value::Num(m) if m.data.len() == 1 => Some(if m.data[0] != 0.0 { "on".into() } else { "off".into() }),
        _ => None,
    })?;
    Some((n.to_string(), Value::chars(&t)))
}

fn figure_node(it: &mut Interpreter, fid: u64) -> R<StructArray> {
    let f = match gfx::resolve(it, fid) {
        Some(gfx::Target::Figure(f)) => f,
        _ => return rt("Invalid figure handle."),
    };
    let mut props = vec![("Position".to_string(), Value::Num(Matrix::row(vec![680.0, 458.0, 560.0, 420.0])))];
    props.extend(read_props(it, fid, &["Color", "Colormap"]));
    let name = it.figures[f].name.clone();
    if !name.is_empty() {
        props.push(("Name".to_string(), Value::chars(&name)));
    }
    let (rows, cols) = it.figures[f].grid;
    let (rows, cols) = (rows.max(1), cols.max(1));
    let n = it.figures[f].axes.len();
    let mut kids = Vec::new();
    let mut extras = Vec::new();
    for a in 0..n {
        if it.figures[f].axes[a].overlay_of != 0 {
            continue;
        }
        // Grid cells never drawn into are not saved.
        if rows * cols > 1 && it.figures[f].axes[a].series.is_empty() && it.figures[f].axes[a].id == 0 {
            continue;
        }
        let id = gfx::axes_id(it, f, a);
        let slot = a % (rows * cols);
        let (r, c) = (slot / cols, slot % cols);
        let sw = 0.1056 / 0.3347;
        let sh = 0.1326 / 0.3412;
        let w = 0.775 / (cols as f64 + (cols as f64 - 1.0) * sw);
        let h = 0.815 / (rows as f64 + (rows as f64 - 1.0) * sh);
        let pos = [0.13 + c as f64 * w * (1.0 + sw), 0.11 + (rows - 1 - r) as f64 * h * (1.0 + sh), w, h];
        kids.push(axes_node(it, id, pos)?);
        if let Ok(leg) = gfx::get_prop_raw(it, id, "Legend") {
            if let Some(&lid) = gfx::ids_of(&leg).and_then(|v| v.first().copied()).as_ref() {
                // MATLAB needs the legend's layout fields and the handles of its entries.
                let entries: Vec<f64> = it.figures[f].axes[a]
                    .series
                    .iter()
                    .filter(|s| !matches!(s.kind, graphics::SeriesKind::Text | graphics::SeriesKind::ConstantLine | graphics::SeriesKind::Region) && s.visible)
                    .map(|s| hv(s.id))
                    .collect();
                let mut lp = read_props(it, lid, &["String", "Location"]);
                let row = |v: &[f64]| Value::Num(Matrix::row(v.to_vec()));
                lp.extend([
                    ("Position".to_string(), row(&[pos[0] + pos[2] - 0.12, pos[1] + pos[3] - 0.09, 0.1, 0.08])),
                    ("PositionMode".to_string(), Value::chars("auto")),
                    ("Box".to_string(), Value::chars("on")),
                    ("EdgeColor".to_string(), row(&[0.15, 0.15, 0.15])),
                    ("FontSize".to_string(), Value::num(9.0)),
                    ("PlotChildrenMode".to_string(), Value::chars("auto")),
                    ("PlotChildrenExcluded".to_string(), Value::Num(Matrix::new(0, 1, Vec::new()))),
                    ("PlotChildrenExcludedMode".to_string(), Value::chars("auto")),
                    ("PlotChildrenSpecified".to_string(), Value::empty()),
                    ("Visible".to_string(), Value::chars("on")),
                    ("ApplicationData".to_string(), Value::Struct(StructArray::scalar())),
                    ("Interruptible".to_string(), Value::chars("off")),
                    ("Tag".to_string(), Value::chars("legend")),
                    ("TagMode".to_string(), Value::chars("auto")),
                    ("Layout".to_string(), Value::empty()),
                    ("XTickLabel".to_string(), Value::chars("")),
                    ("YTickLabel".to_string(), Value::chars("")),
                    ("XTick".to_string(), Value::num(-1.0)),
                    ("YTick".to_string(), Value::num(-1.0)),
                    ("XLim".to_string(), row(&[0.0, 1.0])),
                    ("YLim".to_string(), row(&[0.0, 1.0])),
                    ("Axes".to_string(), Value::num(hv(id))),
                    ("PlotChildren".to_string(), Value::Num(Matrix::new(entries.len(), 1, entries))),
                ]);
                extras.push(node("scribe.legend", hv(lid), lp, Vec::new(), Value::empty()));
            }
        }
        if it.figures[f].axes[a].colorbar {
            let (c0, c1) = match gfx::get_prop_raw(it, id, "CLim") {
                Ok(Value::Num(m)) if m.data.len() == 2 => (m.data[0], m.data[1]),
                _ => (0.0, 1.0),
            };
            let row = |v: &[f64]| Value::Num(Matrix::row(v.to_vec()));
            let lp = vec![
                ("Position".to_string(), row(&[pos[0] + pos[2] + 0.02, pos[1], 0.038, pos[3]])),
                ("PositionMode".to_string(), Value::chars("auto")),
                ("Location".to_string(), Value::chars("eastoutside")),
                ("YAxisLocation".to_string(), Value::chars("right")),
                ("XLim".to_string(), row(&[-1.0, 2.0])),
                ("YLim".to_string(), row(&[c0, c1])),
                ("Box".to_string(), Value::chars("on")),
                ("Limits".to_string(), row(&[c0, c1])),
                ("LimitsMode".to_string(), Value::chars("auto")),
                ("ApplicationData".to_string(), Value::Struct(StructArray::scalar())),
                ("Interruptible".to_string(), Value::chars("off")),
                ("Tag".to_string(), Value::chars("Colorbar")),
                ("TagMode".to_string(), Value::chars("auto")),
                ("Layout".to_string(), Value::empty()),
                ("Axes".to_string(), Value::num(hv(id))),
            ];
            extras.push(node("scribe.colorbar", hv(id) + 3e-5, lp, Vec::new(), Value::empty()));
        }
    }
    kids.extend(extras);
    Ok(node("figure", it.figures[f].id as f64, props, kids, Value::empty()))
}

fn axes_node(it: &mut Interpreter, id: u64, pos: [f64; 4]) -> R<StructArray> {
    let mut props = vec![("Position".to_string(), Value::Num(Matrix::row(pos.to_vec())))];
    for n in ["XLim", "YLim", "ZLim", "CLim", "XTick", "YTick", "ZTick"] {
        let mode = gfx::get_prop_raw(it, id, &format!("{n}Mode")).ok().and_then(|v| v.text().ok()).unwrap_or_else(|| "auto".into());
        if let Ok(v) = gfx::get_prop_raw(it, id, n) {
            props.push((n.to_string(), v));
            props.push((format!("{n}Mode"), Value::chars(&mode)));
        }
    }
    props.extend(read_props(it, id, &["View", "YDir", "XDir", "XScale", "YScale", "ZScale"]));
    for n in ["XGrid", "YGrid", "Box"] {
        props.extend(on_off(it, id, n));
    }
    if gfx::get_prop_raw(it, id, "NextPlot").ok().and_then(|v| v.text().ok()).as_deref() == Some("add") {
        props.push(("NextPlot".to_string(), Value::chars("add")));
    }
    let kid_ids = gfx::get_prop_raw(it, id, "Children").ok().and_then(|v| gfx::ids_of(&v)).unwrap_or_default();
    let mut kids: Vec<StructArray> = Vec::new();
    // Bars refer to a baseline object, which MATLAB needs to rebuild them.
    let baseline = hv(id) + 2e-5;
    let mut has_bars = false;
    // Oldest first, as MATLAB writes them.
    for k in kid_ids.into_iter().rev() {
        if let Some(mut n) = object_node(it, k) {
            if n.get("type").and_then(|t| t.text().ok()).as_deref() == Some("specgraph.barseries") {
                if let Some(Value::Struct(p)) = n.get("properties").cloned() {
                    let mut p = p;
                    p.set("BaseLine", Value::num(baseline));
                    p.set("BarPeers", Value::num(0.0));
                    p.set("BaseValue", Value::num(0.0));
                    n.set("properties", Value::Struct(p));
                }
                has_bars = true;
            }
            kids.push(n);
        }
    }
    if has_bars {
        let row = |v: &[f64]| Value::Num(Matrix::row(v.to_vec()));
        let bp = vec![
            ("Color".to_string(), row(&[0.15, 0.15, 0.15])),
            ("ColorMode".to_string(), Value::chars("auto")),
            ("Axis".to_string(), Value::num(1.0)),
            ("AxisMode".to_string(), Value::chars("auto")),
            ("Description".to_string(), Value::chars("Axes YBaseline")),
            ("DescriptionMode".to_string(), Value::chars("auto")),
            ("HandleVisibility".to_string(), Value::chars("off")),
            ("HandleVisibilityMode".to_string(), Value::chars("auto")),
            ("Internal".to_string(), Value::bool(true)),
            ("Orientation".to_string(), Value::chars("X")),
        ];
        kids.push(node("specgraph.baseline", baseline, bp, Vec::new(), Value::empty()));
    }
    let mut special = vec![0.0; 4];
    for (s, part) in ["Title", "XLabel", "YLabel", "ZLabel"].iter().enumerate() {
        let Some(h) = gfx::get_prop_raw(it, id, part).ok().and_then(|v| gfx::ids_of(&v)).and_then(|v| v.first().copied()) else { continue };
        let text = gfx::get_prop_raw(it, h, "String").ok().and_then(|v| v.text().ok()).unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        let tp = read_props(it, h, &["String", "Color", "FontSize", "FontWeight"]);
        kids.push(node("text", hv(h), tp, Vec::new(), Value::empty()));
        special[s] = kids.len() as f64;
    }
    Ok(node("axes", hv(id), props, kids, Value::Num(Matrix::new(4, 1, special))))
}

fn object_node(it: &mut Interpreter, id: u64) -> Option<StructArray> {
    let ty = gfx::get_prop_raw(it, id, "Type").ok()?.text().ok()?;
    let (t, names): (&str, &[&str]) = match ty.as_str() {
        "line" => ("graph2d.lineseries", &["XData", "YData", "ZData", "Color", "LineStyle", "LineWidth", "Marker", "MarkerSize", "MarkerFaceColor", "MarkerEdgeColor", "DisplayName"]),
        "scatter" => ("specgraph.scattergroup", &["XData", "YData", "ZData", "SizeData", "CData", "Marker", "MarkerFaceColor", "MarkerEdgeColor", "LineWidth", "DisplayName"]),
        "bar" => ("specgraph.barseries", &["XData", "YData", "FaceColor", "EdgeColor", "BarWidth", "LineWidth", "DisplayName"]),
        "stair" => ("specgraph.stairseries", &["XData", "YData", "Color", "LineStyle", "LineWidth", "Marker", "DisplayName"]),
        "stem" => ("specgraph.stemseries", &["XData", "YData", "Color", "LineStyle", "LineWidth", "Marker", "DisplayName"]),
        "area" => ("specgraph.areaseries", &["XData", "YData", "FaceColor", "EdgeColor", "LineStyle", "LineWidth", "DisplayName"]),
        "surface" => ("graph3d.surfaceplot", &["XData", "YData", "ZData", "CData", "FaceColor", "EdgeColor", "FaceAlpha", "LineStyle"]),
        "image" => ("image", &["XData", "YData", "CData", "CDataMapping"]),
        "text" => ("text", &["Position", "String", "Color", "FontSize", "FontWeight", "FontAngle", "HorizontalAlignment", "VerticalAlignment", "Rotation", "Interpreter"]),
        _ => return None,
    };
    let mut props = read_props(it, id, names);
    props.retain(|(n, v)| !(n == "ZData" && v.numel() == 0) && !(n == "DisplayName" && v.text().map(|t| t.is_empty()).unwrap_or(false)));
    Some(node(t, hv(id), props, Vec::new(), Value::empty()))
}
