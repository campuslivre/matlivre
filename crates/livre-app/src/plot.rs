//! Renders recorded figures with the ModernChart look (rounded card, quiet grid,
//! crosshair with a value label on hover).

use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Rounding, Sense, Shape, Stroke, Ui};
use livre_lang::display::fmt_g;
use livre_lang::{Axes, Figure, LineStyle, Marker, SeriesKind, XAxisKind};

use crate::theme::{Skin, RADIUS};

pub fn figure(ui: &mut Ui, skin: &Skin, fig: &Figure, max_width: f32) -> egui::Response {
    let w = max_width.clamp(240.0, 640.0);
    let h = w * 0.75;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let p = ui.painter_at(rect.expand(1.0));
    p.rect_filled(rect, Rounding::same(RADIUS), skin.chart_bg);
    p.rect_stroke(rect.shrink(0.5), Rounding::same(RADIUS), Stroke::new(1.0_f32, skin.border));
    let (gr, gc) = fig.grid;
    let cell_w = rect.width() / gc.max(1) as f32;
    let cell_h = rect.height() / gr.max(1) as f32;
    let hover = resp.hover_pos();
    for (i, ax) in fig.axes.iter().enumerate() {
        let (r, c) = (i / gc.max(1), i % gc.max(1));
        let cell = Rect::from_min_size(pos2(rect.left() + c as f32 * cell_w, rect.top() + r as f32 * cell_h), vec2(cell_w, cell_h));
        draw_axes(&p, skin, ax, cell, hover);
    }
    resp
}

fn nice_step(range: f64, target: f64) -> f64 {
    let raw = (range / target).max(1e-300);
    let mag = 10f64.powf(raw.log10().floor());
    let f = raw / mag;
    let n = if f < 1.5 { 1.0 } else if f < 3.0 { 2.0 } else if f < 7.0 { 5.0 } else { 10.0 };
    n * mag
}

/// Tick-aligned limits and tick values.
fn nice_axis(lo: f64, hi: f64, target: f64) -> (f64, f64, Vec<f64>) {
    let step = nice_step(hi - lo, target);
    let a = (lo / step).floor() * step;
    let b = (hi / step).ceil() * step;
    let n = ((b - a) / step).round() as usize;
    let ticks = (0..=n.min(40)).map(|k| a + k as f64 * step).collect();
    (a, b, ticks)
}

/// Ticks for datetime/duration axes (values in ms) at calendar-friendly steps.
fn time_ticks(lo: f64, hi: f64) -> Vec<f64> {
    const S: f64 = 1000.0;
    const M: f64 = 60.0 * S;
    const H: f64 = 60.0 * M;
    const D: f64 = 24.0 * H;
    let steps = [S, 5.0 * S, 15.0 * S, 30.0 * S, M, 5.0 * M, 15.0 * M, 30.0 * M, H, 3.0 * H, 6.0 * H, 12.0 * H, D, 2.0 * D, 7.0 * D, 14.0 * D, 30.0 * D, 91.0 * D, 365.0 * D];
    let step = steps.iter().copied().find(|s| (hi - lo) / s <= 6.0).unwrap_or_else(|| nice_step(hi - lo, 6.0));
    ticks_within(lo, hi, step)
}

fn x_label(kind: XAxisKind, v: f64, step: f64, range: f64) -> String {
    match kind {
        XAxisKind::Numeric => tick_label(v, step),
        XAxisKind::DateTime => {
            let pattern = if step >= 86_400_000.0 { if range > 400.0 * 86_400_000.0 { "MMM yyyy" } else { "MMM dd" } } else { "HH:mm" };
            livre_lang::time::format_datetime(v, pattern)
        }
        XAxisKind::Duration => livre_lang::time::format_duration(v, "hh:mm"),
    }
}

fn tick_label(v: f64, step: f64) -> String {
    if v.abs() < step * 1e-9 {
        return "0".into();
    }
    fmt_g(v, 6)
}

fn bounds(ax: &Axes) -> ((f64, f64), (f64, f64)) {
    let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for s in &ax.series {
        for (&x, &y) in s.x.iter().zip(&s.y) {
            if x.is_finite() {
                x0 = x0.min(x);
                x1 = x1.max(x);
            }
            if y.is_finite() {
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
        if matches!(s.kind, SeriesKind::Bar | SeriesKind::Area | SeriesKind::Stem) {
            y0 = y0.min(0.0);
            y1 = y1.max(0.0);
        }
        if s.kind == SeriesKind::Bar && s.x.len() > 0 {
            let dx = bar_spacing(&s.x);
            x0 -= dx * 0.6;
            x1 += dx * 0.6;
        }
    }
    if !x0.is_finite() {
        (x0, x1) = (0.0, 1.0);
    }
    if !y0.is_finite() {
        (y0, y1) = (0.0, 1.0);
    }
    if x0 == x1 {
        x0 -= 1.0;
        x1 += 1.0;
    }
    if y0 == y1 {
        let d = if y0 == 0.0 { 1.0 } else { y0.abs() * 0.1 };
        y0 -= d;
        y1 += d;
    }
    (ax.xlim.unwrap_or((x0, x1)), ax.ylim.unwrap_or((y0, y1)))
}

fn bar_spacing(x: &[f64]) -> f64 {
    let mut d = f64::INFINITY;
    for w in x.windows(2) {
        let dx = (w[1] - w[0]).abs();
        if dx > 0.0 {
            d = d.min(dx);
        }
    }
    if d.is_finite() { d } else { 1.0 }
}

fn draw_axes(p: &egui::Painter, skin: &Skin, ax: &Axes, cell: Rect, hover: Option<Pos2>) {
    let small = FontId::proportional(11.5);
    let label_font = FontId::proportional(12.5);
    let mut inner = cell.shrink2(vec2(10.0, 8.0));
    if !ax.title.is_empty() {
        p.text(pos2(inner.center().x, inner.top() + 2.0), Align2::CENTER_TOP, &ax.title, FontId::new(14.0, egui::FontFamily::Name("semibold".into())), skin.text);
        inner.min.y += 24.0;
    }
    if !ax.xlabel.is_empty() {
        p.text(pos2(inner.center().x, inner.bottom()), Align2::CENTER_BOTTOM, &ax.xlabel, label_font.clone(), skin.chart_axis_text);
        inner.max.y -= 18.0;
    }
    if !ax.ylabel.is_empty() {
        let galley = p.layout_no_wrap(ax.ylabel.clone(), label_font.clone(), skin.chart_axis_text);
        let pos = pos2(inner.left(), inner.center().y + galley.size().x / 2.0);
        p.add(egui::epaint::TextShape::new(pos, galley, skin.chart_axis_text).with_angle(-std::f32::consts::FRAC_PI_2));
        inner.min.x += 18.0;
    }
    let ((dx0, dx1), (dy0, dy1)) = bounds(ax);
    let (mut x0, mut x1, mut xticks) = if ax.x_kind != XAxisKind::Numeric {
        (dx0, dx1, time_ticks(dx0, dx1))
    } else if ax.xlim.is_some() {
        let s = nice_step(dx1 - dx0, 6.0);
        (dx0, dx1, ticks_within(dx0, dx1, s))
    } else {
        nice_axis(dx0, dx1, 6.0)
    };
    let (mut y0, mut y1, mut yticks) = if ax.ylim.is_some() { let s = nice_step(dy1 - dy0, 5.0); (dy0, dy1, ticks_within(dy0, dy1, s)) } else { nice_axis(dy0, dy1, 5.0) };
    let ystep = if yticks.len() > 1 { yticks[1] - yticks[0] } else { 1.0 };
    let ylabel_w = yticks.iter().map(|&v| p.layout_no_wrap(tick_label(v, ystep), small.clone(), skin.text).size().x).fold(0.0, f32::max);
    let mut plot = Rect::from_min_max(pos2(inner.left() + ylabel_w + 8.0, inner.top() + 4.0), pos2(inner.right() - 6.0, inner.bottom() - 18.0));
    if plot.width() < 20.0 || plot.height() < 20.0 {
        return;
    }
    // A manual DataAspectRatio: one x unit spans as many pixels as aspect[0]/aspect[1] y units.
    // As in MATLAB, an automatic limit widens to fill the box; with both limits fixed, or when
    // filling would mean narrowing an automatic limit, the box shrinks around its centre.
    if let (Some([ax_x, ax_y, _]), XAxisKind::Numeric) = (ax.data_aspect, ax.x_kind) {
        let target = (plot.width() as f64 * ax_x) / (plot.height() as f64 * ax_y);
        let (xs, ys) = (x1 - x0, y1 - y0);
        if xs > 0.0 && ys > 0.0 {
            let current = xs / ys;
            if current < target && ax.xlim.is_none() {
                let half = ys * target / 2.0;
                let c = (x0 + x1) / 2.0;
                (x0, x1) = (c - half, c + half);
                xticks = ticks_within(x0, x1, nice_step(x1 - x0, 6.0));
            } else if current > target && ax.ylim.is_none() {
                let half = xs / target / 2.0;
                let c = (y0 + y1) / 2.0;
                (y0, y1) = (c - half, c + half);
                yticks = ticks_within(y0, y1, nice_step(y1 - y0, 5.0));
            } else {
                // Narrower when the data is relatively taller than the box, else shorter.
                let size = if current < target { vec2(plot.width() * (current / target) as f32, plot.height()) } else { vec2(plot.width(), plot.height() * (target / current) as f32) };
                plot = Rect::from_center_size(plot.center(), size);
            }
        }
    }
    let xstep = if xticks.len() > 1 { xticks[1] - xticks[0] } else { 1.0 };
    let ystep = if yticks.len() > 1 { yticks[1] - yticks[0] } else { 1.0 };
    let tx = |x: f64| plot.left() + ((x - x0) / (x1 - x0)) as f32 * plot.width();
    let ty = |y: f64| plot.bottom() - ((y - y0) / (y1 - y0)) as f32 * plot.height();

    // Grid, axes box and ticks.
    for &v in &xticks {
        let x = tx(v);
        if ax.grid {
            p.line_segment([pos2(x, plot.top()), pos2(x, plot.bottom())], Stroke::new(1.0_f32, skin.chart_grid));
        }
        p.line_segment([pos2(x, plot.bottom()), pos2(x, plot.bottom() - 4.0)], Stroke::new(1.0_f32, skin.chart_axis));
        p.text(pos2(x, plot.bottom() + 3.0), Align2::CENTER_TOP, x_label(ax.x_kind, v, xstep, x1 - x0), small.clone(), skin.chart_axis_text);
    }
    for &v in &yticks {
        let y = ty(v);
        if ax.grid {
            p.line_segment([pos2(plot.left(), y), pos2(plot.right(), y)], Stroke::new(1.0_f32, skin.chart_grid));
        }
        p.line_segment([pos2(plot.left(), y), pos2(plot.left() + 4.0, y)], Stroke::new(1.0_f32, skin.chart_axis));
        p.text(pos2(plot.left() - 5.0, y), Align2::RIGHT_CENTER, tick_label(v, ystep), small.clone(), skin.chart_axis_text);
    }
    p.rect_stroke(plot, Rounding::ZERO, Stroke::new(1.0_f32, skin.chart_axis));

    let clip = p.with_clip_rect(plot.expand(1.0));
    for (k, s) in ax.series.iter().enumerate() {
        let color = s.color.map(|c| Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(skin.palette[k % skin.palette.len()]);
        let stroke = Stroke::new(s.line_width.max(0.5), color);
        let pts: Vec<Option<Pos2>> = s.x.iter().zip(&s.y).map(|(&x, &y)| if x.is_finite() && y.is_finite() { Some(pos2(tx(x), ty(y))) } else { None }).collect();
        match s.kind {
            SeriesKind::Stem => {
                let base = ty(0.0f64.clamp(y0, y1));
                for pt in pts.iter().flatten() {
                    clip.line_segment([pos2(pt.x, base), *pt], stroke);
                }
            }
            SeriesKind::Bar => {
                let w = s.bar_width.unwrap_or(bar_spacing(&s.x) * 0.8) as f32 * plot.width() / (x1 - x0) as f32;
                let base = ty(0.0f64.clamp(y0, y1));
                for pt in pts.iter().flatten() {
                    let r = Rect::from_min_max(pos2(pt.x - w / 2.0, pt.y.min(base)), pos2(pt.x + w / 2.0, pt.y.max(base)));
                    clip.rect_filled(r, Rounding::same(2.0), color);
                }
            }
            SeriesKind::Area => {
                let base = ty(0.0f64.clamp(y0, y1));
                let fill = color.gamma_multiply(0.45);
                for seg in pts.windows(2) {
                    if let (Some(a), Some(b)) = (seg[0], seg[1]) {
                        clip.add(Shape::convex_polygon(vec![a, b, pos2(b.x, base), pos2(a.x, base)], fill, Stroke::NONE));
                    }
                }
                draw_polyline(&clip, &pts, stroke, LineStyle::Solid);
            }
            SeriesKind::Stairs => {
                let mut stepped = Vec::new();
                for seg in pts.windows(2) {
                    match (seg[0], seg[1]) {
                        (Some(a), Some(b)) => {
                            stepped.push(Some(a));
                            stepped.push(Some(pos2(b.x, a.y)));
                        }
                        _ => stepped.push(None),
                    }
                }
                if let Some(Some(last)) = pts.last() {
                    stepped.push(Some(*last));
                }
                draw_polyline(&clip, &stepped, stroke, s.line_style);
            }
            SeriesKind::Line | SeriesKind::Scatter => {
                if s.line_style != LineStyle::None {
                    draw_polyline(&clip, &pts, stroke, s.line_style);
                }
            }
        }
        if let Some(m) = s.marker {
            let filled = s.kind == SeriesKind::Scatter;
            for pt in pts.iter().flatten() {
                draw_marker(&clip, m, *pt, s.marker_size / 2.0 + 1.0, color, filled);
            }
        }
    }

    if let Some(names) = &ax.legend {
        draw_legend(p, skin, ax, names, plot);
    }

    if let Some(h) = hover {
        if plot.contains(h) {
            let cs = Stroke::new(1.0_f32, if skin.dark { Color32::from_rgb(120, 120, 120) } else { Color32::from_rgb(140, 140, 140) });
            clip.line_segment([pos2(h.x, plot.top()), pos2(h.x, plot.bottom())], cs);
            clip.line_segment([pos2(plot.left(), h.y), pos2(plot.right(), h.y)], cs);
            let xv = x0 + ((h.x - plot.left()) / plot.width()) as f64 * (x1 - x0);
            let yv = y0 + ((plot.bottom() - h.y) / plot.height()) as f64 * (y1 - y0);
            let xs = match ax.x_kind {
                XAxisKind::Numeric => fmt_g(xv, 4),
                XAxisKind::DateTime => livre_lang::time::format_datetime(xv, "dd-MMM-yyyy HH:mm"),
                XAxisKind::Duration => livre_lang::time::format_duration(xv, "hh:mm:ss"),
            };
            let text = format!("x {xs}   y {}", fmt_g(yv, 4));
            let galley = p.layout_no_wrap(text, small.clone(), Color32::WHITE);
            let mut r = Rect::from_min_size(h + vec2(10.0, -26.0), galley.size() + vec2(12.0, 6.0));
            if r.right() > plot.right() {
                r = r.translate(vec2(-r.width() - 20.0, 0.0));
            }
            if r.top() < plot.top() {
                r = r.translate(vec2(0.0, 36.0));
            }
            p.rect_filled(r, Rounding::same(RADIUS), Color32::from_rgb(70, 70, 70));
            p.galley(r.min + vec2(6.0, 3.0), galley, Color32::WHITE);
        }
    }
}

fn ticks_within(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    let a = (lo / step).ceil() * step;
    let n = ((hi - a) / step + 1e-9).floor().max(0.0) as usize;
    (0..=n.min(40)).map(|k| a + k as f64 * step).collect()
}

fn draw_polyline(p: &egui::Painter, pts: &[Option<Pos2>], stroke: Stroke, style: LineStyle) {
    let mut run: Vec<Pos2> = Vec::new();
    let flush = |run: &mut Vec<Pos2>| {
        if run.len() >= 2 {
            match style {
                LineStyle::Solid | LineStyle::None => {
                    p.add(Shape::line(std::mem::take(run), stroke));
                }
                LineStyle::Dashed => {
                    p.extend(Shape::dashed_line(run, stroke, 7.0, 4.0));
                }
                LineStyle::DashDot => {
                    p.extend(Shape::dashed_line(run, stroke, 9.0, 3.0));
                }
                LineStyle::Dotted => {
                    p.extend(Shape::dotted_line(run, stroke.color, 4.0, stroke.width * 0.6));
                }
            }
        }
        run.clear();
    };
    for pt in pts {
        match pt {
            Some(q) => run.push(*q),
            None => flush(&mut run),
        }
    }
    flush(&mut run);
}

fn draw_marker(p: &egui::Painter, m: Marker, c: Pos2, r: f32, color: Color32, filled: bool) {
    let s = Stroke::new(1.3_f32, color);
    let fill = if filled { color } else { Color32::TRANSPARENT };
    match m {
        Marker::Circle => {
            p.circle(c, r, fill, s);
        }
        Marker::Point => {
            p.circle_filled(c, 2.0, color);
        }
        Marker::Square => {
            p.rect(Rect::from_center_size(c, vec2(r * 1.8, r * 1.8)), Rounding::ZERO, fill, s);
        }
        Marker::Diamond => {
            p.add(Shape::convex_polygon(vec![c + vec2(0.0, -r * 1.2), c + vec2(r * 1.2, 0.0), c + vec2(0.0, r * 1.2), c + vec2(-r * 1.2, 0.0)], fill, s));
        }
        Marker::TriangleUp => {
            p.add(Shape::convex_polygon(vec![c + vec2(0.0, -r * 1.2), c + vec2(r * 1.1, r * 0.8), c + vec2(-r * 1.1, r * 0.8)], fill, s));
        }
        Marker::TriangleDown => {
            p.add(Shape::convex_polygon(vec![c + vec2(0.0, r * 1.2), c + vec2(-r * 1.1, -r * 0.8), c + vec2(r * 1.1, -r * 0.8)], fill, s));
        }
        Marker::Plus => {
            p.line_segment([c + vec2(-r, 0.0), c + vec2(r, 0.0)], s);
            p.line_segment([c + vec2(0.0, -r), c + vec2(0.0, r)], s);
        }
        Marker::Cross => {
            p.line_segment([c + vec2(-r, -r), c + vec2(r, r)], s);
            p.line_segment([c + vec2(r, -r), c + vec2(-r, r)], s);
        }
        Marker::Star => {
            p.line_segment([c + vec2(-r, 0.0), c + vec2(r, 0.0)], s);
            p.line_segment([c + vec2(0.0, -r), c + vec2(0.0, r)], s);
            p.line_segment([c + vec2(-r * 0.7, -r * 0.7), c + vec2(r * 0.7, r * 0.7)], s);
            p.line_segment([c + vec2(r * 0.7, -r * 0.7), c + vec2(-r * 0.7, r * 0.7)], s);
        }
    }
}

fn draw_legend(p: &egui::Painter, skin: &Skin, ax: &Axes, names: &[String], plot: Rect) {
    let font = FontId::proportional(12.0);
    let rows: Vec<(String, Color32, LineStyle, Option<Marker>, SeriesKind)> = ax
        .series
        .iter()
        .enumerate()
        .zip(names)
        .map(|((k, s), n)| {
            let c = s.color.map(|c| Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(skin.palette[k % skin.palette.len()]);
            (n.clone(), c, s.line_style, s.marker, s.kind)
        })
        .collect();
    if rows.is_empty() {
        return;
    }
    let text_w = rows.iter().map(|r| p.layout_no_wrap(r.0.clone(), font.clone(), skin.text).size().x).fold(0.0, f32::max);
    let size = vec2(text_w + 46.0, rows.len() as f32 * 18.0 + 8.0);
    let r = Rect::from_min_size(pos2(plot.right() - size.x - 8.0, plot.top() + 8.0), size);
    p.rect_filled(r, Rounding::same(RADIUS), skin.chart_bg);
    p.rect_stroke(r.shrink(0.5), Rounding::same(RADIUS), Stroke::new(1.0_f32, skin.border));
    for (i, (name, c, style, marker, kind)) in rows.iter().enumerate() {
        let y = r.top() + 4.0 + i as f32 * 18.0 + 9.0;
        let a = pos2(r.left() + 8.0, y);
        let b = pos2(r.left() + 30.0, y);
        if *kind == SeriesKind::Bar {
            p.rect_filled(Rect::from_min_max(pos2(a.x + 4.0, y - 5.0), pos2(b.x - 4.0, y + 5.0)), Rounding::same(2.0), *c);
        } else if *style != LineStyle::None {
            draw_polyline(p, &[Some(a), Some(b)], Stroke::new(1.5_f32, *c), *style);
        }
        if let Some(m) = marker {
            draw_marker(p, *m, pos2((a.x + b.x) / 2.0, y), 3.5, *c, *kind == SeriesKind::Scatter);
        }
        p.text(pos2(b.x + 8.0, y), Align2::LEFT_CENTER, name, font.clone(), skin.text);
    }
}
