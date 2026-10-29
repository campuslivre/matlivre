//! Custom-painted controls matching ModernWinForms: buttons with the Win11 elevation stroke
//! (hairline border, stronger bottom edge), toggle switch, segmented control, caption buttons.

use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Response, Rounding, Sense, Shape, Stroke, Ui};

use crate::theme::{Skin, RADIUS};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    Standard,
    Accent,
    Subtle,
    /// Subtle, drawn pressed in: a command that is switched on.
    Toggled,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    New,
    Open,
    Save,
    Text,
    Code,
    SectionBreak,
    Heading,
    Run,
    RunAll,
    RunAdvance,
    Stop,
    Clear,
    Delete,
    Up,
    Down,
    SaveAs,
    ClearOutput,
    Workspace,
    Help,
}

/// Paints the Win11 elevation stroke: a faint all-round border and a stronger bottom edge.
pub fn elevation_stroke(p: &egui::Painter, r: Rect, border: Color32, bottom: Color32) {
    p.rect_stroke(r.shrink(0.5), Rounding::same(RADIUS), Stroke::new(1.0_f32, border));
    if bottom != border {
        let y = r.bottom() - 0.5;
        p.line_segment([pos2(r.left() + RADIUS, y), pos2(r.right() - RADIUS, y)], Stroke::new(1.0_f32, bottom));
    }
}

pub fn paint_icon(p: &egui::Painter, icon: Icon, c: Pos2, color: Color32) {
    let s = Stroke::new(1.4_f32, color);
    let r = |x: f32, y: f32| pos2(c.x + x, c.y + y);
    match icon {
        Icon::New => {
            p.add(Shape::closed_line(vec![r(-5.0, -7.0), r(2.0, -7.0), r(5.5, -3.5), r(5.5, 7.0), r(-5.0, 7.0)], s));
            p.line_segment([r(2.0, -7.0), r(2.0, -3.5)], s);
            p.line_segment([r(2.0, -3.5), r(5.5, -3.5)], s);
        }
        Icon::Open => {
            p.add(Shape::closed_line(vec![r(-7.0, -5.0), r(-2.5, -5.0), r(-1.0, -3.0), r(6.0, -3.0), r(6.0, 6.0), r(-7.0, 6.0)], s));
            p.line_segment([r(-7.0, -0.5), r(6.0, -0.5)], s);
        }
        Icon::Save => {
            p.add(Shape::closed_line(vec![r(-6.5, -6.5), r(4.0, -6.5), r(6.5, -4.0), r(6.5, 6.5), r(-6.5, 6.5)], s));
            p.rect_stroke(Rect::from_min_max(r(-3.5, -6.5), r(2.5, -2.5)), Rounding::ZERO, s);
            p.rect_stroke(Rect::from_min_max(r(-4.0, 1.0), r(4.0, 6.5)), Rounding::ZERO, s);
        }
        Icon::Text => {
            for (i, w) in [12.0, 9.0, 12.0, 7.0].iter().enumerate() {
                let y = -5.5 + i as f32 * 3.7;
                p.line_segment([r(-6.0, y), r(-6.0 + w, y)], s);
            }
        }
        Icon::Code => {
            p.add(Shape::line(vec![r(-2.5, -5.0), r(-7.0, 0.0), r(-2.5, 5.0)], s));
            p.add(Shape::line(vec![r(2.5, -5.0), r(7.0, 0.0), r(2.5, 5.0)], s));
        }
        Icon::SectionBreak => {
            p.rect_stroke(Rect::from_min_max(r(-6.0, -7.0), r(6.0, -2.5)), Rounding::same(1.0), s);
            p.rect_stroke(Rect::from_min_max(r(-6.0, 2.5), r(6.0, 7.0)), Rounding::same(1.0), s);
            for x in [-7.0, -3.5, 0.0, 3.5] {
                p.line_segment([r(x, 0.0), r(x + 2.0, 0.0)], s);
            }
        }
        Icon::Heading => {
            p.line_segment([r(-5.5, -6.0), r(-5.5, 6.0)], s);
            p.line_segment([r(5.5, -6.0), r(5.5, 6.0)], s);
            p.line_segment([r(-5.5, 0.0), r(5.5, 0.0)], s);
        }
        Icon::Run | Icon::RunAll | Icon::RunAdvance => {
            let fill = if icon == Icon::Run { color } else { Color32::TRANSPARENT };
            let dx = if icon == Icon::Run { 0.0 } else { -2.5 };
            p.add(Shape::convex_polygon(vec![r(-4.5 + dx, -6.5), r(6.0 + dx, 0.0), r(-4.5 + dx, 6.5)], fill, s));
            if icon == Icon::RunAll {
                p.line_segment([r(6.5, -6.5), r(6.5, 6.5)], s);
            }
            if icon == Icon::RunAdvance {
                p.add(Shape::line(vec![r(5.0, 3.0), r(7.5, 6.0), r(10.0, 3.0)], s));
            }
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(11.0, 11.0)), Rounding::same(2.0), color);
        }
        Icon::Clear => {
            p.line_segment([r(-6.0, 6.0), r(6.0, 6.0)], s);
            p.add(Shape::closed_line(vec![r(-5.0, 2.0), r(2.0, -6.0), r(7.0, -1.5), r(1.0, 5.0), r(-2.5, 5.0)], s));
        }
        Icon::Delete => {
            p.line_segment([r(-4.0, -4.0), r(4.0, 4.0)], s);
            p.line_segment([r(4.0, -4.0), r(-4.0, 4.0)], s);
        }
        Icon::Up => {
            p.add(Shape::line(vec![r(-4.5, 2.5), r(0.0, -2.5), r(4.5, 2.5)], s));
        }
        Icon::Down => {
            p.add(Shape::line(vec![r(-4.5, -2.5), r(0.0, 2.5), r(4.5, -2.5)], s));
        }
        Icon::SaveAs => {
            p.add(Shape::line(vec![r(0.5, 6.5), r(-6.5, 6.5), r(-6.5, -6.5), r(4.0, -6.5), r(6.5, -4.0), r(6.5, -1.0)], s));
            p.rect_stroke(Rect::from_min_max(r(-3.5, -6.5), r(2.5, -2.5)), Rounding::ZERO, s);
            // Pencil.
            p.line_segment([r(2.5, 7.5), r(8.0, 2.0)], Stroke::new(2.4_f32, color));
        }
        Icon::ClearOutput => {
            for (i, w) in [12.0, 8.0, 5.0].iter().enumerate() {
                let y = -5.5 + i as f32 * 4.0;
                p.line_segment([r(-7.0, y), r(-7.0 + w, y)], s);
            }
            p.line_segment([r(1.5, 1.5), r(7.0, 7.0)], s);
            p.line_segment([r(7.0, 1.5), r(1.5, 7.0)], s);
        }
        Icon::Workspace => {
            p.rect_stroke(Rect::from_min_max(r(-7.0, -6.0), r(7.0, 6.0)), Rounding::same(1.5), s);
            p.line_segment([r(2.0, -6.0), r(2.0, 6.0)], s);
            for y in [-2.5, 0.5, 3.5] {
                p.line_segment([r(3.8, y), r(5.3, y)], s);
            }
        }
        Icon::Help => {
            p.circle_stroke(c, 7.0, s);
            p.text(r(0.0, 0.5), Align2::CENTER_CENTER, "?", FontId::proportional(11.0), color);
        }
    }
}

/// A ModernButton: optional icon, text, Standard / Accent / Subtle style.
pub fn button(ui: &mut Ui, skin: &Skin, style: ButtonStyle, icon: Option<Icon>, text: &str, enabled: bool) -> Response {
    button_impl(ui, skin, style, icon, text, enabled, false)
}

/// A subtle ModernButton with a trailing chevron, for a command that opens a menu.
pub fn dropdown_button(ui: &mut Ui, skin: &Skin, icon: Option<Icon>, text: &str, enabled: bool) -> Response {
    button_impl(ui, skin, ButtonStyle::Subtle, icon, text, enabled, true)
}

fn button_impl(ui: &mut Ui, skin: &Skin, style: ButtonStyle, icon: Option<Icon>, text: &str, enabled: bool, chevron: bool) -> Response {
    let font = FontId::proportional(14.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER);
    let icon_w = if icon.is_some() { 16.0 + if text.is_empty() { 0.0 } else { 8.0 } } else { 0.0 };
    let chevron_w = if chevron { 16.0 } else { 0.0 };
    let pad = if text.is_empty() { 8.0 } else { 12.0 };
    let size = vec2((galley.size().x + icon_w + chevron_w + pad * 2.0).max(32.0), 32.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let hovered = enabled && resp.hovered();
        let pressed = enabled && resp.is_pointer_button_down_on();
        let (fill, fg, stroke, bottom) = match style {
            ButtonStyle::Accent => {
                let fill = if !enabled {
                    if skin.dark { Color32::from_rgb(67, 67, 67) } else { Color32::from_rgba_unmultiplied(0, 0, 0, 55) }
                } else if pressed {
                    skin.accent_pressed
                } else if hovered {
                    skin.accent_hover
                } else {
                    skin.accent
                };
                let fg = if enabled { skin.on_accent } else { skin.text_disabled };
                (fill, fg, Color32::from_rgba_unmultiplied(255, 255, 255, 20), Color32::from_rgba_unmultiplied(0, 0, 0, 60))
            }
            ButtonStyle::Standard => {
                let fill = if pressed { skin.button_pressed } else if hovered { skin.button_hover } else { skin.button_fill };
                let fg = if !enabled { skin.text_disabled } else if pressed { skin.text_secondary } else { skin.text };
                (fill, fg, skin.button_stroke, skin.button_stroke_bottom)
            }
            ButtonStyle::Subtle | ButtonStyle::Toggled => {
                let rest = if style == ButtonStyle::Toggled { skin.subtle_selected } else { Color32::TRANSPARENT };
                let fill = if pressed { skin.subtle_pressed } else if hovered { skin.subtle_hover } else { rest };
                let fg = if enabled { skin.text } else { skin.text_disabled };
                (fill, fg, Color32::TRANSPARENT, Color32::TRANSPARENT)
            }
        };
        p.rect_filled(rect, Rounding::same(RADIUS), fill);
        if stroke != Color32::TRANSPARENT && !pressed {
            elevation_stroke(p, rect, stroke, bottom);
        } else if stroke != Color32::TRANSPARENT {
            p.rect_stroke(rect.shrink(0.5), Rounding::same(RADIUS), Stroke::new(1.0_f32, stroke));
        }
        let content_w = icon_w + galley.size().x + chevron_w;
        let mut x = rect.center().x - content_w / 2.0;
        if let Some(ic) = icon {
            paint_icon(p, ic, pos2(x + 8.0, rect.center().y), fg);
            x += icon_w;
        }
        let text_w = galley.size().x;
        if !text.is_empty() {
            p.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
        }
        if chevron {
            let c = pos2(x + text_w + chevron_w - 4.0, rect.center().y);
            p.add(Shape::line(vec![pos2(c.x - 3.5, c.y - 1.5), pos2(c.x, c.y + 2.0), pos2(c.x + 3.5, c.y - 1.5)], Stroke::new(1.2_f32, fg)));
        }
    }
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Vertical separator between command-bar groups.
pub fn bar_separator(ui: &mut Ui, skin: &Skin) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 32.0), Sense::hover());
    let x = rect.center().x;
    ui.painter().line_segment([pos2(x, rect.top() + 7.0), pos2(x, rect.bottom() - 7.0)], Stroke::new(1.0_f32, skin.separator));
}

/// ModernToggleSwitch: 40×20 pill with an animated thumb.
pub fn toggle_switch(ui: &mut Ui, skin: &Skin, on: &mut bool, label: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(14.0), skin.text);
    let size = vec2(40.0 + if label.is_empty() { 0.0 } else { 10.0 + galley.size().x }, 32.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let track = Rect::from_min_size(pos2(rect.left(), rect.center().y - 10.0), vec2(40.0, 20.0));
    let p = ui.painter();
    let hovered = resp.hovered();
    if *on {
        let fill = if hovered { skin.accent_hover } else { skin.accent };
        p.rect_filled(track, Rounding::same(10.0), fill);
    } else {
        p.rect_filled(track, Rounding::same(10.0), if hovered { skin.subtle_hover } else { Color32::from_rgba_unmultiplied(0, 0, 0, 6) });
        let off_border = if skin.dark { Color32::from_rgb(160, 160, 160) } else { skin.text_secondary };
        p.rect_stroke(track.shrink(0.5), Rounding::same(10.0), Stroke::new(1.0_f32, off_border));
    }
    let thumb_r = if hovered { 7.0 } else { 6.0 };
    let x = egui::lerp(track.left() + 10.0..=track.right() - 10.0, t);
    let thumb = if *on { skin.on_accent } else if skin.dark { Color32::from_rgb(204, 204, 204) } else { skin.text_secondary };
    p.circle_filled(pos2(x, track.center().y), thumb_r, thumb);
    if !label.is_empty() {
        p.galley(pos2(track.right() + 10.0, rect.center().y - galley.size().y / 2.0), galley, skin.text);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// ModernSegmentedControl: returns the clicked index.
pub fn segmented(ui: &mut Ui, skin: &Skin, items: &[&str], selected: usize) -> Option<usize> {
    let font = FontId::proportional(13.0);
    let widths: Vec<f32> = items.iter().map(|s| ui.painter().layout_no_wrap(s.to_string(), font.clone(), skin.text).size().x + 24.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 4.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, 32.0), Sense::hover());
    let track = Rect::from_center_size(rect.center(), vec2(total, 28.0));
    let p = ui.painter().clone();
    p.rect_filled(track, Rounding::same(RADIUS), skin.track);
    p.rect_stroke(track.shrink(0.5), Rounding::same(RADIUS), Stroke::new(1.0_f32, skin.button_stroke));
    let mut x = track.left() + 2.0;
    let mut clicked = None;
    for (i, (label, w)) in items.iter().zip(&widths).enumerate() {
        let seg = Rect::from_min_size(pos2(x, track.top() + 2.0), vec2(*w, track.height() - 4.0));
        let resp = ui.interact(seg, ui.id().with(("seg", label)), Sense::click());
        if i == selected {
            p.rect_filled(seg, Rounding::same(RADIUS - 1.0), skin.button_fill);
            elevation_stroke(&p, seg, skin.button_stroke, skin.button_stroke_bottom);
            let pill = Rect::from_center_size(pos2(seg.center().x, seg.bottom() - 2.0), vec2(16.0, 3.0));
            p.rect_filled(pill, Rounding::same(1.5), skin.accent);
        } else if resp.hovered() {
            p.rect_filled(seg, Rounding::same(RADIUS - 1.0), skin.subtle_hover);
        }
        let fg = if i == selected { skin.text } else { skin.text_secondary };
        p.text(seg.center() - vec2(0.0, 1.0), Align2::CENTER_CENTER, *label, font.clone(), fg);
        if resp.clicked() {
            clicked = Some(i);
        }
        x += w;
    }
    clicked
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Caption {
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// Title-bar caption button (46×32), close turns red on hover.
pub fn caption_button(ui: &mut Ui, skin: &Skin, kind: Caption) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(46.0, crate::theme::TITLE_BAR_HEIGHT), Sense::click());
    let p = ui.painter();
    let hovered = resp.hovered();
    let pressed = resp.is_pointer_button_down_on();
    let mut fg = skin.text;
    if kind == Caption::Close && (hovered || pressed) {
        let c = if pressed { Color32::from_rgb(199, 64, 52) } else { skin.close_hover };
        p.rect_filled(rect, Rounding::ZERO, c);
        fg = Color32::WHITE;
    } else if pressed {
        p.rect_filled(rect, Rounding::ZERO, skin.subtle_pressed);
    } else if hovered {
        p.rect_filled(rect, Rounding::ZERO, skin.subtle_hover);
    }
    let c = rect.center();
    let s = Stroke::new(1.0_f32, fg);
    match kind {
        Caption::Minimize => {
            p.line_segment([pos2(c.x - 5.0, c.y + 0.5), pos2(c.x + 5.0, c.y + 0.5)], s);
        }
        Caption::Maximize => {
            p.rect_stroke(Rect::from_center_size(c, vec2(10.0, 10.0)), Rounding::same(1.5), s);
        }
        Caption::Restore => {
            p.rect_stroke(Rect::from_min_size(pos2(c.x - 5.0, c.y - 3.0), vec2(8.0, 8.0)), Rounding::same(1.5), s);
            p.add(Shape::line(vec![pos2(c.x - 3.0, c.y - 5.0), pos2(c.x + 5.0, c.y - 5.0), pos2(c.x + 5.0, c.y + 3.0)], s));
        }
        Caption::Close => {
            p.line_segment([pos2(c.x - 5.0, c.y - 5.0), pos2(c.x + 5.0, c.y + 5.0)], s);
            p.line_segment([pos2(c.x + 5.0, c.y - 5.0), pos2(c.x - 5.0, c.y + 5.0)], s);
        }
    }
    resp
}

/// What happened to a title-bar tab this frame.
pub struct TabResponse {
    pub selected: bool,
    pub close: bool,
    /// The tab body, which can be dragged.
    pub body: Response,
}

/// One ModernTabStrip tab: rounded, the selected one lifted onto the surface colour with a
/// hairline border and a semibold title; a close cross on the right, replaced by a dot while
/// the document has unsaved changes and the pointer is elsewhere. Middle-click closes.
#[allow(clippy::too_many_arguments)]
pub fn title_tab(ui: &mut Ui, skin: &Skin, id: egui::Id, title: &str, tip: &str, width: f32, active: bool, dirty: bool, busy: bool) -> TabResponse {
    let (slot, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    let resp = ui.interact(slot, id, Sense::click_and_drag());
    let close_rect = Rect::from_center_size(pos2(slot.right() - 16.0, slot.center().y), vec2(20.0, 20.0));
    let close = ui.interact(close_rect, id.with("close"), Sense::click());
    let hot = resp.hovered() || close.hovered();
    let p = ui.painter();
    if active {
        p.rect_filled(slot, Rounding::same(5.0), skin.surface);
        p.rect_stroke(slot.shrink(0.5), Rounding::same(5.0), Stroke::new(1.0_f32, skin.border));
    } else if hot {
        p.rect_filled(slot, Rounding::same(5.0), skin.subtle_hover);
    }

    let mut x = slot.left() + 12.0;
    if busy {
        p.circle_filled(pos2(x + 3.5, slot.center().y), 3.5, skin.accent);
        x += 14.0;
    }
    let family = if active { egui::FontFamily::Name("semibold".into()) } else { egui::FontFamily::Proportional };
    let color = if active { skin.text } else { skin.text_secondary };
    let mut job = egui::text::LayoutJob::simple_singleline(title.to_string(), FontId::new(13.0, family), color);
    job.wrap = egui::text::TextWrapping { break_anywhere: true, ..egui::text::TextWrapping::truncate_at_width((close_rect.left() - 6.0 - x).max(0.0)) };
    let galley = ui.fonts(|f| f.layout_job(job));
    p.galley(pos2(x, slot.center().y - galley.size().y / 2.0), galley, color);

    let c = close_rect.center();
    if dirty && !hot {
        p.circle_filled(c, 3.5, skin.text_secondary);
    } else {
        let fg = if close.hovered() {
            p.rect_filled(close_rect, Rounding::same(4.0), skin.subtle_selected);
            skin.text
        } else {
            skin.text_secondary
        };
        let s = Stroke::new(1.2_f32, fg);
        p.line_segment([pos2(c.x - 4.0, c.y - 4.0), pos2(c.x + 4.0, c.y + 4.0)], s);
        p.line_segment([pos2(c.x + 4.0, c.y - 4.0), pos2(c.x - 4.0, c.y + 4.0)], s);
    }
    let close_clicked = close.clicked() || resp.clicked_by(egui::PointerButton::Middle);
    let selected = resp.clicked() || resp.drag_started();
    let body = if resp.dragged() { resp } else { resp.on_hover_text(tip) };
    TabResponse { selected, close: close_clicked, body }
}

/// The + button after the last tab.
pub fn add_tab_button(ui: &mut Ui, skin: &Skin) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(5.0), skin.subtle_hover);
    }
    let c = rect.center();
    let s = Stroke::new(1.4_f32, skin.text);
    ui.painter().line_segment([pos2(c.x - 6.0, c.y), pos2(c.x + 6.0, c.y)], s);
    ui.painter().line_segment([pos2(c.x, c.y - 6.0), pos2(c.x, c.y + 6.0)], s);
    resp
}

/// The ModernProgressRing indeterminate spinner.
pub fn progress_ring(ui: &mut Ui, skin: &Skin, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    let c = rect.center();
    let r = size / 2.0 - 2.0;
    let start = t * 5.0;
    let sweep = 1.2 + (t * 2.4).sin().abs() * 2.6;
    let pts: Vec<Pos2> = (0..=24).map(|k| {
        let a = start + sweep * k as f32 / 24.0;
        pos2(c.x + r * a.cos(), c.y + r * a.sin())
    }).collect();
    ui.painter().add(Shape::line(pts, Stroke::new(2.0_f32, skin.accent)));
    ui.ctx().request_repaint();
}
