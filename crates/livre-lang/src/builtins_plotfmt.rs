//! Axes formatting (`xticks`, `xtickformat`, `xscale`, `zlim`, ...), annotations (`text`,
//! `xline`, `xregion`), more 2-D plot types (`fill`/`patch`, `errorbar`, `barh`, `line`,
//! `semilogx`, ...) and saving figures (`saveas`, `print`, `exportgraphics`).

use crate::builtins::{arg, none, one, Ret};
use crate::builtins_plot::{apply_props, handles_out, plot_series};
use crate::error::{rt, LangError};
use crate::gfx;
use crate::graphics::{self, ExportFormat, LineStyle, Series, SeriesKind, XAxisKind};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "xticks", "yticks", "zticks", "xticklabels", "yticklabels", "zticklabels", "xtickformat", "ytickformat", "ztickformat", "xtickangle", "ytickangle",
    "ztickangle", "xscale", "yscale", "zscale", "semilogx", "semilogy", "loglog", "zlim", "text", "xline", "yline", "xregion", "yregion", "fill", "patch",
    "errorbar", "barh", "line", "saveas", "print", "exportgraphics", "fontsize", "colorbar",
];

fn texts(v: &Value) -> R<Vec<String>> {
    Ok(match v {
        Value::Cell(c) => c.data.iter().map(|x| if x.numel() == 0 { Ok(String::new()) } else { x.text() }).collect::<R<_>>()?,
        Value::Str(s) => s.data.iter().map(|x| x.clone().unwrap_or_default()).collect(),
        Value::Num(m) if m.kind == Kind::Char && m.rows() > 1 => (0..m.rows()).map(|r| (0..m.cols()).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or(' ')).collect::<String>().trim_end().to_string()).collect(),
        Value::Num(m) if m.kind != Kind::Char => m.data.iter().map(|&x| crate::display::fmt_g(x, 5)).collect(),
        other => vec![other.text()?],
    })
}

fn is_textlike(v: &Value) -> bool {
    v.is_text() || matches!(v, Value::Str(_))
}

/// Gets or sets an axes property through the current axes handle.
fn axes_prop(it: &mut Interpreter, name: &str, value: Option<Value>) -> R<Value> {
    let h = it.current_axes_handle();
    match value {
        Some(v) => {
            gfx::call(it, "set", &[h, Value::chars(name), v], 0)?;
            Ok(Value::empty())
        }
        None => Ok(gfx::call(it, "get", &[h, Value::chars(name)], 1)?.and_then(|v| v.into_iter().next()).unwrap_or_else(Value::empty)),
    }
}

/// Splits trailing name-value pairs (from the first text argument at an even offset
/// from `from`) off the arguments.
fn split_props(a: &[Value], from: usize) -> R<(&[Value], Vec<(String, Value)>)> {
    let mut i = from;
    while i < a.len() && !is_textlike(&a[i]) {
        i += 1;
    }
    let (head, tail) = a.split_at(i);
    if tail.len() % 2 != 0 {
        return rt(format!("Invalid parameter/value pair arguments ('{}').", tail.last().map(|v| v.text().unwrap_or_default()).unwrap_or_default()));
    }
    let props = tail.chunks(2).map(|p| Ok((p[0].text()?, p[1].clone()))).collect::<R<Vec<_>>>()?;
    Ok((head, props))
}

/// A color argument for fills: a short or long color name, or an RGB triplet.
fn fill_color(v: &Value) -> Option<[u8; 3]> {
    if is_textlike(v) {
        let t = v.text().ok()?;
        return graphics::named_color(&t).or_else(|| graphics::parse_line_spec(&t).and_then(|p| p.0));
    }
    let m = v.to_matrix().ok()?;
    (m.numel() == 3 && m.data.iter().all(|c| (0.0..=1.0).contains(c))).then(|| [(m.data[0] * 255.0).round() as u8, (m.data[1] * 255.0).round() as u8, (m.data[2] * 255.0).round() as u8])
}

/// The figure to save: a figure or axes handle argument, or the current figure.
fn figure_of(it: &mut Interpreter, v: Option<&Value>) -> R<usize> {
    if let Some(ids) = v.and_then(gfx::ids_of) {
        return match ids.first().and_then(|&id| gfx::resolve(it, id)) {
            Some(gfx::Target::Figure(f) | gfx::Target::Axes(f, _) | gfx::Target::Series(f, _, _) | gfx::Target::Part(f, _, _)) => Ok(f),
            _ => rt("Invalid or deleted object."),
        };
    }
    if let Some(v) = v.filter(|v| !is_textlike(v)) {
        let n = v.scalar()? as u32;
        return it.figures.iter().position(|f| f.id == n).ok_or_else(|| LangError::new("Invalid or deleted object.", 0));
    }
    Ok(it.current_figure_index())
}

/// Writes figure `f` to `path` as `format` (png, jpg, bmp, tif, gif or svg).
fn export(it: &mut Interpreter, f: usize, path: &str, format: &str, dpi: f64) -> R<()> {
    let fig = it.figures[f].clone();
    let Some(exporter) = it.exporter.as_mut() else {
        return rt("Saving figures needs the matlivre editor's renderer, which is not available here.");
    };
    let scale = (dpi / 72.0) as f32;
    let image_format = match format {
        "svg" => {
            let svg = exporter(&fig, ExportFormat::Svg, 1.0).map_err(|e| LangError::new(e, 0))?;
            return std::fs::write(path, svg).map_err(|e| LangError::new(format!("Unable to write file '{path}': {e}"), 0));
        }
        "png" => None,
        "jpg" | "jpeg" => Some(image::ImageFormat::Jpeg),
        "bmp" => Some(image::ImageFormat::Bmp),
        "tif" | "tiff" => Some(image::ImageFormat::Tiff),
        "gif" => Some(image::ImageFormat::Gif),
        other => return rt(format!("Unsupported format '{other}'. Use png, jpg, bmp, tif, gif or svg.")),
    };
    let png = exporter(&fig, ExportFormat::Png, scale).map_err(|e| LangError::new(e, 0))?;
    match image_format {
        None => std::fs::write(path, png).map_err(|e| LangError::new(format!("Unable to write file '{path}': {e}"), 0)),
        Some(fmt) => {
            let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png).map_err(|e| LangError::new(e.to_string(), 0))?;
            image::DynamicImage::ImageRgb8(img.to_rgb8()).save_with_format(path, fmt).map_err(|e| LangError::new(format!("Unable to write file '{path}': {e}"), 0))
        }
    }
}

fn ext_of(path: &str) -> String {
    std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn saveas(it: &mut Interpreter, a: &[Value]) -> Ret {
    let f = figure_of(it, Some(arg(a, 0, "saveas")?))?;
    let mut path = arg(a, 1, "saveas")?.text()?;
    let format = match a.get(2) {
        Some(v) => v.text()?.to_lowercase(),
        None if ext_of(&path).is_empty() => "fig".to_string(),
        None => ext_of(&path),
    };
    let format = match format.as_str() {
        "jpeg" => "jpg".to_string(),
        "tiff" | "tiffn" => "tif".to_string(),
        "fig" | "m" => return rt("Saving MATLAB figure files (.fig) is not supported; save as png, jpg, svg, ... instead."),
        other => other.to_string(),
    };
    if ext_of(&path).is_empty() {
        path = format!("{path}.{format}");
    }
    export(it, f, &path, &format, 150.0)?;
    none()
}

fn print(it: &mut Interpreter, a: &[Value]) -> Ret {
    let mut fig = None;
    let mut path = None;
    let mut format = "png".to_string();
    let mut dpi = 150.0;
    for v in a {
        if gfx::is_gfx(v) || (!is_textlike(v) && fig.is_none()) {
            fig = Some(v.clone());
            continue;
        }
        let t = v.text()?;
        if let Some(d) = t.strip_prefix("-d") {
            format = match d {
                "png" => "png",
                "jpeg" | "jpg" => "jpg",
                "bmp" | "bmp16m" | "bmp256" => "bmp",
                "tiff" | "tiffn" => "tif",
                "svg" => "svg",
                "meta" | "pdf" | "epsc" | "eps" | "epsc2" | "eps2" => return rt(format!("The '-d{d}' format is not supported; use -dpng, -djpeg, -dtiff, -dbmp or -dsvg.")),
                other => return rt(format!("Unrecognized output format '-d{other}'.")),
            }
            .to_string();
        } else if let Some(r) = t.strip_prefix("-r") {
            dpi = r.parse::<f64>().ok().filter(|&d| d > 0.0).unwrap_or(150.0);
        } else if t.starts_with('-') {
            // Other options (-painters, -opengl, -noui, -fillpage, ...) do not change the image.
        } else {
            path = Some(t);
        }
    }
    let Some(mut path) = path else {
        return rt("Printing to a printer is not supported; give a file name.");
    };
    if ext_of(&path).is_empty() {
        path = format!("{path}.{format}");
    }
    let f = figure_of(it, fig.as_ref())?;
    export(it, f, &path, &format, dpi)?;
    none()
}

fn exportgraphics(it: &mut Interpreter, a: &[Value]) -> Ret {
    let f = figure_of(it, Some(arg(a, 0, "exportgraphics")?))?;
    let path = arg(a, 1, "exportgraphics")?.text()?;
    let mut dpi = 150.0;
    let mut i = 2;
    while i + 1 < a.len() {
        if a[i].text()?.eq_ignore_ascii_case("Resolution") {
            dpi = a[i + 1].scalar()?;
        }
        i += 2;
    }
    let format = match ext_of(&path).as_str() {
        "jpeg" => "jpg".to_string(),
        "tiff" => "tif".to_string(),
        "" => return rt("File name must include an extension (png, jpg, tif, gif or svg)."),
        e => e.to_string(),
    };
    export(it, f, &path, &format, dpi)?;
    none()
}

/// text(x, y, txt), text(x, y, z, txt), with vectors of positions and a cell of strings.
fn text(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let x = arg(a, 0, "text")?.to_matrix()?;
    let y = arg(a, 1, "text")?.to_matrix()?;
    let mut at = 2;
    if a.get(2).is_some_and(|v| !is_textlike(v) && !matches!(v, Value::Cell(_))) {
        at = 3;
    }
    let strings = texts(arg(a, at, "text")?)?;
    let (_, props) = split_props(a, at + 1)?;
    let n = x.numel().max(1);
    let mut series = Vec::new();
    for k in 0..n {
        let label = if x.numel() > 1 && strings.len() == x.numel() { strings[k].clone() } else { strings.join("\n") };
        let mut s = Series::new(SeriesKind::Text, vec![x.data[k]], vec![y.data[k.min(y.numel() - 1)]]);
        s.label = label;
        s.line_style = LineStyle::None;
        series.push(s);
    }
    let ids = it.add_decoration(series);
    apply_props(it, &ids, &props)?;
    handles_out(it, &ids, nargout)
}

/// xline(v, LineSpec, label, Name, Value) / yline(...): one line per value.
fn constant_line(it: &mut Interpreter, a: &[Value], horizontal: bool, nargout: usize) -> Ret {
    let values = arg(a, 0, if horizontal { "yline" } else { "xline" })?.to_matrix()?;
    let mut i = 1;
    let mut spec = None;
    if let Some(v) = a.get(1).filter(|v| v.is_text()) {
        if let Some(ls) = graphics::parse_line_spec(&v.text()?) {
            spec = Some(ls);
            i = 2;
        }
    }
    let mut labels = Vec::new();
    if let Some(v) = a.get(i) {
        // A label is a text argument not followed by a value (or a cell/string array).
        if matches!(v, Value::Cell(_)) || (is_textlike(v) && (a.len() - i) % 2 == 1) {
            labels = texts(v)?;
            i += 1;
        }
    }
    let (_, props) = split_props(a, i)?;
    let mut series = Vec::new();
    for (k, &v) in values.data.iter().enumerate() {
        let mut s = Series::new(SeriesKind::ConstantLine, vec![v], vec![v]);
        s.horizontal = horizontal;
        if let Some((c, st, _)) = spec {
            s.color = c.or(s.color);
            if let Some(st) = st {
                s.line_style = st;
            }
        }
        s.label = labels.get(k).or(labels.first()).cloned().unwrap_or_default();
        series.push(s);
    }
    let ids = it.add_decoration(series);
    apply_props(it, &ids, &props)?;
    handles_out(it, &ids, nargout)
}

fn region(it: &mut Interpreter, a: &[Value], horizontal: bool, nargout: usize) -> Ret {
    let name = if horizontal { "yregion" } else { "xregion" };
    let lo = arg(a, 0, name)?.to_matrix()?;
    let (hi, rest) = match a.get(1) {
        Some(v) if !is_textlike(v) => (v.to_matrix()?, 2),
        _ => {
            // xregion([x1 x2; x3 x4]) or xregion([x1 x2]).
            let m = lo.clone();
            let r = if m.cols() == 2 { m.rows() } else { 1 };
            let pairs: Vec<(f64, f64)> = (0..r).map(|k| if m.cols() == 2 { (m.get(k, 0), m.get(k, 1)) } else { (m.data[0], m.data[1]) }).collect();
            let (_, props) = split_props(a, 1)?;
            return finish_regions(it, pairs, horizontal, &props, nargout);
        }
    };
    let pairs = lo.data.iter().zip(&hi.data).map(|(&x, &y)| (x, y)).collect();
    let (_, props) = split_props(a, rest)?;
    finish_regions(it, pairs, horizontal, &props, nargout)
}

fn finish_regions(it: &mut Interpreter, pairs: Vec<(f64, f64)>, horizontal: bool, props: &[(String, Value)], nargout: usize) -> Ret {
    let series = pairs
        .into_iter()
        .map(|(x0, x1)| {
            let mut s = Series::new(SeriesKind::Region, vec![x0, x1], vec![x0, x1]);
            s.horizontal = horizontal;
            s.face_alpha = 0.3;
            s
        })
        .collect();
    let ids = it.add_decoration(series);
    apply_props(it, &ids, props)?;
    handles_out(it, &ids, nargout)
}

/// fill(x, y, c, ...) and patch(x, y, c, ...): one polygon per column of matrix x and y.
fn fill(it: &mut Interpreter, a: &[Value], name: &str, nargout: usize) -> Ret {
    let mut series = Vec::new();
    let mut i = 0;
    let mut props = Vec::new();
    // patch('XData', x, 'YData', y, ...).
    if a.first().is_some_and(is_textlike) {
        let (_, p) = split_props(a, 0)?;
        let get = |n: &str| p.iter().find(|(k, _)| k.eq_ignore_ascii_case(n)).map(|(_, v)| v.clone());
        let x = get("XData").map(|v| v.to_matrix()).transpose()?.unwrap_or_else(|| Matrix::new(0, 0, Vec::new()));
        let y = get("YData").map(|v| v.to_matrix()).transpose()?.unwrap_or_else(|| Matrix::new(0, 0, Vec::new()));
        let mut s = Series::new(SeriesKind::Patch, x.data, y.data);
        s.line_style = LineStyle::Solid;
        s.edge_color = Some([0, 0, 0]);
        series.push(s);
        props = p.into_iter().filter(|(k, _)| !k.eq_ignore_ascii_case("XData") && !k.eq_ignore_ascii_case("YData")).collect();
    }
    let positional = series.is_empty();
    while positional && i + 1 < a.len() && !is_textlike(&a[i]) && !is_textlike(&a[i + 1]) {
        let x = a[i].to_matrix()?;
        let y = a[i + 1].to_matrix()?;
        // The color: a color name or spec, or numeric color data.
        let c = a.get(i + 2);
        let color = c.and_then(fill_color);
        let consumed_color = c.is_some_and(|c| color.is_some() || !is_textlike(c));
        let (rows, cols) = if x.rows() == 1 || x.cols() == 1 { (x.numel(), 1) } else { (x.rows(), x.cols()) };
        for k in 0..cols {
            let xs = x.data[k * rows..(k + 1) * rows].to_vec();
            let ys: Vec<f64> = if y.numel() == x.numel() { y.data[k * rows..(k + 1) * rows].to_vec() } else { y.data.clone() };
            let mut s = Series::new(SeriesKind::Patch, xs, ys);
            s.line_style = LineStyle::Solid;
            s.edge_color = Some([0, 0, 0]);
            s.color = color;
            series.push(s);
        }
        i += if consumed_color { 3 } else { 2 };
    }
    if props.is_empty() {
        props = split_props(a, i)?.1;
    }
    let ids = if name == "patch" { it.add_decoration(series) } else { it.add_series(series, XAxisKind::Numeric) };
    apply_props(it, &ids, &props)?;
    handles_out(it, &ids, nargout)
}

/// errorbar(y, err), errorbar(x, y, err), errorbar(x, y, neg, pos), with a line spec.
fn errorbar(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let (head, props) = split_props(a, 0)?;
    let nums: Vec<Matrix> = head.iter().map(|v| v.to_matrix()).collect::<R<_>>()?;
    let spec = a.get(head.len()).filter(|v| v.is_text()).and_then(|v| graphics::parse_line_spec(&v.text().ok()?));
    let props: Vec<(String, Value)> = if spec.is_some() { split_props(a, head.len() + 1)?.1 } else { props };
    let (x, y, neg, pos) = match nums.len() {
        2 => {
            let y = nums[0].data.clone();
            ((1..=y.len()).map(|k| k as f64).collect(), y, nums[1].data.clone(), nums[1].data.clone())
        }
        3 => (nums[0].data.clone(), nums[1].data.clone(), nums[2].data.clone(), nums[2].data.clone()),
        4 => (nums[0].data.clone(), nums[1].data.clone(), nums[2].data.clone(), nums[3].data.clone()),
        6 => (nums[0].data.clone(), nums[1].data.clone(), nums[2].data.clone(), nums[3].data.clone()),
        _ => return rt("Invalid number of input arguments to errorbar."),
    };
    let fit = |e: Vec<f64>| if e.len() == 1 { vec![e[0]; y.len()] } else { e };
    let (neg, pos) = (fit(neg), fit(pos));
    if x.len() != y.len() || neg.len() != y.len() || pos.len() != y.len() {
        return rt("X, Y and error data must be the same size.");
    }
    let mut s = Series::new(SeriesKind::ErrorBar, x, y);
    s.err = Some((neg, pos));
    if let Some((c, st, mk)) = spec {
        s.color = c;
        if let Some(st) = st {
            s.line_style = st;
        } else if mk.is_some() {
            s.line_style = LineStyle::None;
        }
        s.marker = mk;
    }
    let ids = it.add_series(vec![s], XAxisKind::Numeric);
    apply_props(it, &ids, &props)?;
    handles_out(it, &ids, nargout)
}

fn ticks_fn(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    let axis = &name[..1].to_uppercase();
    let what = &name[1..];
    let prop = match what {
        "ticks" => format!("{axis}Tick"),
        "ticklabels" => format!("{axis}TickLabel"),
        "tickangle" => format!("{axis}TickLabelRotation"),
        "scale" => format!("{axis}Scale"),
        _ => format!("{axis}Lim"),
    };
    match a.first() {
        None => one(axes_prop(it, &prop, None)?),
        Some(v) if is_textlike(v) && matches!(v.text()?.as_str(), "auto" | "manual" | "mode") && what != "scale" => {
            let t = v.text()?;
            let mode_prop = format!("{prop}Mode");
            if t == "mode" {
                return one(axes_prop(it, &mode_prop, None)?);
            }
            axes_prop(it, &mode_prop, Some(Value::chars(&t)))?;
            none()
        }
        Some(v) => {
            axes_prop(it, &prop, Some(v.clone()))?;
            if nargout > 0 { one(axes_prop(it, &prop, None)?) } else { none() }
        }
    }
}

fn tickformat(it: &mut Interpreter, name: &str, a: &[Value]) -> Ret {
    let ruler = format!("{}Axis", name[..1].to_uppercase());
    let h = it.current_axes_handle();
    let r = gfx::member(it, &h, &ruler)?;
    match a.first() {
        None => one(gfx::member(it, &r, "TickLabelFormat")?),
        Some(v) => {
            gfx::set_member(it, &r, "TickLabelFormat", Value::chars(&v.text()?))?;
            none()
        }
    }
}

pub fn call(it: &mut Interpreter, name: &str, args: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    let a = if matches!(name, "saveas" | "print" | "exportgraphics" | "fontsize") { args } else { crate::builtins_plot::select_axes(it, args)? };
    match name {
        "xticks" | "yticks" | "zticks" | "xticklabels" | "yticklabels" | "zticklabels" | "xtickangle" | "ytickangle" | "ztickangle" | "xscale" | "yscale" | "zscale" | "zlim" => {
            if name.starts_with('z') && name != "zlim" {
                return none();
            }
            ticks_fn(it, name, a, nargout)
        }
        "xtickformat" | "ytickformat" | "ztickformat" => {
            if name == "ztickformat" {
                return none();
            }
            tickformat(it, name, a)
        }
        "semilogx" | "semilogy" | "loglog" => {
            let r = crate::builtins_plot::call(it, "plot", a, nargout)?;
            let ax = it.current_figure_index();
            let axes = it.figures[ax].axes_mut();
            axes.xlog = name != "semilogy";
            axes.ylog = name != "semilogx";
            Ok(r)
        }
        "text" => text(it, a, nargout),
        "xline" | "yline" => constant_line(it, a, name == "yline", nargout),
        "xregion" | "yregion" => region(it, a, name == "yregion", nargout),
        "fill" | "patch" => fill(it, a, name, nargout),
        "errorbar" => errorbar(it, a, nargout),
        "barh" => {
            let (mut series, x_kind, props) = plot_series(a, SeriesKind::Bar)?;
            for s in series.iter_mut() {
                s.kind = SeriesKind::BarH;
            }
            let ids = it.add_series(series, x_kind);
            apply_props(it, &ids, &props)?;
            handles_out(it, &ids, nargout)
        }
        "line" => {
            // Low-level line: added to the axes without replacing what is there.
            let (head, props) = split_props(a, 0)?;
            let (series, _, _) = plot_series(head, SeriesKind::Line)?;
            let ids = it.add_decoration(series);
            apply_props(it, &ids, &props)?;
            handles_out(it, &ids, nargout)
        }
        "saveas" => saveas(it, a),
        "print" => print(it, a),
        "exportgraphics" => exportgraphics(it, a),
        "fontsize" => {
            // fontsize(size, units), fontsize(obj, size, units), fontsize('increase'|'decrease'|'default').
            let (target, rest) = match a.first() {
                Some(v) if gfx::is_gfx(v) => (Some(v.clone()), &a[1..]),
                _ => (None, a),
            };
            let f = figure_of(it, target.as_ref())?;
            let first = arg(rest, 0, name)?;
            for ax in it.figures[f].axes.iter_mut() {
                let cur = ax.font_size.unwrap_or(10.0);
                ax.font_size = match first.text().ok().as_deref() {
                    Some("increase") => Some(cur * 1.1),
                    Some("decrease") => Some(cur / 1.1),
                    Some("default") => None,
                    _ => Some(first.scalar()? as f32),
                };
            }
            let id = it.figures[f].id;
            it.mark_figure_dirty(id);
            none()
        }
        "colorbar" => {
            let off = a.first().is_some_and(|v| v.text().map(|t| t == "off" || t == "delete").unwrap_or(false));
            let f = it.current_figure_index();
            it.figures[f].axes_mut().colorbar = !off;
            let id = it.figures[f].id;
            it.mark_figure_dirty(id);
            if nargout > 0 {
                let h = it.current_axes_handle();
                return one(h);
            }
            none()
        }
        _ => Ok(None),
    }
}
