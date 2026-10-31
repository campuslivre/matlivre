//! Renders recorded figures with the ModernChart look (rounded card, quiet grid, dashed
//! crosshair with a value badge on hover). One drawing path feeds two backends: an egui
//! `Painter` on screen and an SVG writer for export (SVG, PNG through resvg, clipboard).

use std::fmt::Write as _;
use std::sync::{Arc, OnceLock};

use eframe::egui::{self, emath::Rot2, epaint::TextShape, pos2, vec2, Align, Color32, FontFamily, FontId, Galley, Pos2, Rect, Rounding, Sense, Shape, Stroke, Ui, Vec2};
use livre_lang::display::fmt_g;
use livre_lang::graphics::{self, nice_step};
use livre_lang::{Axes, Figure, LineStyle, Marker, Series, SeriesKind, Ticks, XAxisKind};
use resvg::{tiny_skia, usvg};

use crate::theme::{Skin, RADIUS};

/// Default figure size; narrower figures are this layout scaled down.
const DESIGN: Vec2 = Vec2 { x: 560.0, y: 300.0 };

#[derive(Clone, Copy)]
struct Font {
    size: f32,
    bold: bool,
}

const TITLE: Font = Font { size: 14.0, bold: true };
const LABEL: Font = Font { size: 12.0, bold: false };
const TICK: Font = Font { size: 11.0, bold: false };

impl Font {
    fn id(self, scale: f32) -> FontId {
        FontId::new(self.size * scale, if self.bold { FontFamily::Name("semibold".into()) } else { FontFamily::Proportional })
    }

    /// Baseline offset that vertically centres digits on a given y.
    fn mid(self) -> f32 {
        self.size * 0.35
    }
}

struct Tokens {
    fg: Color32,
    text: Color32,
    bg: Color32,
    border: Color32,
    grid: Color32,
    axis: Color32,
    cross: Color32,
    badge: Color32,
    series: [Color32; 8],
}

impl Tokens {
    fn new(skin: &Skin) -> Self {
        let mut series = skin.palette;
        [series[0], series[1]] = if skin.dark { [Color32::from_rgb(0x4C, 0xC2, 0xFF), Color32::from_rgb(0xFF, 0xB7, 0x4D)] } else { [Color32::from_rgb(0x00, 0x5F, 0xB8), Color32::from_rgb(0x9A, 0x6A, 0x12)] };
        Self {
            fg: skin.text,
            text: skin.chart_axis_text,
            bg: skin.chart_bg,
            border: skin.border,
            grid: skin.chart_grid,
            axis: skin.chart_axis,
            cross: Color32::from_gray(if skin.dark { 0x78 } else { 0x8C }),
            badge: Color32::from_gray(0x46),
            series,
        }
    }

    fn color(&self, s: &Series, k: usize) -> Color32 {
        s.color.map(|c| Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(self.series[k % self.series.len()])
    }
}

fn baseline(g: &Galley) -> f32 {
    g.rows.first().and_then(|r| r.glyphs.first()).map_or(g.size().y * 0.8, |gl| gl.pos.y)
}

/// Drawing surface in figure points; both backends map them to their own space.
trait Canvas {
    fn ctx(&self) -> &egui::Context;
    /// A polyline, dashed when `dash` holds alternating dash/gap lengths.
    fn line(&mut self, pts: &[Pos2], stroke: Stroke, dash: &[f32]);
    fn rect(&mut self, r: Rect, radius: f32, fill: Color32, stroke: Stroke);
    fn circle(&mut self, c: Pos2, r: f32, fill: Color32, stroke: Stroke);
    fn polygon(&mut self, pts: &[Pos2], fill: Color32, stroke: Stroke);
    /// Text whose baseline starts, centres or ends (`align`) at `pos`, rotated about `pos`.
    fn text(&mut self, pos: Pos2, align: Align, text: &str, font: Font, color: Color32, angle: f32);
    /// An RGBA image (`w` × `h` pixels, rows top to bottom) stretched over `r`.
    fn image(&mut self, r: Rect, w: usize, h: usize, rgba: &[u8]);
    fn clip(&mut self, r: Option<Rect>);

    fn text_width(&self, text: &str, font: Font) -> f32 {
        self.ctx().fonts(|f| f.layout_no_wrap(text.to_owned(), font.id(1.0), Color32::WHITE)).size().x
    }

    fn dots(&mut self, pts: &[Pos2], spacing: f32, r: f32, color: Color32) {
        let mut at = 0.0;
        for w in pts.windows(2) {
            let d = w[1] - w[0];
            let len = d.length();
            while at < len {
                self.circle(w[0] + d * (at / len), r, color, Stroke::NONE);
                at += spacing;
            }
            at -= len;
        }
    }
}

struct PainterCanvas {
    p: egui::Painter,
    o: Vec2,
    s: f32,
    full: Rect,
}

impl PainterCanvas {
    fn pt(&self, p: Pos2) -> Pos2 {
        p * self.s + self.o
    }

    fn stroke(&self, s: Stroke) -> Stroke {
        Stroke::new(s.width * self.s, s.color)
    }
}

impl Canvas for PainterCanvas {
    fn ctx(&self) -> &egui::Context {
        self.p.ctx()
    }

    fn line(&mut self, pts: &[Pos2], stroke: Stroke, dash: &[f32]) {
        if pts.len() < 2 {
            return;
        }
        let pts: Vec<Pos2> = pts.iter().map(|&q| self.pt(q)).collect();
        let stroke = self.stroke(stroke);
        if dash.is_empty() {
            self.p.add(Shape::line(pts, stroke));
        } else {
            let d: Vec<f32> = dash.iter().step_by(2).map(|v| v * self.s).collect();
            let g: Vec<f32> = dash.iter().skip(1).step_by(2).map(|v| v * self.s).collect();
            self.p.extend(Shape::dashed_line_with_offset(&pts, stroke, &d, &g, 0.0));
        }
    }

    fn rect(&mut self, r: Rect, radius: f32, fill: Color32, stroke: Stroke) {
        self.p.rect(Rect::from_min_max(self.pt(r.min), self.pt(r.max)), Rounding::same(radius * self.s), fill, self.stroke(stroke));
    }

    fn circle(&mut self, c: Pos2, r: f32, fill: Color32, stroke: Stroke) {
        self.p.circle(self.pt(c), r * self.s, fill, self.stroke(stroke));
    }

    fn polygon(&mut self, pts: &[Pos2], fill: Color32, stroke: Stroke) {
        self.p.add(Shape::convex_polygon(pts.iter().map(|&q| self.pt(q)).collect(), fill, self.stroke(stroke)));
    }

    fn text(&mut self, pos: Pos2, align: Align, text: &str, font: Font, color: Color32, angle: f32) {
        if text.is_empty() {
            return;
        }
        let g = self.p.layout_no_wrap(text.to_owned(), font.id(self.s), color);
        let anchor = self.pt(pos);
        let top_left = anchor - vec2(g.size().x * align.to_factor(), baseline(&g));
        let pos = anchor + Rot2::from_angle(angle) * (top_left - anchor);
        self.p.add(TextShape::new(pos, g, color).with_angle(angle));
    }

    fn clip(&mut self, r: Option<Rect>) {
        let r = r.map_or(self.full, |r| Rect::from_min_max(self.pt(r.min), self.pt(r.max)).intersect(self.full));
        self.p.set_clip_rect(r);
    }

    fn image(&mut self, r: Rect, w: usize, h: usize, rgba: &[u8]) {
        let tex = cached_texture(self.p.ctx(), w, h, rgba);
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        self.p.image(tex.id(), Rect::from_min_max(self.pt(r.min), self.pt(r.max)), uv, Color32::WHITE);
    }
}

struct SvgCanvas<'a> {
    ctx: &'a egui::Context,
    out: String,
    clips: usize,
    clipped: bool,
}

fn num(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

fn paint(attr: &str, c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    match a {
        0 => format!(r#" {attr}="none""#),
        255 => format!(r##" {attr}="#{r:02X}{g:02X}{b:02X}""##),
        _ => format!(r##" {attr}="#{r:02X}{g:02X}{b:02X}" {attr}-opacity="{}""##, num(a as f32 / 255.0)),
    }
}

fn stroke_attrs(s: Stroke) -> String {
    if s.width <= 0.0 || s.color.a() == 0 {
        return String::new();
    }
    format!(r#"{} stroke-width="{}""#, paint("stroke", s.color), num(s.width))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

impl Canvas for SvgCanvas<'_> {
    fn ctx(&self) -> &egui::Context {
        self.ctx
    }

    fn line(&mut self, pts: &[Pos2], stroke: Stroke, dash: &[f32]) {
        if pts.len() < 2 {
            return;
        }
        let points: Vec<String> = pts.iter().map(|p| format!("{},{}", num(p.x), num(p.y))).collect();
        let dash = if dash.is_empty() { String::new() } else { format!(r#" stroke-dasharray="{}""#, dash.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(" ")) };
        let _ = write!(self.out, r#"<polyline points="{}" fill="none"{} stroke-linejoin="round"{dash}/>"#, points.join(" "), stroke_attrs(stroke));
    }

    fn rect(&mut self, r: Rect, radius: f32, fill: Color32, stroke: Stroke) {
        let rx = if radius > 0.0 { format!(r#" rx="{}""#, num(radius)) } else { String::new() };
        let _ = write!(self.out, r#"<rect x="{}" y="{}" width="{}" height="{}"{rx}{}{}/>"#, num(r.left()), num(r.top()), num(r.width()), num(r.height()), paint("fill", fill), stroke_attrs(stroke));
    }

    fn circle(&mut self, c: Pos2, r: f32, fill: Color32, stroke: Stroke) {
        let _ = write!(self.out, r#"<circle cx="{}" cy="{}" r="{}"{}{}/>"#, num(c.x), num(c.y), num(r), paint("fill", fill), stroke_attrs(stroke));
    }

    fn polygon(&mut self, pts: &[Pos2], fill: Color32, stroke: Stroke) {
        let points: Vec<String> = pts.iter().map(|p| format!("{},{}", num(p.x), num(p.y))).collect();
        let _ = write!(self.out, r#"<polygon points="{}"{}{} stroke-linejoin="round"/>"#, points.join(" "), paint("fill", fill), stroke_attrs(stroke));
    }

    fn text(&mut self, pos: Pos2, align: Align, text: &str, font: Font, color: Color32, angle: f32) {
        if text.is_empty() {
            return;
        }
        let anchor = match align {
            Align::Min => "start",
            Align::Center => "middle",
            Align::Max => "end",
        };
        let weight = if font.bold { r#" font-weight="600""# } else { "" };
        let rotate = if angle != 0.0 { format!(r#" transform="rotate({} {} {})""#, num(angle.to_degrees()), num(pos.x), num(pos.y)) } else { String::new() };
        let _ = write!(self.out, r#"<text x="{}" y="{}" font-size="{}"{weight} text-anchor="{anchor}"{}{rotate}>{}</text>"#, num(pos.x), num(pos.y), font.size, paint("fill", color), escape(text));
    }

    fn clip(&mut self, r: Option<Rect>) {
        if std::mem::take(&mut self.clipped) {
            self.out.push_str("</g>");
        }
        if let Some(r) = r {
            self.clips += 1;
            let id = self.clips;
            let _ = write!(self.out, r#"<clipPath id="c{id}"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath><g clip-path="url(#c{id})">"#, num(r.left()), num(r.top()), num(r.width()), num(r.height()));
            self.clipped = true;
        }
    }

    fn image(&mut self, r: Rect, w: usize, h: usize, rgba: &[u8]) {
        let Some(mut pm) = tiny_skia::Pixmap::new(w as u32, h as u32) else { return };
        for (px, c) in pm.pixels_mut().iter_mut().zip(rgba.chunks(4)) {
            *px = tiny_skia::ColorU8::from_rgba(c[0], c[1], c[2], c[3]).premultiply();
        }
        let Ok(png) = pm.encode_png() else { return };
        let _ = write!(
            self.out,
            r#"<image x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="none" style="image-rendering:pixelated" href="data:image/png;base64,{}"/>"#,
            num(r.left()),
            num(r.top()),
            num(r.width()),
            num(r.height()),
            base64(&png)
        );
    }
}

/// The size `figure` allocates for a given available width.
pub fn figure_size(max_width: f32) -> egui::Vec2 {
    let w = max_width.clamp(120.0, DESIGN.x);
    vec2(w, w * DESIGN.y / DESIGN.x)
}

/// Layout size in figure points and the factor mapping it onto `size`.
fn layout_scale(size: Vec2) -> (Vec2, f32) {
    let s = (size.x / DESIGN.x).min(size.y / DESIGN.y).clamp(1e-3, 1.0);
    (size / s, s)
}

/// Draws the figure (with hover crosshair) and returns its response.
pub fn figure(ui: &mut Ui, skin: &Skin, fig: &Figure, max_width: f32) -> egui::Response {
    let size = figure_size(max_width);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let (layout, s) = layout_scale(size);
    // Stay inside the enclosing clip (the scrolled document card).
    let full = rect.expand(1.0).intersect(ui.clip_rect());
    let mut c = PainterCanvas { p: ui.painter_at(full), o: rect.min.to_vec2(), s, full };
    let hover = resp.hover_pos().map(|h| ((h - rect.min) / s).to_pos2());
    draw_figure(&mut c, skin, fig, layout, hover);
    resp
}

/// The figure as a standalone SVG document of `size` points.
pub fn to_svg(ctx: &egui::Context, skin: &Skin, fig: &Figure, size: egui::Vec2) -> String {
    svg_document(ctx, skin, fig, size, None)
}

fn svg_document(ctx: &egui::Context, skin: &Skin, fig: &Figure, size: Vec2, hover: Option<Pos2>) -> String {
    let (layout, s) = layout_scale(size);
    let mut c = SvgCanvas { ctx, out: String::new(), clips: 0, clipped: false };
    draw_figure(&mut c, skin, fig, layout, hover);
    c.clip(None);
    let (w, h) = (num(size.x), num(size.y));
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Segoe UI" xml:space="preserve"><g transform="scale({s})">{}</g></svg>"#,
        c.out
    )
}

fn fontdb() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        if let Ok(win) = std::env::var("WINDIR") {
            let dir = std::path::Path::new(&win).join("Fonts");
            for f in ["segoeui.ttf", "seguisb.ttf", "SegUIVar.ttf"] {
                let _ = db.load_font_file(dir.join(f));
            }
        }
        db.load_system_fonts();
        db.set_sans_serif_family("Segoe UI");
        Arc::new(db)
    })
    .clone()
}

fn rasterize(svg: &str, size: Vec2, scale: f32) -> Result<tiny_skia::Pixmap, String> {
    let opt = usvg::Options { fontdb: fontdb(), font_family: "Segoe UI".into(), ..Default::default() };
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    let (w, h) = ((size.x * scale).round() as u32, (size.y * scale).round() as u32);
    let mut pm = tiny_skia::Pixmap::new(w, h).ok_or("invalid figure size")?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pm.as_mut());
    Ok(pm)
}

/// The figure rasterised at `scale` × `size` pixels, as PNG bytes.
pub fn to_png(ctx: &egui::Context, skin: &Skin, fig: &Figure, size: egui::Vec2, scale: f32) -> Result<Vec<u8>, String> {
    rasterize(&to_svg(ctx, skin, fig, size), size, scale)?.encode_png().map_err(|e| e.to_string())
}

/// Puts the figure on the system clipboard as an image (rasterised at 2×).
pub fn copy_to_clipboard(ctx: &egui::Context, skin: &Skin, fig: &Figure, size: egui::Vec2) -> Result<(), String> {
    let pm = rasterize(&to_svg(ctx, skin, fig, size), size, 2.0)?;
    let bytes: Vec<u8> = pm
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    let image = arboard::ImageData { width: pm.width() as usize, height: pm.height() as usize, bytes: bytes.into() };
    arboard::Clipboard::new().and_then(|mut cb| cb.set_image(image)).map_err(|e| e.to_string())
}

/// Textures of recently drawn images, kept alive while they are on screen.
fn cached_texture(ctx: &egui::Context, w: usize, h: usize, rgba: &[u8]) -> egui::TextureHandle {
    use std::hash::{Hash, Hasher};
    thread_local! {
        static CACHE: std::cell::RefCell<Vec<(u64, egui::TextureHandle)>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (w, h, rgba).hash(&mut hasher);
    let key = hasher.finish();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((_, t)) = c.iter().find(|(k, _)| *k == key) {
            return t.clone();
        }
        let img = egui::ColorImage::from_rgba_unmultiplied([w, h], rgba);
        let t = ctx.load_texture("figure-image", img, egui::TextureOptions::NEAREST);
        c.push((key, t.clone()));
        if c.len() > 32 {
            drop(c.remove(0));
        }
        t
    })
}

fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Colormap and color limits of an axes.
struct ColorScale<'a> {
    map: &'a [[f64; 3]],
    lim: (f64, f64),
}

impl ColorScale<'_> {
    fn color(&self, v: f64) -> Option<Color32> {
        graphics::map_color(self.map, self.lim, v).map(rgb32)
    }

    /// A colormap row by 1-based index (direct image indexing).
    fn direct(&self, v: f64) -> Option<Color32> {
        if !v.is_finite() || self.map.is_empty() {
            return None;
        }
        Some(rgb32(self.map[(v.floor() as i64 - 1).clamp(0, self.map.len() as i64 - 1) as usize]))
    }
}

fn rgb32(c: [f64; 3]) -> Color32 {
    Color32::from_rgb((c[0].clamp(0.0, 1.0) * 255.0).round() as u8, (c[1].clamp(0.0, 1.0) * 255.0).round() as u8, (c[2].clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// NaN-separated runs of (x, y) points with the first value of `c` in each run.
fn runs<'a>(x: &'a [f64], y: &'a [f64], c: &'a [f64]) -> Vec<(Vec<usize>, f64)> {
    let mut out = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    for i in 0..x.len() {
        if x[i].is_nan() || y[i].is_nan() {
            if !cur.is_empty() {
                let v = c.get(cur[0]).copied().unwrap_or(f64::NAN);
                out.push((std::mem::take(&mut cur), v));
            }
        } else {
            cur.push(i);
        }
    }
    if !cur.is_empty() {
        let v = c.get(cur[0]).copied().unwrap_or(f64::NAN);
        out.push((cur, v));
    }
    out
}

/// The RGBA pixels of an image series (rows top to bottom as stored).
fn image_pixels(s: &Series, cs: &ColorScale) -> Vec<u8> {
    let (rows, cols) = s.grid;
    let n = rows * cols;
    let mut out = Vec::with_capacity(n * 4);
    for r in 0..rows {
        for j in 0..cols {
            let k = j * rows + r;
            let c = if s.channels == 3 {
                Some(rgb32([s.c[k], s.c[n + k], s.c[2 * n + k]]))
            } else if s.direct {
                cs.direct(s.c[k])
            } else {
                cs.color(s.c[k])
            };
            let c = c.unwrap_or(Color32::TRANSPARENT);
            out.extend([c.r(), c.g(), c.b(), c.a()]);
        }
    }
    out
}

/// The arrow of a quiver: the shaft and two head strokes.
fn arrow(c: &mut dyn Canvas, a: Pos2, b: Pos2, stroke: Stroke) {
    c.line(&[a, b], stroke, &[]);
    let d = b - a;
    let len = d.length();
    if len < 1e-3 {
        return;
    }
    let head = (len * 0.3).min(9.0);
    let dir = d / len;
    for side in [-1.0f32, 1.0] {
        let rot = Rot2::from_angle(side * 0.45);
        c.line(&[b, b - rot * dir * head], stroke, &[]);
    }
}

/// A vertical colorbar in `r` with tick labels on its right.
fn draw_colorbar(c: &mut dyn Canvas, t: &Tokens, cs: &ColorScale, r: Rect) {
    let n = cs.map.len().max(1);
    for (i, col) in cs.map.iter().enumerate() {
        let y0 = r.bottom() - (i as f32 + 1.0) * r.height() / n as f32;
        c.rect(Rect::from_min_max(pos2(r.left(), y0), pos2(r.right(), y0 + r.height() / n as f32 + 0.5)), 0.0, rgb32(*col), Stroke::NONE);
    }
    c.rect(r, 0.0, Color32::TRANSPARENT, Stroke::new(1.0_f32, t.axis));
    let (lo, hi) = cs.lim;
    let step = nice_step(hi - lo, 5.0);
    for v in ticks_within(lo, hi, step) {
        let y = r.bottom() - ((v - lo) / (hi - lo)) as f32 * r.height();
        c.line(&[pos2(r.right() - 3.0, y), pos2(r.right(), y)], Stroke::new(1.0_f32, t.axis), &[]);
        c.text(pos2(r.right() + 4.0, y + TICK.mid()), Align::Min, &tick_label(v, step), TICK, t.text, 0.0);
    }
}

/// An orthographic 3-D view: data scaled into the unit cube, turned to (azimuth, elevation)
/// as MATLAB's `view`, and fitted into the plot box.
struct View3 {
    lim: [(f64, f64); 3],
    sin_az: f64,
    cos_az: f64,
    sin_el: f64,
    cos_el: f64,
    scale: f32,
    centre: Pos2,
}

impl View3 {
    fn unit(&self, p: [f64; 3]) -> [f64; 3] {
        let mut q = [0.0; 3];
        for k in 0..3 {
            let (lo, hi) = self.lim[k];
            q[k] = (p[k] - lo) / (hi - lo) - 0.5;
        }
        q
    }

    /// Screen offset (x right, y up) and depth (larger is farther) of a unit-cube point.
    fn turn(&self, q: [f64; 3]) -> (f64, f64, f64) {
        let fwd = -q[0] * self.sin_az + q[1] * self.cos_az;
        let sx = q[0] * self.cos_az + q[1] * self.sin_az;
        let sy = q[2] * self.cos_el + fwd * self.sin_el;
        let depth = fwd * self.cos_el - q[2] * self.sin_el;
        (sx, sy, depth)
    }

    fn screen_unit(&self, q: [f64; 3]) -> Pos2 {
        let (sx, sy, _) = self.turn(q);
        pos2(self.centre.x + sx as f32 * self.scale, self.centre.y - sy as f32 * self.scale)
    }

    fn screen(&self, p: [f64; 3]) -> Option<Pos2> {
        p.iter().all(|v| v.is_finite()).then(|| self.screen_unit(self.unit(p)))
    }

    fn depth(&self, p: [f64; 3]) -> f64 {
        self.turn(self.unit(p)).2
    }
}

enum Prim {
    Poly(Vec<Pos2>, Color32, Stroke),
    Line([Pos2; 2], Stroke, LineStyle),
    Marker(Pos2, Marker, f32, Color32, bool),
}

fn draw_axes_3d(c: &mut dyn Canvas, t: &Tokens, ax: &Axes, cs: &ColorScale, cell: Rect, plot: Rect, m: &Frame) {
    let ((x0, x1), (y0, y1)) = ax.bounds();
    let (z0, z1) = ax.zbounds();
    let sx = Scale::new((x0, x1), ax.xlim.is_some(), false, 1.0, 4.0, false);
    let sy = Scale::new((y0, y1), ax.ylim.is_some(), false, 1.0, 4.0, false);
    let sz = Scale::new((z0, z1), ax.zlim.is_some(), false, 1.0, 4.0, false);
    let (az, el) = ax.view.unwrap_or((-37.5, 30.0));
    let (az, el) = (az.to_radians(), el.to_radians());
    let mut v = View3 { lim: [(sx.lo, sx.hi), (sy.lo, sy.hi), (sz.lo, sz.hi)], sin_az: az.sin(), cos_az: az.cos(), sin_el: el.sin(), cos_el: el.cos(), scale: 1.0, centre: Pos2::ZERO };
    // Fit the projected cube into the plot box, leaving room for tick labels.
    let corners: Vec<[f64; 3]> = (0..8).map(|k| [if k & 1 == 0 { -0.5 } else { 0.5 }, if k & 2 == 0 { -0.5 } else { 0.5 }, if k & 4 == 0 { -0.5 } else { 0.5 }]).collect();
    let proj: Vec<(f64, f64, f64)> = corners.iter().map(|&q| v.turn(q)).collect();
    let (minx, maxx) = proj.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| (a.0.min(p.0), a.1.max(p.0)));
    let (miny, maxy) = proj.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| (a.0.min(p.1), a.1.max(p.1)));
    let room = plot.shrink2(vec2(24.0, 14.0));
    v.scale = (room.width() / (maxx - minx) as f32).min(room.height() / (maxy - miny) as f32);
    v.centre = room.center() + vec2(-((minx + maxx) / 2.0) as f32 * v.scale, ((miny + maxy) / 2.0) as f32 * v.scale);

    c.text(pos2(plot.center().x, cell.top() + m.title_y), Align::Center, &ax.title, TITLE, t.fg, 0.0);
    let grid = Stroke::new(1.0_f32, t.grid);
    let axis = Stroke::new(1.0_f32, t.axis);
    let show_axes = ax.visible_axes.unwrap_or(true);
    // Back faces: for each axis, the side of the cube farther from the viewer.
    let back = |k: usize| -> f64 {
        let mut q = [0.0; 3];
        q[k] = 0.5;
        if v.turn(q).2 > 0.0 { 0.5 } else { -0.5 }
    };
    let (bx, by, bz) = (back(0), back(1), back(2));
    let lims = [(sx.lo, sx.hi), (sy.lo, sy.hi), (sz.lo, sz.hi)];
    let unit_of = |k: usize, val: f64| (val - lims[k].0) / (lims[k].1 - lims[k].0) - 0.5;
    if show_axes {
        // The three back planes, with grid lines at the ticks of the axes spanning them.
        let planes: [(usize, f64, usize, usize); 3] = [(0, bx, 1, 2), (1, by, 0, 2), (2, bz, 0, 1)];
        for (fixed, at, a, b) in planes {
            let pt = |ua: f64, ub: f64| {
                let mut q = [0.0; 3];
                q[fixed] = at;
                q[a] = ua;
                q[b] = ub;
                v.screen_unit(q)
            };
            if ax.grid {
                for axis_k in [a, b] {
                    let ticks = [&sx.ticks, &sy.ticks, &sz.ticks][axis_k];
                    for &tv in ticks.iter() {
                        let u = unit_of(axis_k, tv);
                        if !(-0.5001..=0.5001).contains(&u) {
                            continue;
                        }
                        let (p, q) = if axis_k == a { (pt(u, -0.5), pt(u, 0.5)) } else { (pt(-0.5, u), pt(0.5, u)) };
                        c.line(&[p, q], grid, &[]);
                    }
                }
            }
            let outline = [pt(-0.5, -0.5), pt(0.5, -0.5), pt(0.5, 0.5), pt(-0.5, 0.5), pt(-0.5, -0.5)];
            c.line(&outline, axis, &[]);
        }
        // Tick labels along the front edges of the floor (x, y) and a left vertical edge (z).
        let floor = -0.5;
        let label = |c: &mut dyn Canvas, q: [f64; 3], text: &str, away: Vec2| {
            let p = v.screen_unit(q) + away;
            let align = if away.x < -1.0 { Align::Max } else if away.x > 1.0 { Align::Min } else { Align::Center };
            c.text(p + vec2(0.0, TICK.mid()), align, text, TICK, t.text, 0.0);
        };
        let centre = v.screen_unit([0.0, 0.0, floor]);
        for (k, s) in [(0usize, &sx), (1usize, &sy)] {
            let other = if k == 0 { 1 } else { 0 };
            let front = if k == 0 { -by } else { -bx };
            let mut edge_mid = [0.0; 3];
            edge_mid[other] = front;
            edge_mid[2] = floor;
            let out = v.screen_unit(edge_mid) - centre;
            let away = if out.length() > 1e-3 { out.normalized() * 14.0 } else { vec2(0.0, 14.0) };
            for &tv in &s.ticks {
                let u = unit_of(k, tv);
                if !(-0.5001..=0.5001).contains(&u) {
                    continue;
                }
                let mut q = edge_mid;
                q[k] = u;
                label(c, q, &tick_label(tv, s.step()), away);
            }
            let name = if k == 0 { &ax.xlabel } else { &ax.ylabel };
            if !name.is_empty() {
                label(c, edge_mid, name, away * 2.6);
            }
        }
        // z: the vertical edge drawn farthest left.
        let mut best = (f32::INFINITY, [0.0, 0.0]);
        for (ex, ey) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
            let p = v.screen_unit([ex, ey, 0.0]);
            if p.x < best.0 {
                best = (p.x, [ex, ey]);
            }
        }
        for &tv in &sz.ticks {
            let u = unit_of(2, tv);
            if (-0.5001..=0.5001).contains(&u) {
                label(c, [best.1[0], best.1[1], u], &tick_label(tv, sz.step()), vec2(-8.0, 0.0));
            }
        }
        if !ax.zlabel.is_empty() {
            let p = v.screen_unit([best.1[0], best.1[1], 0.0]) + vec2(-34.0, 0.0);
            c.text(p, Align::Center, &ax.zlabel, LABEL, t.text, -std::f32::consts::FRAC_PI_2);
        }
    }

    // Data, far to near.
    let mut prims: Vec<(f64, Prim)> = Vec::new();
    for (k, s) in ax.series.iter().enumerate().filter(|(_, s)| s.visible) {
        let color = t.color(s, k);
        let zat = |i: usize| s.z.get(i).copied().unwrap_or(0.0);
        match s.kind {
            SeriesKind::Surface => {
                let (rows, cols) = s.grid;
                let idx = |i: usize, j: usize| j * rows + i;
                for i in 0..rows.saturating_sub(1) {
                    for j in 0..cols.saturating_sub(1) {
                        let q = [idx(i, j), idx(i, j + 1), idx(i + 1, j + 1), idx(i + 1, j)];
                        let pts3: Vec<[f64; 3]> = q.iter().map(|&n| [s.x[n], s.y[n], zat(n)]).collect();
                        let quad: Option<Vec<Pos2>> = pts3.iter().map(|&p| v.screen(p)).collect();
                        let Some(quad) = quad else { continue };
                        let depth = pts3.iter().map(|&p| v.depth(p)).sum::<f64>() / 4.0;
                        let value = s.c.get(q[0]).copied().unwrap_or(f64::NAN);
                        let fill = if s.face_none { Color32::TRANSPARENT } else if s.face_mapped { cs.color(value).unwrap_or(Color32::TRANSPARENT) } else { color };
                        let edge = if s.edge_mapped { cs.color(value).map(|e| Stroke::new(0.8_f32, e)).unwrap_or(Stroke::NONE) } else { s.edge_color.map(|e| Stroke::new(0.4_f32, Color32::from_rgb(e[0], e[1], e[2]))).unwrap_or(Stroke::NONE) };
                        // Faces filled with the edge color hide the seams between quads.
                        let fill_edge = if edge == Stroke::NONE && fill != Color32::TRANSPARENT { Stroke::new(0.6_f32, fill) } else { edge };
                        prims.push((depth, Prim::Poly(quad, fill, fill_edge)));
                    }
                }
            }
            SeriesKind::Quiver => {
                for i in 0..s.x.len() {
                    let a = [s.x[i], s.y[i], zat(i)];
                    let b = [a[0] + s.u.get(i).copied().unwrap_or(0.0), a[1] + s.v.get(i).copied().unwrap_or(0.0), a[2] + s.w.get(i).copied().unwrap_or(0.0)];
                    if let (Some(p), Some(q)) = (v.screen(a), v.screen(b)) {
                        prims.push((v.depth(a), Prim::Line([p, q], Stroke::new(1.2_f32, color), LineStyle::Solid)));
                        let d = q - p;
                        if d.length() > 1e-3 {
                            let head = (d.length() * 0.3).min(9.0);
                            for side in [-1.0f32, 1.0] {
                                let r = Rot2::from_angle(side * 0.45);
                                prims.push((v.depth(b), Prim::Line([q, q - r * d.normalized() * head], Stroke::new(1.2_f32, color), LineStyle::Solid)));
                            }
                        }
                    }
                }
            }
            SeriesKind::Stem => {
                for i in 0..s.x.len() {
                    let top = [s.x[i], s.y[i], zat(i)];
                    let foot = [s.x[i], s.y[i], 0.0f64.clamp(sz.lo, sz.hi)];
                    if let (Some(p), Some(q)) = (v.screen(foot), v.screen(top)) {
                        prims.push((v.depth(top), Prim::Line([p, q], Stroke::new(1.5_f32, color), LineStyle::Solid)));
                        prims.push((v.depth(top) - 1e-6, Prim::Marker(q, Marker::Circle, 4.0, color, false)));
                    }
                }
            }
            _ => {
                // Lines (plot3, contour3, contours on the floor) and markers.
                let line_color = |i: usize| -> Color32 {
                    if s.kind == SeriesKind::Contour && s.edge_mapped {
                        cs.color(s.c.get(i).copied().unwrap_or(f64::NAN)).unwrap_or(color)
                    } else {
                        color
                    }
                };
                let pts: Vec<[f64; 3]> = (0..s.x.len()).map(|i| [s.x[i], s.y.get(i).copied().unwrap_or(f64::NAN), zat(i)]).collect();
                if s.line_style != LineStyle::None {
                    for i in 1..pts.len() {
                        if let (Some(p), Some(q)) = (v.screen(pts[i - 1]), v.screen(pts[i])) {
                            let depth = (v.depth(pts[i - 1]) + v.depth(pts[i])) / 2.0;
                            prims.push((depth, Prim::Line([p, q], Stroke::new(s.line_width.max(1.5), line_color(i - 1)), s.line_style)));
                        }
                    }
                }
                if let Some(mk) = s.marker {
                    for &p3 in &pts {
                        if let Some(p) = v.screen(p3) {
                            prims.push((v.depth(p3) - 1e-6, Prim::Marker(p, mk, s.marker_size / 2.0 + 1.0, color, s.kind == SeriesKind::Scatter)));
                        }
                    }
                }
            }
        }
    }
    prims.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    c.clip(Some(plot.expand(20.0).intersect(cell)));
    for (_, p) in prims {
        match p {
            Prim::Poly(pts, fill, stroke) => c.polygon(&pts, fill, stroke),
            Prim::Line(seg, stroke, style) => draw_polyline(c, &[Some(seg[0]), Some(seg[1])], stroke, style),
            Prim::Marker(p, mk, r, color, filled) => draw_marker(c, mk, p, r, color, filled),
        }
    }
    c.clip(None);
    if let Some(names) = &ax.legend {
        draw_legend(c, t, ax, names, plot);
    }
}

fn draw_figure(c: &mut dyn Canvas, skin: &Skin, fig: &Figure, size: Vec2, hover: Option<Pos2>) {
    let t = Tokens::new(skin);
    c.rect(Rect::from_min_size(Pos2::ZERO, size).shrink(0.5), RADIUS, t.bg, Stroke::new(1.0_f32, t.border));
    let (gr, gc) = (fig.grid.0.max(1), fig.grid.1.max(1));
    let cell = vec2(size.x / gc as f32, size.y / gr as f32);
    for (i, ax) in fig.axes.iter().enumerate() {
        let (r, col) = (i / gc, i % gc);
        let rect = Rect::from_min_size(pos2(col as f32 * cell.x, r as f32 * cell.y), cell);
        draw_axes(c, &t, ax, fig.colormap.as_deref(), rect, gr * gc > 1, hover);
    }
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
fn time_ticks(lo: f64, hi: f64, target: f64) -> Vec<f64> {
    const S: f64 = 1000.0;
    const M: f64 = 60.0 * S;
    const H: f64 = 60.0 * M;
    const D: f64 = 24.0 * H;
    let steps = [S, 5.0 * S, 15.0 * S, 30.0 * S, M, 5.0 * M, 15.0 * M, 30.0 * M, H, 3.0 * H, 6.0 * H, 12.0 * H, D, 2.0 * D, 7.0 * D, 14.0 * D, 30.0 * D, 91.0 * D, 365.0 * D];
    let step = steps.iter().copied().find(|s| (hi - lo) / s <= target).unwrap_or_else(|| nice_step(hi - lo, target));
    ticks_within(lo, hi, step)
}

fn ticks_within(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    let a = (lo / step).ceil() * step;
    let n = ((hi - a) / step + 1e-9).floor().max(0.0) as usize;
    (0..=n.min(40)).map(|k| a + k as f64 * step).collect()
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

/// A crosshair readout value: three decimals unless very large or small.
fn readout(v: f64) -> String {
    if v == 0.0 || (1e-3..1e6).contains(&v.abs()) {
        format!("{v:.3}")
    } else {
        fmt_g(v, 4)
    }
}

/// The y a line-like series passes through at `x` (linear, or held for stairs).
fn value_at(s: &Series, x: f64) -> Option<f64> {
    let pts: Vec<(f64, f64)> = s.x.iter().copied().zip(s.y.iter().copied()).collect();
    for w in pts.windows(2) {
        let ((xa, ya), (xb, yb)) = (w[0], w[1]);
        if !(xa.is_finite() && xb.is_finite() && ya.is_finite() && yb.is_finite()) || x < xa.min(xb) || x > xa.max(xb) {
            continue;
        }
        if s.kind == SeriesKind::Stairs {
            return Some(if x >= xb && xb > xa { yb } else { ya });
        }
        return Some(if xb == xa { ya } else { ya + (yb - ya) * (x - xa) / (xb - xa) });
    }
    None
}

fn is_line_like(s: &Series) -> bool {
    match s.kind {
        SeriesKind::Line => s.line_style != LineStyle::None,
        SeriesKind::Area | SeriesKind::Stairs => true,
        _ => false,
    }
}

/// Margins and text positions of one axes box, relative to its cell.
struct Frame {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    title_y: f32,
    xlabel_dy: f32,
    ylabel_x: f32,
    tick_dy: f32,
}

impl Frame {
    fn new(ax: &Axes, multi: bool) -> Self {
        if multi {
            let top = if ax.title.is_empty() { 14.0 } else { 30.0 };
            let bottom = if ax.xlabel.is_empty() { 26.0 } else { 40.0 };
            Self { left: 44.0, right: 10.0, top, bottom, title_y: 20.0, xlabel_dy: 8.0, ylabel_x: 14.0, tick_dy: 15.0 }
        } else {
            Self { left: 52.0, right: 16.0, top: 36.0, bottom: 46.0, title_y: 22.0, xlabel_dy: 10.0, ylabel_x: 16.0, tick_dy: 16.0 }
        }
    }
}

/// One axis: its limits (in data units), whether it is logarithmic, and its tick values.
struct Scale {
    lo: f64,
    hi: f64,
    log: bool,
    ticks: Vec<f64>,
}

impl Scale {
    /// The data limits, rounded out to ticks unless set, on a linear or log axis.
    fn new(auto: (f64, f64), set: bool, log: bool, positive_min: f64, target: f64, time: bool) -> Self {
        if log {
            let lo = if auto.0 > 0.0 { auto.0 } else { positive_min.min(auto.1.max(1.0)) };
            let hi = auto.1.max(lo * 10.0);
            let (mut l0, mut l1) = (lo.log10(), hi.log10());
            if !set {
                (l0, l1) = (l0.floor(), l1.ceil());
                if l0 == l1 {
                    l1 += 1.0;
                }
            }
            let step = ((l1.ceil() - l0.floor()) / 8.0).ceil().max(1.0);
            let ticks = (l0.ceil() as i64..=l1.floor() as i64).filter(|k| (*k as f64 - l0.ceil()) % step == 0.0).map(|k| 10f64.powi(k as i32)).collect();
            return Self { lo: 10f64.powf(l0), hi: 10f64.powf(l1), log, ticks };
        }
        let (lo, hi, ticks) = if time {
            (auto.0, auto.1, time_ticks(auto.0, auto.1, target))
        } else if set {
            (auto.0, auto.1, ticks_within(auto.0, auto.1, nice_step(auto.1 - auto.0, target)))
        } else {
            nice_axis(auto.0, auto.1, target)
        };
        Self { lo, hi, log, ticks }
    }

    /// Position of `v` along the axis, 0 at `lo` and 1 at `hi`.
    fn frac(&self, v: f64) -> Option<f64> {
        if !v.is_finite() {
            return None;
        }
        if self.log {
            return (v > 0.0).then(|| (v.log10() - self.lo.log10()) / (self.hi.log10() - self.lo.log10()));
        }
        Some((v - self.lo) / (self.hi - self.lo))
    }

    fn step(&self) -> f64 {
        if self.ticks.len() > 1 { self.ticks[1] - self.ticks[0] } else { 1.0 }
    }
}

/// Smallest positive value plotted along x (`x_axis`) or y, for log axes.
fn positive_min(ax: &Axes, x_axis: bool) -> f64 {
    let mut m = f64::INFINITY;
    for s in &ax.series {
        let horizontal_bars = s.kind == SeriesKind::BarH;
        let vals = if x_axis != horizontal_bars { &s.x } else { &s.y };
        for &v in vals {
            if v > 0.0 && v < m {
                m = v;
            }
        }
    }
    if m.is_finite() { m } else { 1.0 }
}

fn superscript(k: i64) -> String {
    k.to_string().chars().map(|c| match c {
        '-' => '⁻',
        d => ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'][d.to_digit(10).unwrap_or(0) as usize],
    }).collect()
}

/// The label of tick `i` (value `v`): set labels, a tick format, powers of ten on log axes,
/// or the default number/time text.
fn tick_text(ticks: &Ticks, scale: &Scale, i: usize, v: f64, default: impl Fn(f64) -> String) -> String {
    if let Some(labels) = &ticks.labels {
        return if labels.is_empty() { String::new() } else { labels[i % labels.len()].clone() };
    }
    if let Some(f) = &ticks.format {
        return livre_lang::gfx::format_tick(Some(f), v);
    }
    if scale.log {
        return format!("10{}", superscript(v.log10().round() as i64));
    }
    default(v)
}

/// Kinds that take part in legends.
fn in_legend(s: &Series) -> bool {
    !matches!(s.kind, SeriesKind::Text | SeriesKind::ConstantLine | SeriesKind::Region)
}

fn draw_axes(c: &mut dyn Canvas, t: &Tokens, ax: &Axes, fig_map: Option<&[[f64; 3]]>, cell: Rect, multi: bool, hover: Option<Pos2>) {
    let m = Frame::new(ax, multi);
    let mut plot = Rect::from_min_max(cell.min + vec2(m.left, m.top), cell.max - vec2(m.right, m.bottom));
    let parula;
    let map: &[[f64; 3]] = match (ax.colormap.as_deref(), fig_map) {
        (Some(m), _) | (None, Some(m)) => m,
        _ => {
            parula = graphics::colormap("parula", 256).unwrap_or_default();
            &parula
        }
    };
    let cs = ColorScale { map, lim: ax.color_limits() };
    if ax.colorbar {
        plot.max.x -= 56.0;
        draw_colorbar(c, t, &cs, Rect::from_min_max(pos2(plot.right() + 14.0, plot.top()), pos2(plot.right() + 28.0, plot.bottom())));
    }
    if ax.is_3d() {
        draw_axes_3d(c, t, ax, &cs, cell, plot, &m);
        return;
    }
    let (xt, yt) = (((plot.width() / 82.0) as f64).clamp(2.0, 8.0), ((plot.height() / 44.0) as f64).clamp(2.0, 6.0));
    let ((dx0, dx1), (dy0, dy1)) = ax.bounds();
    let mut sx = Scale::new((dx0, dx1), ax.xlim.is_some(), ax.xlog, positive_min(ax, true), xt, ax.x_kind != XAxisKind::Numeric);
    let mut sy = Scale::new((dy0, dy1), ax.ylim.is_some(), ax.ylog, positive_min(ax, false), yt, false);
    let show_axes = ax.visible_axes.unwrap_or(true);
    let ylabel_of = |sy: &Scale, i: usize, v: f64| tick_text(&ax.yticks, sy, i, v, |v| tick_label(v, sy.step()));
    if let Some(v) = &ax.yticks.values {
        sy.ticks = v.clone();
    }
    let tick_w = if show_axes { sy.ticks.iter().enumerate().map(|(i, &v)| c.text_width(&ylabel_of(&sy, i, v), TICK)).fold(0.0, f32::max) } else { 0.0 };
    let need = tick_w + 8.0 + if ax.ylabel.is_empty() { 8.0 } else { m.ylabel_x + 8.0 };
    plot.min.x = plot.min.x.max(cell.left() + need);
    if ax.xticks.angle.abs() > 1.0 {
        plot.max.y -= (ax.xticks.angle.to_radians().sin().abs() * 30.0).min(40.0);
    }
    if plot.width() < 20.0 || plot.height() < 20.0 {
        return;
    }
    // A manual DataAspectRatio: one x unit spans as many pixels as aspect[0]/aspect[1] y units.
    // As in MATLAB, an automatic limit widens to fill the box; with both limits fixed, or when
    // filling would mean narrowing an automatic limit, the box shrinks around its centre.
    if let (Some([ax_x, ax_y, _]), XAxisKind::Numeric, false, false) = (ax.data_aspect, ax.x_kind, sx.log, sy.log) {
        let target = (plot.width() as f64 * ax_x) / (plot.height() as f64 * ax_y);
        let (xs, ys) = (sx.hi - sx.lo, sy.hi - sy.lo);
        if xs > 0.0 && ys > 0.0 {
            let current = xs / ys;
            if current < target && ax.xlim.is_none() {
                let half = ys * target / 2.0;
                let mid = (sx.lo + sx.hi) / 2.0;
                (sx.lo, sx.hi) = (mid - half, mid + half);
                sx.ticks = ticks_within(sx.lo, sx.hi, nice_step(sx.hi - sx.lo, xt));
            } else if current > target && ax.ylim.is_none() {
                let half = xs / target / 2.0;
                let mid = (sy.lo + sy.hi) / 2.0;
                (sy.lo, sy.hi) = (mid - half, mid + half);
                sy.ticks = ticks_within(sy.lo, sy.hi, nice_step(sy.hi - sy.lo, yt));
            } else {
                // Narrower when the data is relatively taller than the box, else shorter.
                let size = if current < target { vec2(plot.width() * (current / target) as f32, plot.height()) } else { vec2(plot.width(), plot.height() * (target / current) as f32) };
                plot = Rect::from_center_size(plot.center(), size);
            }
        }
    }
    if let Some(v) = &ax.xticks.values {
        sx.ticks = v.clone();
    }
    if let Some(v) = &ax.yticks.values {
        sy.ticks = v.clone();
    }
    let (x0, x1, y0, y1) = (sx.lo, sx.hi, sy.lo, sy.hi);
    let mx = |x: f64| sx.frac(x).map(|f| plot.left() + f as f32 * plot.width());
    let my = |y: f64| sy.frac(y).map(|f| if ax.y_reverse { plot.top() + f as f32 * plot.height() } else { plot.bottom() - f as f32 * plot.height() });
    let ty = |y: f64| my(y).unwrap_or(plot.bottom());
    let at = |x: f64, y: f64| match (mx(x), my(y)) {
        (Some(a), Some(b)) => Some(pos2(a, b)),
        _ => None,
    };

    c.text(pos2(plot.center().x, cell.top() + m.title_y), Align::Center, &ax.title, TITLE, t.fg, 0.0);
    c.text(pos2(plot.center().x, cell.bottom() - m.xlabel_dy), Align::Center, &ax.xlabel, LABEL, t.text, 0.0);
    c.text(pos2(cell.left() + m.ylabel_x, plot.center().y), Align::Center, &ax.ylabel, LABEL, t.text, -std::f32::consts::FRAC_PI_2);

    let grid = Stroke::new(1.0_f32, t.grid);
    let axis = Stroke::new(1.0_f32, t.axis);
    if show_axes {
        let xstep = sx.step();
        for (i, &v) in sx.ticks.iter().enumerate() {
            let Some(x) = mx(v).filter(|x| (plot.left() - 0.5..=plot.right() + 0.5).contains(x)) else { continue };
            if ax.grid {
                c.line(&[pos2(x, plot.top()), pos2(x, plot.bottom())], grid, &[]);
            }
            c.line(&[pos2(x, plot.bottom()), pos2(x, plot.bottom() - 4.0)], axis, &[]);
            let label = tick_text(&ax.xticks, &sx, i, v, |v| x_label(ax.x_kind, v, xstep, x1 - x0));
            if ax.xticks.angle.abs() > 1.0 {
                c.text(pos2(x, plot.bottom() + 10.0), Align::Max, &label, TICK, t.text, -ax.xticks.angle.to_radians());
            } else {
                c.text(pos2(x, plot.bottom() + m.tick_dy), Align::Center, &label, TICK, t.text, 0.0);
            }
        }
        for (i, &v) in sy.ticks.iter().enumerate() {
            let Some(y) = my(v).filter(|y| (plot.top() - 0.5..=plot.bottom() + 0.5).contains(y)) else { continue };
            if ax.grid {
                c.line(&[pos2(plot.left(), y), pos2(plot.right(), y)], grid, &[]);
            }
            c.line(&[pos2(plot.left(), y), pos2(plot.left() + 4.0, y)], axis, &[]);
            c.text(pos2(plot.left() - 8.0, y + TICK.mid()), Align::Max, &ylabel_of(&sy, i, v), TICK, t.text, -ax.yticks.angle.to_radians());
        }
        c.rect(plot, 0.0, Color32::TRANSPARENT, axis);
    }

    c.clip(Some(plot.expand(1.0)));
    let base = ty(0.0f64.clamp(y0, y1));
    let base_x = mx(0.0f64.clamp(x0, x1)).unwrap_or(plot.left());
    // Regions first, so data draws over them.
    for (k, s) in ax.series.iter().enumerate().filter(|(_, s)| s.visible && s.kind == SeriesKind::Region) {
        let fill = t.color(s, k).gamma_multiply(s.face_alpha.clamp(0.0, 1.0));
        let r = if s.horizontal {
            let (a, b) = (ty(s.y[0]), ty(*s.y.get(1).unwrap_or(&s.y[0])));
            Rect::from_min_max(pos2(plot.left(), a.min(b)), pos2(plot.right(), a.max(b)))
        } else {
            let (a, b) = (mx(s.x[0]).unwrap_or(plot.left()), mx(*s.x.get(1).unwrap_or(&s.x[0])).unwrap_or(plot.left()));
            Rect::from_min_max(pos2(a.min(b), plot.top()), pos2(a.max(b), plot.bottom()))
        };
        c.rect(r, 0.0, fill, Stroke::NONE);
    }
    for (k, s) in ax.series.iter().enumerate().filter(|(_, s)| s.visible) {
        let color = t.color(s, k);
        let stroke = Stroke::new(s.line_width.max(2.0), color);
        let pts: Vec<Option<Pos2>> = s.x.iter().zip(&s.y).map(|(&x, &y)| at(x, y)).collect();
        match s.kind {
            SeriesKind::Region => {}
            SeriesKind::Stem => {
                for pt in pts.iter().flatten() {
                    c.line(&[pos2(pt.x, base), *pt], stroke, &[]);
                }
            }
            SeriesKind::Bar => {
                let w = s.bar_width.unwrap_or(graphics::bar_spacing(&s.x) * 0.8) as f32 * plot.width() / (x1 - x0) as f32;
                for pt in pts.iter().flatten() {
                    c.rect(Rect::from_min_max(pos2(pt.x - w / 2.0, pt.y.min(base)), pos2(pt.x + w / 2.0, pt.y.max(base))), 2.0, color, Stroke::NONE);
                }
            }
            SeriesKind::BarH => {
                let h = s.bar_width.unwrap_or(graphics::bar_spacing(&s.x) * 0.8) as f32 * plot.height() / (y1 - y0) as f32;
                for (&p, &len) in s.x.iter().zip(&s.y) {
                    let (Some(yc), Some(xe)) = (my(p), mx(len)) else { continue };
                    c.rect(Rect::from_min_max(pos2(xe.min(base_x), yc - h / 2.0), pos2(xe.max(base_x), yc + h / 2.0)), 2.0, color, Stroke::NONE);
                }
            }
            SeriesKind::Area => {
                let fill = color.gamma_multiply(0.45);
                for seg in pts.windows(2) {
                    if let (Some(a), Some(b)) = (seg[0], seg[1]) {
                        if (a.y - base) * (b.y - base) < 0.0 {
                            let m = a + (b - a) * ((base - a.y) / (b.y - a.y));
                            c.polygon(&[a, m, pos2(a.x, base)], fill, Stroke::NONE);
                            c.polygon(&[m, b, pos2(b.x, base)], fill, Stroke::NONE);
                        } else {
                            c.polygon(&[a, b, pos2(b.x, base), pos2(a.x, base)], fill, Stroke::NONE);
                        }
                    }
                }
                draw_polyline(c, &pts, stroke, LineStyle::Solid);
            }
            SeriesKind::Stairs => {
                let mut stepped = Vec::new();
                for seg in pts.windows(2) {
                    match (seg[0], seg[1]) {
                        (Some(a), Some(b)) => stepped.extend([Some(a), Some(pos2(b.x, a.y))]),
                        _ => stepped.push(None),
                    }
                }
                if let Some(Some(last)) = pts.last() {
                    stepped.push(Some(*last));
                }
                draw_polyline(c, &stepped, stroke, s.line_style);
            }
            SeriesKind::ErrorBar => {
                draw_polyline(c, &pts, stroke, s.line_style);
                if let Some((lo, hi)) = &s.err {
                    let thin = Stroke::new(1.5_f32, color);
                    for (i, (&x, &y)) in s.x.iter().zip(&s.y).enumerate() {
                        let (Some(xp), Some(a), Some(b)) = (mx(x), my(y - lo[i]), my(y + hi[i])) else { continue };
                        c.line(&[pos2(xp, a), pos2(xp, b)], thin, &[]);
                        c.line(&[pos2(xp - 4.0, a), pos2(xp + 4.0, a)], thin, &[]);
                        c.line(&[pos2(xp - 4.0, b), pos2(xp + 4.0, b)], thin, &[]);
                    }
                }
            }
            SeriesKind::Patch => {
                let poly: Vec<Pos2> = pts.iter().flatten().copied().collect();
                if poly.len() >= 3 {
                    let fill = color.gamma_multiply(s.face_alpha.clamp(0.0, 1.0));
                    let edge = s.edge_color.filter(|_| s.line_style != LineStyle::None).map(|e| Stroke::new(1.0_f32, Color32::from_rgb(e[0], e[1], e[2]))).unwrap_or(Stroke::NONE);
                    c.polygon(&poly, fill, edge);
                }
            }
            SeriesKind::Text => {
                if let Some(p) = pts.first().copied().flatten() {
                    let color = s.color.map(|c| Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(t.fg);
                    let font = Font { size: s.font_size.map(|f| f * 1.2).unwrap_or(LABEL.size), bold: false };
                    for (i, line) in s.label.lines().enumerate() {
                        c.text(p + vec2(0.0, font.mid() + i as f32 * font.size * 1.25), Align::Min, line, font, color, 0.0);
                    }
                }
            }
            SeriesKind::ConstantLine => {
                let color = s.color.map(|c| Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(t.axis);
                let stroke = Stroke::new(s.line_width.max(1.5), color);
                let (a, b, label_at, align, angle) = if s.horizontal {
                    let Some(y) = my(s.y[0]) else { continue };
                    (pos2(plot.left(), y), pos2(plot.right(), y), pos2(plot.right() - 4.0, y - 5.0), Align::Max, 0.0)
                } else {
                    let Some(x) = mx(s.x[0]) else { continue };
                    (pos2(x, plot.top()), pos2(x, plot.bottom()), pos2(x - 5.0, plot.top() + 4.0), Align::Max, -std::f32::consts::FRAC_PI_2)
                };
                draw_polyline(c, &[Some(a), Some(b)], stroke, s.line_style);
                if !s.label.is_empty() {
                    let label_color = if s.color.is_some() { color } else { t.text };
                    c.text(label_at, align, &s.label, TICK, label_color, angle);
                }
            }
            SeriesKind::Image => {
                let (dx, dy) = graphics::image_pixel(s);
                let (xa, xb) = (s.x[0] - dx / 2.0, s.x[s.x.len() - 1] + dx / 2.0);
                let (ya, yb) = (s.y[0] - dy / 2.0, s.y[s.y.len() - 1] + dy / 2.0);
                let (Some(l), Some(r), Some(top_row), Some(bottom_row)) = (mx(xa), mx(xb), my(ya), my(yb)) else { continue };
                let mut px = image_pixels(s, &cs);
                let (rows, cols) = s.grid;
                // Row 0 is at y0: flip when y0 is drawn at the bottom.
                if top_row > bottom_row {
                    let row = cols * 4;
                    px = (0..rows).rev().flat_map(|r| px[r * row..(r + 1) * row].to_vec()).collect();
                }
                if rows > 0 && cols > 0 {
                    c.image(Rect::from_min_max(pos2(l.min(r), top_row.min(bottom_row)), pos2(l.max(r), top_row.max(bottom_row))), cols, rows, &px);
                }
            }
            SeriesKind::Surface => {
                let (rows, cols) = s.grid;
                for i in 0..rows.saturating_sub(1) {
                    for j in 0..cols.saturating_sub(1) {
                        let at_ij = |i: usize, j: usize| j * rows + i;
                        let corners = [at_ij(i, j), at_ij(i, j + 1), at_ij(i + 1, j + 1), at_ij(i + 1, j)];
                        let quad: Option<Vec<Pos2>> = corners.iter().map(|&q| at(s.x[q], s.y[q])).collect();
                        let Some(quad) = quad else { continue };
                        let value = s.c.get(corners[0]).copied().unwrap_or(f64::NAN);
                        let fill = if s.face_none { Color32::TRANSPARENT } else if s.face_mapped { cs.color(value).unwrap_or(Color32::TRANSPARENT) } else { color };
                        let edge = if s.edge_mapped { cs.color(value).map(|e| Stroke::new(0.6_f32, e)).unwrap_or(Stroke::NONE) } else { s.edge_color.map(|e| Stroke::new(0.5_f32, Color32::from_rgb(e[0], e[1], e[2]))).unwrap_or(Stroke::NONE) };
                        c.polygon(&quad, fill, edge);
                    }
                }
            }
            SeriesKind::Contour => {
                for (idx, v) in runs(&s.u, &s.v, &s.w) {
                    let poly: Vec<Pos2> = idx.iter().filter_map(|&i| at(s.u[i], s.v[i])).collect();
                    if poly.len() >= 3 {
                        let fill = cs.color(v).unwrap_or(Color32::TRANSPARENT);
                        c.polygon(&poly, fill, Stroke::new(0.6_f32, fill));
                    }
                }
                if s.line_style != LineStyle::None {
                    for (idx, v) in runs(&s.x, &s.y, &s.c) {
                        let line: Vec<Option<Pos2>> = idx.iter().map(|&i| at(s.x[i], s.y[i])).collect();
                        let color = if s.edge_mapped { cs.color(v).unwrap_or(t.axis) } else { s.edge_color.map(|e| Color32::from_rgb(e[0], e[1], e[2])).unwrap_or(t.fg) };
                        draw_polyline(c, &line, Stroke::new(s.line_width.max(1.2), color), LineStyle::Solid);
                    }
                }
            }
            SeriesKind::Quiver => {
                let thin = Stroke::new(s.line_width.max(1.2), color);
                for i in 0..s.x.len() {
                    if let (Some(a), Some(b)) = (at(s.x[i], s.y[i]), at(s.x[i] + s.u.get(i).copied().unwrap_or(0.0), s.y[i] + s.v.get(i).copied().unwrap_or(0.0))) {
                        arrow(c, a, b, thin);
                    }
                }
            }
            SeriesKind::Line | SeriesKind::Scatter => draw_polyline(c, &pts, stroke, s.line_style),
        }
        if let (Some(mk), false) = (s.marker, matches!(s.kind, SeriesKind::Text | SeriesKind::ConstantLine | SeriesKind::Region | SeriesKind::Patch)) {
            for pt in pts.iter().flatten() {
                draw_marker(c, mk, *pt, s.marker_size / 2.0 + 1.0, color, s.kind == SeriesKind::Scatter);
            }
        }
    }
    c.clip(None);

    if let Some(names) = &ax.legend {
        draw_legend(c, t, ax, names, plot);
    }

    if sx.log || sy.log {
        return;
    }
    let Some(h) = hover.filter(|h| plot.contains(*h)) else { return };
    c.clip(Some(plot));
    c.line(&[pos2(h.x, plot.top()), pos2(h.x, plot.bottom())], Stroke::new(1.0_f32, t.cross), &[3.0, 3.0]);
    c.clip(None);
    let xv = x0 + ((h.x - plot.left()) / plot.width()) as f64 * (x1 - x0);
    let mut values = Vec::new();
    for (k, s) in ax.series.iter().enumerate().filter(|(_, s)| is_line_like(s)) {
        let Some(yv) = value_at(s, xv) else { continue };
        let name = ax.legend.as_ref().and_then(|n| n.get(k)).or(s.name.as_ref()).cloned().unwrap_or_else(|| format!("y{}", k + 1));
        values.push(format!("{name} {}", readout(yv)));
        if (y0.min(y1)..=y0.max(y1)).contains(&yv) {
            c.circle(pos2(h.x, ty(yv)), 3.5, t.color(s, k), Stroke::new(1.5_f32, t.bg));
        }
    }
    if values.is_empty() {
        values.push(format!("y = {}", readout(y0 + ((plot.bottom() - h.y) / plot.height()) as f64 * (y1 - y0))));
    }
    let xs = match ax.x_kind {
        XAxisKind::Numeric => readout(xv),
        XAxisKind::DateTime => livre_lang::time::format_datetime(xv, "dd-MMM-yyyy HH:mm"),
        XAxisKind::Duration => livre_lang::time::format_duration(xv, "hh:mm:ss"),
    };
    let lines = [format!("x = {xs}"), values.join("   ")];
    let w = lines.iter().map(|l| c.text_width(l, LABEL)).fold(116.0, f32::max) + 20.0;
    let left = if h.x + 10.0 + w > plot.right() { h.x - w - 10.0 } else { h.x + 10.0 };
    let r = Rect::from_min_size(pos2(left, plot.bottom() - 52.0), vec2(w, 44.0));
    c.rect(r, RADIUS, t.badge, Stroke::NONE);
    for (i, l) in lines.iter().enumerate() {
        c.text(pos2(r.left() + 10.0, r.top() + 18.0 + i as f32 * 17.0), Align::Min, l, LABEL, Color32::WHITE, 0.0);
    }
}

fn draw_polyline(c: &mut dyn Canvas, pts: &[Option<Pos2>], stroke: Stroke, style: LineStyle) {
    for run in pts.split(Option::is_none) {
        let run: Vec<Pos2> = run.iter().flatten().copied().collect();
        if run.len() < 2 {
            continue;
        }
        match style {
            LineStyle::None => {}
            LineStyle::Solid => c.line(&run, stroke, &[]),
            LineStyle::Dashed => c.line(&run, stroke, &[7.0, 5.0]),
            LineStyle::DashDot => c.line(&run, stroke, &[9.0, 3.0, 2.0, 3.0]),
            LineStyle::Dotted => c.dots(&run, 4.0 + stroke.width, stroke.width * 0.6, stroke.color),
        }
    }
}

fn draw_marker(c: &mut dyn Canvas, m: Marker, p: Pos2, r: f32, color: Color32, filled: bool) {
    let s = Stroke::new(1.3_f32, color);
    let fill = if filled { color } else { Color32::TRANSPARENT };
    let seg = |c: &mut dyn Canvas, a: Vec2| c.line(&[p - a, p + a], s, &[]);
    match m {
        Marker::Circle => c.circle(p, r, fill, s),
        Marker::Point => c.circle(p, 2.0, color, Stroke::NONE),
        Marker::Square => c.rect(Rect::from_center_size(p, vec2(r * 1.8, r * 1.8)), 0.0, fill, s),
        Marker::Diamond => c.polygon(&[p + vec2(0.0, -r * 1.2), p + vec2(r * 1.2, 0.0), p + vec2(0.0, r * 1.2), p + vec2(-r * 1.2, 0.0)], fill, s),
        Marker::TriangleUp => c.polygon(&[p + vec2(0.0, -r * 1.2), p + vec2(r * 1.1, r * 0.8), p + vec2(-r * 1.1, r * 0.8)], fill, s),
        Marker::TriangleDown => c.polygon(&[p + vec2(0.0, r * 1.2), p + vec2(-r * 1.1, -r * 0.8), p + vec2(r * 1.1, -r * 0.8)], fill, s),
        Marker::Plus => {
            seg(c, vec2(r, 0.0));
            seg(c, vec2(0.0, r));
        }
        Marker::Cross => {
            seg(c, vec2(r, r));
            seg(c, vec2(r, -r));
        }
        Marker::Star => {
            seg(c, vec2(r, 0.0));
            seg(c, vec2(0.0, r));
            seg(c, vec2(r * 0.7, r * 0.7));
            seg(c, vec2(r * 0.7, -r * 0.7));
        }
    }
}

fn draw_legend(c: &mut dyn Canvas, t: &Tokens, ax: &Axes, names: &[String], plot: Rect) {
    let rows: Vec<(&Series, Color32, &String)> = ax.series.iter().enumerate().filter(|(_, s)| in_legend(s) && s.visible).zip(names).map(|((k, s), n)| (s, t.color(s, k), n)).collect();
    if rows.is_empty() {
        return;
    }
    let text_w = rows.iter().map(|r| c.text_width(r.2, LABEL)).fold(0.0, f32::max);
    let size = vec2(10.0 + 22.0 + 8.0 + text_w + 10.0, rows.len() as f32 * 18.0 + 8.0);
    let r = Rect::from_min_size(pos2(plot.right() - size.x - 10.0, plot.top() + 10.0), size);
    c.rect(r, RADIUS, t.bg, Stroke::new(1.0_f32, t.border));
    for (i, (s, color, name)) in rows.into_iter().enumerate() {
        let y = r.top() + 4.0 + i as f32 * 18.0 + 9.0;
        let (a, b) = (pos2(r.left() + 10.0, y), pos2(r.left() + 32.0, y));
        if s.kind == SeriesKind::Bar {
            c.rect(Rect::from_min_max(pos2(a.x + 4.0, y - 5.0), pos2(b.x - 4.0, y + 5.0)), 2.0, color, Stroke::NONE);
        } else {
            let style = if s.kind == SeriesKind::Area { LineStyle::Solid } else { s.line_style };
            draw_polyline(c, &[Some(a), Some(b)], Stroke::new(s.line_width.max(2.0), color), style);
        }
        if let Some(m) = s.marker {
            draw_marker(c, m, pos2((a.x + b.x) / 2.0, y), 3.5, color, s.kind == SeriesKind::Scatter);
        }
        c.text(pos2(b.x + 8.0, y + LABEL.mid()), Align::Min, name, LABEL, t.fg, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> egui::Context {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let _ = ctx.run(Default::default(), |_| {});
        ctx
    }

    fn line_figure() -> Figure {
        let mut fig = Figure::new(1);
        let ax = fig.axes_mut();
        ax.title = "Damped wave".into();
        ax.xlabel = "t (s)".into();
        ax.ylabel = "amplitude".into();
        ax.grid = true;
        let x: Vec<f64> = (0..=200).map(|i| i as f64 * 0.05).collect();
        ax.series.push(Series::new(SeriesKind::Line, x.clone(), x.iter().map(|t| (-0.2 * t).exp() * (2.0 * t).sin()).collect()));
        let mut env = Series::new(SeriesKind::Line, x.clone(), x.iter().map(|t| (-0.2 * t).exp()).collect());
        env.line_style = LineStyle::Dashed;
        ax.series.push(env);
        ax.legend = Some(vec!["signal".into(), "envelope".into()]);
        fig
    }

    fn bar_figure() -> Figure {
        let mut fig = Figure::new(2);
        let ax = fig.axes_mut();
        ax.title = "Counts".into();
        ax.series.push(Series::new(SeriesKind::Bar, vec![1.0, 2.0, 3.0, 4.0], vec![3.0, 1.0, 4.0, 2.0]));
        fig
    }

    #[test]
    fn svg_has_title_legend_and_parses() {
        let ctx = ctx();
        let svg = to_svg(&ctx, &Skin::light(), &line_figure(), figure_size(800.0));
        assert!(svg.contains(">Damped wave</text>"));
        assert!(svg.contains(">signal</text>") && svg.contains(">envelope</text>"));
        assert!(svg.contains(r#"stroke-dasharray="7 5""#));
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("valid SVG");
        assert_eq!((tree.size().width(), tree.size().height()), (560.0, 300.0));
    }

    #[test]
    fn png_has_expected_size() {
        let ctx = ctx();
        let png = to_png(&ctx, &Skin::dark(), &line_figure(), vec2(280.0, 150.0), 2.0).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let dim = |i: usize| u32::from_be_bytes(png[i..i + 4].try_into().unwrap());
        assert_eq!((dim(16), dim(20)), (560, 300));
    }

    #[test]
    fn paints_on_screen_with_crosshair() {
        let ctx = ctx();
        let mut fig = line_figure();
        fig.grid = (1, 2);
        fig.axes.push(bar_figure().axes.remove(0));
        let mut rect = Rect::NOTHING;
        for _ in 0..3 {
            let input = egui::RawInput { events: vec![egui::Event::PointerMoved(rect.left_top() + vec2(120.0, 90.0))], ..Default::default() };
            let out = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| rect = figure(ui, &Skin::light(), &fig, 400.0).rect);
            });
            assert!(!out.shapes.is_empty());
        }
        assert_eq!(rect.size(), figure_size(400.0));
    }

    /// saveas/print go through the exporter the engine installs; the PNG and SVG are written.
    #[test]
    fn interpreter_saves_figures() {
        let ctx = ctx();
        let dir = std::env::temp_dir().join(format!("matlivre-saveas-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut it = livre_lang::Interpreter::new();
        let c2 = ctx.clone();
        it.set_figure_exporter(move |fig, format, scale| match format {
            livre_lang::ExportFormat::Svg => Ok(to_svg(&c2, &Skin::light(), fig, vec2(560.0, 420.0)).into_bytes()),
            livre_lang::ExportFormat::Png => to_png(&c2, &Skin::light(), fig, vec2(560.0, 420.0), scale),
        });
        let p = |n: &str| dir.join(n).display().to_string().replace('\'', "''");
        let src = format!(
            concat!(
                "x = 1:10; plot(x, x.^2, 'DisplayName', 'square'); hold on\n",
                "errorbar(x, x.^1.5, 0.1*x.^1.5); fill([2 4 4 2], [10 10 40 40], 'g', 'FaceAlpha', 0.3);\n",
                "xline(5, '--', 'five'); yline(60); xregion(7, 8); text(1.5, 80, 'peak');\n",
                "xticks([0 5 10]); xticklabels({{'zero','five','ten'}}); ytickformat('usd'); xtickangle(30);\n",
                "title('Formatted'); legend show\n",
                "saveas(gcf, '{png}'); print(gcf, '{svg}', '-dsvg'); exportgraphics(gca, '{jpg}', 'Resolution', 72);\n",
                "figure; semilogy(1:4, [1 10 100 1000], 'o-'); grid on; saveas(gcf, '{log}'); barh([3 1 2]);\n",
            ),
            png = p("fmt.png"),
            svg = p("fmt.svg"),
            jpg = p("fmt.jpg"),
            log = p("log.png"),
        );
        it.run_source(&src).unwrap();
        let png = std::fs::read(dir.join("fmt.png")).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let svg = std::fs::read_to_string(dir.join("fmt.svg")).unwrap();
        assert!(svg.contains(">five</text>") && svg.contains(">peak</text>") && svg.contains("$20.00"), "{svg}");
        assert!(std::fs::read(dir.join("fmt.jpg")).unwrap().starts_with(&[0xFF, 0xD8]));
        if std::env::var_os("LIVRE_PLOT_PREVIEW").is_some() {
            let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
            std::fs::copy(dir.join("fmt.png"), target.join("plot-fmt.png")).unwrap();
            std::fs::copy(dir.join("log.png"), target.join("plot-log.png")).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Surfaces, contours, images and 3-D lines render (written to `target/` for previews).
    #[test]
    fn renders_3d_images_and_contours() {
        let ctx = ctx();
        let mut it = livre_lang::Interpreter::new();
        let c2 = ctx.clone();
        it.set_figure_exporter(move |fig, format, scale| match format {
            livre_lang::ExportFormat::Svg => Ok(to_svg(&c2, &Skin::light(), fig, vec2(560.0, 420.0)).into_bytes()),
            livre_lang::ExportFormat::Png => to_png(&c2, &Skin::light(), fig, vec2(560.0, 420.0), scale),
        });
        let dir = std::env::temp_dir().join(format!("matlivre-3d-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = |n: &str| dir.join(n).display().to_string().replace('\'', "''");
        let src = format!(
            concat!(
                "surf(peaks(25)); title('surf'); xlabel('x'); ylabel('y'); zlabel('z'); colorbar; saveas(gcf, '{a}');\n",
                "figure; subplot(2, 2, 1); contour(peaks(40), 12); subplot(2, 2, 2); contourf(peaks(40), 6); colormap(jet);\n",
                "subplot(2, 2, 3); imagesc(magic(8)); subplot(2, 2, 4); [x, y] = meshgrid(-2:0.5:2); quiver(x, y, -y, x); saveas(gcf, '{b}');\n",
                "figure; t = linspace(0, 6*pi, 200); plot3(sin(t), cos(t), t, 'LineWidth', 2); hold on; [X, Y, Z] = sphere(16); mesh(X, Y, Z + 10); grid on; saveas(gcf, '{c}');\n",
            ),
            a = p("surf.png"),
            b = p("2d.png"),
            c = p("lines3.png"),
        );
        it.run_source(&src).unwrap();
        let svg = to_svg(&ctx, &Skin::light(), &it.figures_snapshot()[1], vec2(560.0, 420.0));
        assert!(svg.contains("data:image/png;base64,"), "imagesc embeds an image");
        for n in ["surf.png", "2d.png", "lines3.png"] {
            assert!(std::fs::read(dir.join(n)).unwrap().starts_with(b"\x89PNG"));
        }
        if std::env::var_os("LIVRE_PLOT_PREVIEW").is_some() {
            let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
            for n in ["surf.png", "2d.png", "lines3.png"] {
                std::fs::copy(dir.join(n), target.join(format!("plot-{n}"))).unwrap();
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `LIVRE_PLOT_PREVIEW=1 cargo test -p livre-app preview` writes sample renders to `target/`.
    #[test]
    fn preview() {
        if std::env::var_os("LIVRE_PLOT_PREVIEW").is_none() {
            return;
        }
        let ctx = ctx();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let mut subplots = line_figure();
        subplots.grid = (2, 2);
        let mut bars = Axes { title: "Bars".into(), grid: true, ..Default::default() };
        bars.series.push(Series::new(SeriesKind::Bar, vec![1.0, 2.0, 3.0, 4.0], vec![3.0, -1.0, 4.0, 2.0]));
        let mut area = Axes { title: "Area + stairs".into(), xlabel: "x".into(), ..Default::default() };
        area.series.push(Series::new(SeriesKind::Area, vec![0.0, 1.0, 2.0, 3.0, 4.0], vec![1.0, -0.5, 2.0, 1.5, 0.5]));
        area.series.push(Series::new(SeriesKind::Stairs, vec![0.0, 1.0, 2.0, 3.0, 4.0], vec![2.0, 1.0, 2.5, 0.0, 1.0]));
        let mut scatter = Axes { title: "Scatter".into(), ..Default::default() };
        scatter.series.push(Series::new(SeriesKind::Scatter, vec![1.0, 2.0, 3.0, 4.0, 5.0], vec![2.0, 3.5, 1.0, 4.0, 2.5]));
        let mut stem = Series::new(SeriesKind::Stem, vec![1.0, 2.0, 3.0, 4.0, 5.0], vec![1.0, -2.0, 1.5, 0.5, -1.0]);
        stem.color = Some([200, 60, 60]);
        scatter.series.push(stem);
        subplots.axes.extend([bars, area, scatter]);
        for (name, skin, fig) in [("plot-light", Skin::light(), line_figure()), ("plot-dark", Skin::dark(), line_figure()), ("plot-subplots", Skin::light(), subplots)] {
            std::fs::write(dir.join(format!("{name}.png")), to_png(&ctx, &skin, &fig, figure_size(800.0), 2.0).unwrap()).unwrap();
            std::fs::write(dir.join(format!("{name}.svg")), to_svg(&ctx, &skin, &fig, figure_size(800.0))).unwrap();
        }
        for (name, skin, fig, at) in [("plot-hover", Skin::light(), line_figure(), pos2(250.0, 150.0)), ("plot-hover-dark", Skin::dark(), line_figure(), pos2(500.0, 120.0)), ("plot-hover-bars", Skin::light(), bar_figure(), pos2(300.0, 150.0))] {
            let svg = svg_document(&ctx, &skin, &fig, DESIGN, Some(at));
            std::fs::write(dir.join(format!("{name}.png")), rasterize(&svg, DESIGN, 2.0).unwrap().encode_png().unwrap()).unwrap();
        }
    }
}
