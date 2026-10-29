//! Graphics functions: figures, axes properties and plot types.

use crate::builtins::{arg, none, one, Ret};
use crate::error::{rt, LangError};
use crate::graphics::{self, Axes, Figure, LineStyle, Series, SeriesKind, XAxisKind};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "figure", "close", "clf", "subplot", "hold", "grid", "title", "xlabel", "ylabel", "legend", "axis", "xlim", "ylim", "plot", "scatter", "bar",
    "stairs", "area", "histogram", "stem", "gcf", "gca", "ishold", "box", "subtitle", "sgtitle",
];

impl Interpreter {
    fn current_figure(&mut self) -> &mut Figure {
        if self.current_figure.is_none() {
            let id = self.next_figure_id;
            self.next_figure_id += 1;
            self.figures.push(Figure::new(id));
            self.current_figure = Some(self.figures.len() - 1);
        }
        let i = self.current_figure.unwrap();
        let id = self.figures[i].id;
        self.mark_figure_dirty(id);
        &mut self.figures[i]
    }

    fn add_series(&mut self, series: Vec<Series>, x_kind: XAxisKind) {
        let ax = self.current_figure().axes_mut();
        if !ax.hold {
            let grid = ax.grid;
            *ax = Axes { grid, ..Axes::default() };
        }
        if ax.series.is_empty() {
            ax.x_kind = x_kind;
        }
        ax.series.extend(series);
    }
}

fn text_of(v: &Value) -> R<String> {
    match v {
        Value::Cell(c) => Ok(c.data.iter().map(|x| x.text()).collect::<R<Vec<_>>>()?.join("\n")),
        Value::Str(s) if !s.is_scalar() => Ok(s.data.iter().map(|x| x.clone().unwrap_or_default()).collect::<Vec<_>>().join("\n")),
        Value::Sym(s) => Ok(s.disp()),
        other => other.text(),
    }
}

/// Numeric x data; datetimes and durations plot by their ms value with a time axis.
fn x_numeric(v: &Value) -> R<(Matrix, XAxisKind)> {
    match v {
        Value::Time(t) => {
            let kind = match t.kind {
                crate::time::TimeKind::DateTime => XAxisKind::DateTime,
                _ => XAxisKind::Duration,
            };
            let offset = t.tz.unwrap_or(0.0) * 60_000.0;
            let mut m = crate::time::to_numeric(t);
            if kind == XAxisKind::DateTime {
                m.data.iter_mut().for_each(|x| *x += offset);
            }
            Ok((m, kind))
        }
        Value::Sym(s) => Ok((s.to_matrix()?, XAxisKind::Numeric)),
        Value::Num(m) if m.kind == Kind::Char => rt("Data must be numeric, datetime, duration or an array convertible to double."),
        other => Ok((other.to_matrix()?, XAxisKind::Numeric)),
    }
}

fn y_numeric(v: &Value) -> R<Matrix> {
    match v {
        Value::Time(t) => Ok(crate::time::to_numeric(t)),
        Value::Sym(s) => s.to_matrix(),
        other => {
            let m = other.to_matrix()?;
            Ok(Matrix::from_dims(m.dims.clone(), m.data.clone()))
        }
    }
}

fn plot_series(args: &[Value], kind: SeriesKind) -> R<(Vec<Series>, XAxisKind)> {
    let mut out: Vec<Series> = Vec::new();
    let mut x_kind = XAxisKind::Numeric;
    let mut i = 0;
    let mut props: Vec<(String, Value)> = Vec::new();
    while i < args.len() {
        if args[i].is_text() {
            let name = args[i].text()?;
            if graphics::parse_line_spec(&name).is_some() && !out.is_empty() && props.is_empty() {
                i += 1;
                continue;
            }
            match args.get(i + 1) {
                Some(v) => props.push((name.to_lowercase(), v.clone())),
                None if name == "filled" => {}
                None => return rt(format!("Invalid parameter/value pair arguments ('{name}').")),
            }
            i += 2;
            continue;
        }
        let (x, y) = match args.get(i + 1) {
            Some(b) if !b.is_text() => {
                let (xm, k) = x_numeric(&args[i])?;
                x_kind = k;
                i += 2;
                (Some(xm), y_numeric(b)?)
            }
            _ => {
                let y = y_numeric(&args[i])?;
                i += 1;
                (None, y)
            }
        };
        let mut spec = None;
        if let Some(s) = args.get(i) {
            if s.is_text() {
                if let Some(ls) = graphics::parse_line_spec(&s.text()?) {
                    spec = Some(ls);
                    i += 1;
                }
            }
        }
        // Columns of a matrix Y are separate series (rows if Y's rows don't match X).
        let (yr, yc) = (y.rows(), y.cols());
        let mut ys: Vec<Vec<f64>> = Vec::new();
        if yr == 1 || yc == 1 {
            ys.push(y.data.clone());
        } else {
            let use_rows = matches!(&x, Some(xm) if xm.is_vector() && xm.numel() == yc && xm.numel() != yr);
            if use_rows {
                for r in 0..yr {
                    ys.push((0..yc).map(|c| y.get(r, c)).collect());
                }
            } else {
                for c in 0..yc {
                    ys.push(y.data[c * yr..(c + 1) * yr].to_vec());
                }
            }
        }
        for yv in ys {
            let xv = match &x {
                Some(xm) => xm.data.clone(),
                None => (1..=yv.len()).map(|k| k as f64).collect(),
            };
            if xv.len() != yv.len() {
                return rt("Vectors must be the same length.");
            }
            let mut s = Series::new(kind, xv, yv);
            if let Some((c, st, mk)) = spec {
                if c.is_some() {
                    s.color = c;
                }
                if let Some(st) = st {
                    s.line_style = st;
                } else if mk.is_some() && kind == SeriesKind::Line {
                    s.line_style = LineStyle::None;
                }
                if mk.is_some() {
                    s.marker = mk;
                }
            }
            out.push(s);
        }
    }
    for (name, v) in props {
        for s in out.iter_mut() {
            match name.as_str() {
                "linewidth" => s.line_width = v.scalar()? as f32,
                "markersize" => s.marker_size = v.scalar()? as f32,
                "displayname" => s.name = Some(v.text()?),
                "color" | "markerfacecolor" | "facecolor" | "markeredgecolor" => {
                    if v.is_text() {
                        s.color = graphics::named_color(&v.text()?).or(s.color);
                    } else {
                        let m = v.to_matrix()?;
                        if m.numel() >= 3 {
                            let k = if m.data.iter().take(3).all(|&c| c <= 1.0) { 255.0 } else { 1.0 };
                            s.color = Some([(m.data[0] * k) as u8, (m.data[1] * k) as u8, (m.data[2] * k) as u8]);
                        }
                    }
                }
                "linestyle" => {
                    let t = v.text()?;
                    if t == "none" {
                        s.line_style = LineStyle::None;
                    } else if let Some((_, Some(st), _)) = graphics::parse_line_spec(&t) {
                        s.line_style = st;
                    }
                }
                "marker" => {
                    let t = v.text()?;
                    s.marker = if t == "none" { None } else { graphics::parse_line_spec(&t).and_then(|p| p.2) };
                }
                _ => {}
            }
        }
    }
    Ok((out, x_kind))
}

pub fn call(it: &mut Interpreter, name: &str, args: &[Value], nargout: usize) -> Ret {
    let a = args;
    match name {
        "figure" => {
            let id = match a.first() {
                Some(v) if !v.is_text() => Some(v.scalar()? as u32),
                _ => None,
            };
            let idx = match id.and_then(|id| it.figures.iter().position(|f| f.id == id)) {
                Some(i) => i,
                None => {
                    let id = id.unwrap_or(it.next_figure_id);
                    it.next_figure_id = it.next_figure_id.max(id + 1);
                    it.figures.push(Figure::new(id));
                    it.figures.len() - 1
                }
            };
            it.current_figure = Some(idx);
            let id = it.figures[idx].id;
            if nargout >= 1 {
                return one(Value::num(id as f64));
            }
            none()
        }
        "gcf" => {
            let id = it.current_figure().id;
            one(Value::num(id as f64))
        }
        "gca" | "box" => none(),
        "close" => {
            let all = a.iter().any(|v| v.text().map(|s| s == "all").unwrap_or(false));
            if all {
                it.close_all_figures();
            } else if let Some(i) = it.current_figure {
                it.figures.remove(i);
                it.current_figure = if it.figures.is_empty() { None } else { Some(it.figures.len() - 1) };
            }
            none()
        }
        "clf" => {
            let f = it.current_figure();
            *f = Figure::new(f.id);
            none()
        }
        "subplot" => {
            let (m, n, p) = if a.len() == 1 {
                let v = a[0].scalar()? as usize;
                (v / 100, (v / 10) % 10, v % 10)
            } else {
                (arg(a, 0, name)?.as_usize()?, arg(a, 1, name)?.as_usize()?, arg(a, 2, name)?.as_usize()?)
            };
            if m == 0 || n == 0 || p == 0 || p > m * n {
                return rt("Index exceeds number of subplots.");
            }
            let f = it.current_figure();
            if f.grid != (m, n) {
                f.grid = (m, n);
                f.axes = vec![Axes::default(); m * n];
            }
            f.current_axes = p - 1;
            none()
        }
        "hold" | "grid" => {
            let mode = a.first().map(|v| v.text()).transpose()?;
            let ax = it.current_figure().axes_mut();
            let flag = if name == "hold" { &mut ax.hold } else { &mut ax.grid };
            *flag = match mode.as_deref() {
                Some("on") | Some("all") | Some("minor") => true,
                Some("off") => false,
                _ => !*flag,
            };
            none()
        }
        "ishold" => {
            let h = it.current_figure().axes_mut().hold;
            one(Value::bool(h))
        }
        "title" | "xlabel" | "ylabel" | "subtitle" | "sgtitle" => {
            let text = match a.first() {
                Some(v) => text_of(v)?,
                None => String::new(),
            };
            let fig = it.current_figure();
            if name == "sgtitle" {
                fig.name = text;
                return none();
            }
            let ax = fig.axes_mut();
            match name {
                "title" => ax.title = text,
                "subtitle" => ax.title = format!("{}\n{text}", ax.title),
                "xlabel" => ax.xlabel = text,
                _ => ax.ylabel = text,
            }
            none()
        }
        "legend" => {
            let mut names: Vec<String> = Vec::new();
            let mut show = true;
            for v in a {
                match v {
                    Value::Cell(_) | Value::Str(_) if !v.is_text() => names.extend(text_of(v)?.split('\n').map(String::from)),
                    other => {
                        let t = other.text()?;
                        match t.as_str() {
                            "off" => show = false,
                            "show" | "boxoff" | "boxon" => {}
                            "Location" | "location" | "Orientation" | "NumColumns" => break,
                            _ => names.push(t),
                        }
                    }
                }
            }
            let ax = it.current_figure().axes_mut();
            if !show {
                ax.legend = None;
            } else if names.is_empty() {
                ax.legend = Some(ax.series.iter().enumerate().map(|(i, s)| s.name.clone().unwrap_or_else(|| format!("data{}", i + 1))).collect());
            } else {
                ax.legend = Some(names);
            }
            none()
        }
        "axis" | "xlim" | "ylim" => {
            let Some(v) = a.first() else { return none() };
            if v.is_text() {
                let ax = it.current_figure().axes_mut();
                if matches!(v.text()?.as_str(), "auto" | "normal") {
                    ax.xlim = None;
                    ax.ylim = None;
                }
                return none();
            }
            let m = match v {
                Value::Time(_) => x_numeric(v)?.0,
                other => other.to_matrix()?,
            };
            let ax = it.current_figure().axes_mut();
            match name {
                "xlim" if m.numel() >= 2 => ax.xlim = Some((m.data[0], m.data[1])),
                "ylim" if m.numel() >= 2 => ax.ylim = Some((m.data[0], m.data[1])),
                "axis" if m.numel() >= 4 => {
                    ax.xlim = Some((m.data[0], m.data[1]));
                    ax.ylim = Some((m.data[2], m.data[3]));
                }
                _ => {}
            }
            none()
        }
        "plot" | "scatter" | "bar" | "stairs" | "area" | "stem" => {
            let kind = match name {
                "plot" => SeriesKind::Line,
                "scatter" => SeriesKind::Scatter,
                "bar" => SeriesKind::Bar,
                "stairs" => SeriesKind::Stairs,
                "stem" => SeriesKind::Stem,
                _ => SeriesKind::Area,
            };
            // scatter(x, y, sz, c): numeric size/color arguments after x, y.
            let mut args2: Vec<Value> = a.to_vec();
            let mut size = None;
            if kind == SeriesKind::Scatter {
                if args2.len() >= 3 && !args2[2].is_text() {
                    let s = args2.remove(2);
                    if s.numel() >= 1 {
                        size = Some(s.scalar()?);
                    }
                }
                if args2.len() >= 3 && !args2[2].is_text() {
                    args2.remove(2);
                }
                args2.retain(|v| v.text().map(|t| t != "filled").unwrap_or(true));
            }
            let (mut series, x_kind) = plot_series(&args2, kind)?;
            if let Some(sz) = size {
                for s in series.iter_mut() {
                    s.marker_size = (sz.sqrt() as f32).max(2.0);
                }
            }
            it.add_series(series, x_kind);
            none()
        }
        "histogram" => {
            let data = arg(a, 0, name)?.to_matrix()?;
            let finite: Vec<f64> = data.data.iter().copied().filter(|x| x.is_finite()).collect();
            if finite.is_empty() {
                return none();
            }
            let nbins = match a.get(1) {
                Some(v) if !v.is_text() && v.numel() == 1 => v.as_usize()?.max(1),
                _ => ((finite.len() as f64).sqrt().ceil() as usize).clamp(1, 50),
            };
            let lo = finite.iter().cloned().fold(f64::INFINITY, f64::min);
            let hi = finite.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let w = if hi > lo { (hi - lo) / nbins as f64 } else { 1.0 };
            let mut counts = vec![0.0; nbins];
            for x in finite {
                let k = (((x - lo) / w) as usize).min(nbins - 1);
                counts[k] += 1.0;
            }
            let centers: Vec<f64> = (0..nbins).map(|k| lo + w * (k as f64 + 0.5)).collect();
            let mut s = Series::new(SeriesKind::Bar, centers, counts);
            s.bar_width = Some(w);
            it.add_series(vec![s], XAxisKind::Numeric);
            none()
        }
        _ => Ok(None),
    }
}
