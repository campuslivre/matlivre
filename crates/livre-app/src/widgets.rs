//! Custom-painted controls matching ModernWinForms: subtle command-bar buttons, menus, the
//! title-bar TabStrip, caption buttons, splitters, tree rows, and the text box, track bar,
//! combo box and check box the live controls use.

use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Id, Pos2, Rect, Response, Rounding, Sense, Shape, Stroke, Ui};

use crate::icons::{self, Ic};
use crate::theme::{semibold, Skin, RADIUS, RADIUS_LARGE};

/// Paints the Win11 elevation stroke: a faint all-round border and a stronger bottom edge.
pub fn elevation_stroke(p: &egui::Painter, r: Rect, border: Color32, bottom: Color32) {
    p.rect_stroke(r.shrink(0.5), Rounding::same(RADIUS), Stroke::new(1.0_f32, border));
    if bottom != border {
        let y = r.bottom() - 0.5;
        p.line_segment([pos2(r.left() + RADIUS, y), pos2(r.right() - RADIUS, y)], Stroke::new(1.0_f32, bottom));
    }
}

fn chevron_down(p: &egui::Painter, c: Pos2, color: Color32) {
    p.add(Shape::line(vec![pos2(c.x - 4.0, c.y - 2.0), pos2(c.x, c.y + 2.0), pos2(c.x + 4.0, c.y - 2.0)], Stroke::new(1.0_f32, color)));
}

fn chevron_right(p: &egui::Painter, c: Pos2, color: Color32) {
    p.add(Shape::line(vec![pos2(c.x - 2.0, c.y - 4.0), pos2(c.x + 2.0, c.y), pos2(c.x - 2.0, c.y + 4.0)], Stroke::new(1.0_f32, color)));
}

/// A transparent (subtle) ModernButton, 32 px tall: optional colour icon, label and drop-down
/// chevron. `selected` keeps the `--sel` fill (a panel toggle whose panel is open).
pub fn cmd_button(ui: &mut Ui, skin: &Skin, icon: Option<Ic>, text: &str, enabled: bool, dropdown: bool, selected: bool) -> Response {
    let font = FontId::proportional(14.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER);
    let icon_w = match (icon.is_some(), text.is_empty()) {
        (false, _) => 0.0,
        (true, true) => 16.0,
        (true, false) => 16.0 + 6.0,
    };
    let chevron_w = if dropdown { 8.0 + 12.0 } else { 0.0 };
    let (pad_l, pad_r) = if text.is_empty() && !dropdown { (8.0, 8.0) } else if dropdown { (12.0, 6.0) } else { (12.0, 12.0) };
    let w = (pad_l + icon_w + galley.size().x + chevron_w + pad_r).max(32.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 32.0), sense);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let fill = if !enabled {
            Color32::TRANSPARENT
        } else if resp.is_pointer_button_down_on() {
            skin.subtle_pressed
        } else if resp.hovered() {
            skin.subtle_hover
        } else {
            Color32::TRANSPARENT
        };
        if selected {
            p.rect_filled(rect, Rounding::same(RADIUS), skin.selected);
        }
        p.rect_filled(rect, Rounding::same(RADIUS), fill);
        let fg = if enabled { skin.text } else { skin.text_disabled };
        let content = icon_w + galley.size().x;
        let mut x = if dropdown { rect.left() + pad_l } else { rect.center().x - content / 2.0 };
        if let Some(ic) = icon {
            icons::paint_with(ui, skin, ic, pos2(x + 8.0, rect.center().y), 16.0, !enabled, fg);
            x += icon_w;
        }
        if !text.is_empty() {
            p.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley.clone(), fg);
        }
        if dropdown {
            chevron_down(p, pos2(rect.right() - pad_r - 6.0, rect.center().y + 0.5), fg);
        }
    }
    resp
}

/// The 1 × 18 divider between command-bar groups.
pub fn bar_divider(ui: &mut Ui, skin: &Skin) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 32.0), Sense::hover());
    let x = rect.center().x.round() + 0.5;
    ui.painter().line_segment([pos2(x, rect.center().y - 9.0), pos2(x, rect.center().y + 9.0)], Stroke::new(1.0_f32, skin.separator));
}

/// ModernSegmentedControl: returns the clicked index.
pub fn segmented(ui: &mut Ui, skin: &Skin, items: &[&str], selected: usize) -> Option<usize> {
    let widths: Vec<f32> =
        items.iter().map(|s| ui.painter().layout_no_wrap(s.to_string(), semibold(14.0), skin.text).size().x + 28.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 4.0;
    let (track, _) = ui.allocate_exact_size(vec2(total, 32.0), Sense::hover());
    let p = ui.painter().clone();
    p.rect_filled(track, Rounding::same(RADIUS), skin.track);
    p.rect_stroke(track.shrink(0.5), Rounding::same(RADIUS), Stroke::new(1.0_f32, skin.separator));
    let mut x = track.left() + 2.0;
    let mut clicked = None;
    for (i, (label, w)) in items.iter().zip(&widths).enumerate() {
        let seg = Rect::from_min_size(pos2(x, track.top() + 2.0), vec2(*w, track.height() - 4.0));
        let resp = ui.interact(seg, ui.id().with(("seg", label)), Sense::click());
        if i == selected {
            p.rect_filled(seg, Rounding::same(RADIUS - 1.0), if skin.dark { skin.button_fill } else { Color32::WHITE });
            elevation_stroke(&p, seg, skin.button_stroke, skin.button_stroke_bottom);
        } else if resp.hovered() {
            p.rect_filled(seg, Rounding::same(RADIUS - 1.0), skin.subtle_hover);
        }
        let font = if i == selected { semibold(14.0) } else { FontId::proportional(14.0) };
        p.text(seg.center(), Align2::CENTER_CENTER, *label, font, skin.text);
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

/// Windows 11 caption button, 46 × 40, with a 10 px glyph drawn in 1 px strokes.
pub fn caption_button(ui: &mut Ui, skin: &Skin, kind: Caption) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(46.0, crate::theme::TITLE_BAR_HEIGHT), Sense::click());
    let p = ui.painter();
    let hovered = resp.hovered();
    let pressed = resp.is_pointer_button_down_on();
    let mut fg = skin.text;
    if kind == Caption::Close && (hovered || pressed) {
        p.rect_filled(rect, Rounding::ZERO, if pressed { skin.close_pressed } else { skin.close_hover });
        fg = Color32::WHITE;
    } else if pressed {
        p.rect_filled(rect, Rounding::ZERO, skin.subtle_pressed);
    } else if hovered {
        p.rect_filled(rect, Rounding::ZERO, skin.subtle_hover);
    }
    // The glyph's 10 × 10 box, on the pixel grid.
    let o = pos2((rect.center().x - 5.0).round(), (rect.center().y - 5.0).round());
    let s = Stroke::new(1.0_f32, fg);
    match kind {
        Caption::Minimize => {
            p.line_segment([pos2(o.x, o.y + 5.5), pos2(o.x + 10.0, o.y + 5.5)], s);
        }
        Caption::Maximize => {
            p.rect_stroke(Rect::from_min_size(o + vec2(0.5, 0.5), vec2(9.0, 9.0)), Rounding::same(1.5), s);
        }
        Caption::Restore => {
            p.rect_stroke(Rect::from_min_size(o + vec2(0.5, 2.5), vec2(7.0, 7.0)), Rounding::same(1.5), s);
            p.add(Shape::line(vec![o + vec2(2.5, 0.5), o + vec2(8.0, 0.5), o + vec2(9.5, 2.0), o + vec2(9.5, 7.5)], s));
        }
        Caption::Close => {
            p.line_segment([o + vec2(0.5, 0.5), o + vec2(9.5, 9.5)], s);
            p.line_segment([o + vec2(9.5, 0.5), o + vec2(0.5, 9.5)], s);
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

/// One ModernWinForms TabStrip tab (30 px, 5 px radius): the selected one filled with
/// `--ts-sel` and its border; a 7 px accent dot while the document runs; a 20 px close
/// button, replaced by a dot while there are unsaved changes and the pointer is elsewhere.
/// Middle-click closes.
#[allow(clippy::too_many_arguments)]
pub fn title_tab(ui: &mut Ui, skin: &Skin, id: Id, title: &str, tip: &str, width: f32, active: bool, dirty: bool, busy: bool) -> TabResponse {
    let (slot, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    let resp = ui.interact(slot, id, Sense::click_and_drag());
    let close_rect = Rect::from_center_size(pos2(slot.right() - 6.0 - 10.0, slot.center().y), vec2(20.0, 20.0));
    let close = ui.interact(close_rect, id.with("close"), Sense::click());
    let hot = resp.hovered() || close.hovered();
    let p = ui.painter();
    if active {
        p.rect_filled(slot, Rounding::same(5.0), skin.tab_selected);
        p.rect_stroke(slot.shrink(0.5), Rounding::same(5.0), Stroke::new(1.0_f32, skin.tab_selected_border));
    } else if hot {
        p.rect_filled(slot, Rounding::same(5.0), skin.subtle_hover);
    }

    let mut x = slot.left() + 12.0;
    if busy {
        p.circle_filled(pos2(x + 3.5, slot.center().y), 3.5, skin.accent);
        x += 7.0 + 7.0;
    }
    let mut job = egui::text::LayoutJob::simple_singleline(title.to_string(), FontId::proportional(14.0), skin.text);
    job.wrap = egui::text::TextWrapping { break_anywhere: true, ..egui::text::TextWrapping::truncate_at_width((close_rect.left() - 4.0 - x).max(0.0)) };
    let galley = ui.fonts(|f| f.layout_job(job));
    p.galley(pos2(x, slot.center().y - galley.size().y / 2.0), galley, skin.text);

    let c = close_rect.center();
    if dirty && !hot {
        p.circle_filled(c, 3.5, skin.text_secondary);
    } else {
        let fg = if close.hovered() {
            p.rect_filled(close_rect, Rounding::same(4.0), skin.subtle_hover);
            skin.text
        } else {
            skin.text_tertiary
        };
        let s = Stroke::new(1.0_f32, fg);
        p.line_segment([pos2(c.x - 4.0, c.y - 4.0), pos2(c.x + 4.0, c.y + 4.0)], s);
        p.line_segment([pos2(c.x + 4.0, c.y - 4.0), pos2(c.x - 4.0, c.y + 4.0)], s);
    }
    let close_clicked = close.clicked() || resp.clicked_by(egui::PointerButton::Middle);
    let selected = resp.clicked() || resp.drag_started();
    let body = if resp.dragged() { resp } else { resp.on_hover_text(tip) };
    TabResponse { selected, close: close_clicked, body }
}

/// The TabStrip's 30 px + button.
pub fn add_tab_button(ui: &mut Ui, skin: &Skin) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(5.0), skin.subtle_hover);
    }
    let c = pos2(rect.center().x.round() + 0.5, rect.center().y.round() + 0.5);
    let s = Stroke::new(1.0_f32, skin.text);
    ui.painter().line_segment([pos2(c.x - 5.0, c.y), pos2(c.x + 5.0, c.y)], s);
    ui.painter().line_segment([pos2(c.x, c.y - 5.0), pos2(c.x, c.y + 5.0)], s);
    resp
}

/// The ModernProgressRing indeterminate spinner.
pub fn progress_ring(ui: &mut Ui, skin: &Skin, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    let c = rect.center();
    let r = size / 2.0 - 1.0;
    let start = t * 5.0;
    let sweep = 1.2 + (t * 2.4).sin().abs() * 2.6;
    let pts: Vec<Pos2> = (0..=24)
        .map(|k| {
            let a = start + sweep * k as f32 / 24.0;
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    ui.painter().add(Shape::line(pts, Stroke::new(2.0_f32, skin.accent)));
    ui.ctx().request_repaint();
}

// ---- menus ---------------------------------------------------------------------------------

/// A ModernWinForms Menu flyout at `pos` (its top-left corner), `width` wide. Returns the
/// flyout's rectangle so the caller can tell clicks inside it from clicks outside.
pub fn menu_flyout(ctx: &egui::Context, skin: &Skin, id: Id, pos: Pos2, width: f32, add: impl FnOnce(&mut Ui)) -> Rect {
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(skin.menu_bg)
                .stroke(Stroke::new(1.0_f32, skin.menu_border))
                .rounding(Rounding::same(RADIUS_LARGE))
                .shadow(ctx.style().visuals.popup_shadow)
                .inner_margin(egui::Margin { left: 0.0, right: 0.0, top: 4.0, bottom: 5.0 })
                .show(ui, |ui| {
                    ui.set_width(width - 2.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    add(ui);
                });
        })
        .response
        .rect
}

pub struct MenuEntry<'a> {
    pub icon: Option<Ic>,
    pub label: &'a str,
    pub shortcut: &'a str,
    pub checked: bool,
    pub enabled: bool,
    pub submenu: bool,
}

impl<'a> MenuEntry<'a> {
    pub fn new(icon: Ic, label: &'a str) -> Self {
        Self { icon: Some(icon), label, shortcut: "", checked: false, enabled: true, submenu: false }
    }
    pub fn shortcut(mut self, s: &'a str) -> Self {
        self.shortcut = s;
        self
    }
    pub fn checked(mut self, c: bool) -> Self {
        self.checked = c;
        self
    }
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
    pub fn submenu(mut self) -> Self {
        self.submenu = true;
        self
    }
}

/// One 36 px menu row: check column, 16 px colour icon, 10 px, label, shortcut or › .
pub fn menu_item(ui: &mut Ui, skin: &Skin, e: MenuEntry, hot: bool) -> Response {
    let w = ui.available_width();
    let (outer, resp) = ui.allocate_exact_size(vec2(w, 36.0), if e.enabled { Sense::click() } else { Sense::hover() });
    let row = outer.shrink2(vec2(4.0, 0.0));
    let p = ui.painter();
    if e.enabled && (resp.hovered() || hot) {
        p.rect_filled(row, Rounding::same(RADIUS), skin.menu_hover);
    }
    let fg = if e.enabled { skin.text } else { skin.text.gamma_multiply(0.45) };
    let cy = row.center().y;
    if e.checked {
        let c = pos2(row.left() + 14.0, cy);
        p.add(Shape::line(vec![c + vec2(-4.5, 0.0), c + vec2(-1.5, 3.0), c + vec2(4.5, -3.5)], Stroke::new(1.2_f32, fg)));
    }
    let mut x = row.left() + 28.0 + 4.0;
    if let Some(ic) = e.icon {
        icons::paint(ui, skin, ic, pos2(x + 8.0, cy), 16.0, !e.enabled);
        x += 16.0 + 10.0;
    }
    p.text(pos2(x, cy), Align2::LEFT_CENTER, e.label, FontId::proportional(14.0), fg);
    if e.submenu {
        chevron_right(p, pos2(row.right() - 14.0, cy), fg);
    } else if !e.shortcut.is_empty() {
        p.text(pos2(row.right() - 12.0, cy), Align2::RIGHT_CENTER, e.shortcut, FontId::proportional(14.0), fg.gamma_multiply(0.55));
    }
    resp
}

pub fn menu_separator(ui: &mut Ui, skin: &Skin) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 5.0), Sense::hover());
    let y = r.center().y.round() + 0.5;
    ui.painter().line_segment([pos2(r.left(), y), pos2(r.right(), y)], Stroke::new(1.0_f32, skin.menu_border));
}

// ---- panels --------------------------------------------------------------------------------

/// A 12 px ModernWinForms splitter with its dot grip. Returns the drag delta along its axis.
pub fn splitter(ui: &mut Ui, skin: &Skin, id: Id, rect: Rect, vertical: bool) -> f32 {
    let resp = ui.interact(rect, id, Sense::drag());
    let p = ui.painter();
    if resp.dragged() {
        p.rect_filled(rect, Rounding::same(RADIUS), skin.split_active);
    } else if resp.hovered() {
        p.rect_filled(rect, Rounding::same(RADIUS), skin.subtle_hover);
    }
    // 2 × 3 dots (6 × 10) for a column splitter, 3 × 2 (10 × 6) for a row splitter.
    let c = pos2(rect.center().x.round(), rect.center().y.round());
    let (cols, rows) = if vertical { (2, 3) } else { (3, 2) };
    let origin = c - vec2((cols * 4 - 2) as f32 / 2.0, (rows * 4 - 2) as f32 / 2.0);
    for i in 0..cols {
        for j in 0..rows {
            p.rect_filled(Rect::from_min_size(origin + vec2(i as f32 * 4.0, j as f32 * 4.0), vec2(2.0, 2.0)), Rounding::ZERO, skin.grip);
        }
    }
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(if vertical { egui::CursorIcon::ResizeColumn } else { egui::CursorIcon::ResizeRow });
    }
    if vertical { resp.drag_delta().x } else { resp.drag_delta().y }
}

/// A transparent 24 px header button (Drawer header, block and figure toolbars).
pub fn tool_button(ui: &mut Ui, skin: &Skin, ic: Ic, size: f32, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    if resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS), skin.subtle_pressed);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS), skin.subtle_hover);
    }
    icons::paint(ui, skin, ic, rect.center(), size, false);
    resp.on_hover_text(tip)
}

/// The drawer header's collapse chevron (pointing left) and maximize button glyphs.
pub fn glyph_button(ui: &mut Ui, skin: &Skin, maximize: bool, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, Rounding::same(RADIUS), skin.subtle_hover);
    }
    let c = pos2(rect.center().x.round() + 0.5, rect.center().y.round() + 0.5);
    let s = Stroke::new(1.0_f32, skin.text_secondary);
    if maximize {
        p.rect_stroke(Rect::from_center_size(c, vec2(9.0, 9.0)), Rounding::same(1.5), s);
    } else {
        p.add(Shape::line(vec![pos2(c.x + 2.0, c.y - 4.0), pos2(c.x - 2.0, c.y), pos2(c.x + 2.0, c.y + 4.0)], s));
    }
    resp.on_hover_text(tip)
}

/// One ModernWinForms tree row (28 px): indent, 16 px expander, 16 px icon, name. The
/// selected row has the 3 × 16 accent pill.
#[allow(clippy::too_many_arguments)]
pub fn tree_row(ui: &mut Ui, skin: &Skin, id: Id, depth: usize, expandable: bool, open: bool, icon: Ic, name: &str, selected: bool) -> Response {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, Rounding::same(RADIUS), skin.row_selected);
        p.rect_filled(Rect::from_min_size(pos2(rect.left(), rect.center().y - 8.0), vec2(3.0, 16.0)), Rounding::same(1.5), skin.accent);
    } else if resp.hovered() {
        p.rect_filled(rect, Rounding::same(RADIUS), skin.row_hover);
    }
    let mut x = rect.left() + 4.0 + depth as f32 * 10.0;
    let cy = rect.center().y;
    if expandable {
        // An 8 px arrow that turns 90° when open, over 120 ms.
        let t = ui.ctx().animate_bool_with_time(id.with("open"), open, 0.12);
        let a = t * std::f32::consts::FRAC_PI_2;
        let c = pos2(x + 8.0, cy);
        let rot = |dx: f32, dy: f32| pos2(c.x + dx * a.cos() - dy * a.sin(), c.y + dx * a.sin() + dy * a.cos());
        p.add(Shape::line(vec![rot(-1.5, -3.5), rot(2.0, 0.0), rot(-1.5, 3.5)], Stroke::new(1.0_f32, skin.text_secondary)));
    }
    x += 16.0 + 4.0;
    icons::paint(ui, skin, icon, pos2(x + 8.0, cy), 16.0, false);
    x += 16.0 + 4.0 + 2.0;
    let clip = Rect::from_min_max(pos2(x, rect.top()), pos2(rect.right() - 8.0, rect.bottom()));
    let mut job = egui::text::LayoutJob::simple_singleline(name.to_string(), FontId::proportional(14.0), skin.text);
    job.wrap = egui::text::TextWrapping::truncate_at_width(clip.width().max(0.0));
    let galley = ui.fonts(|f| f.layout_job(job));
    p.galley(pos2(x, cy - galley.size().y / 2.0), galley, skin.text);
    resp
}

// ---- inputs --------------------------------------------------------------------------------

/// A ModernWinForms TextBox (`.mwf-field`): hairline border with a stronger bottom edge,
/// which turns into a 2 px accent line while focused.
#[allow(clippy::too_many_arguments)]
pub fn text_box(ui: &mut Ui, skin: &Skin, text: &mut String, id: Id, hint: &str, mono: bool, enabled: bool, size: egui::Vec2) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let focused = ui.memory(|m| m.has_focus(id));
    let hovered = ui.rect_contains_pointer(rect);
    let fill = if !enabled {
        skin.field_disabled
    } else if focused {
        skin.field_focus
    } else if hovered {
        skin.field_hover
    } else {
        skin.field_fill
    };
    let p = ui.painter().clone();
    p.rect_filled(rect, Rounding::same(RADIUS), fill);
    p.rect_stroke(rect.shrink(0.5), Rounding::same(RADIUS), Stroke::new(1.0_f32, skin.button_stroke));
    let font = if mono { FontId::monospace(13.0) } else { FontId::proportional(14.0) };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(8.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let resp = child
        .add_enabled_ui(enabled, |ui| {
            ui.visuals_mut().extreme_bg_color = Color32::TRANSPARENT;
            ui.add(
                egui::TextEdit::singleline(text)
                    .id(id)
                    .frame(false)
                    .margin(egui::Margin::ZERO)
                    .font(font)
                    .text_color(if enabled { skin.text } else { skin.text_disabled })
                    .hint_text(egui::RichText::new(hint).color(skin.text_tertiary))
                    .desired_width(f32::INFINITY),
            )
        })
        .inner;
    if enabled && focused {
        let y = rect.bottom();
        p.rect_filled(Rect::from_min_max(pos2(rect.left() + 1.0, y - 2.0), pos2(rect.right() - 1.0, y)), Rounding { nw: 0.0, ne: 0.0, sw: RADIUS, se: RADIUS }, skin.accent);
    } else {
        let y = rect.bottom() - 0.5;
        p.line_segment([pos2(rect.left() + RADIUS, y), pos2(rect.right() - RADIUS, y)], Stroke::new(1.0_f32, skin.field_stroke_bottom));
    }
    resp
}

/// ModernWinForms TrackBar: 4 px track, accent fill, a 20 px thumb with a 12 px accent dot,
/// and the value after it. Returns true when the value changed.
#[allow(clippy::too_many_arguments)]
pub fn track_bar(ui: &mut Ui, skin: &Skin, id: Id, value: &mut f64, min: f64, max: f64, step: f64, width: f32) -> bool {
    let label = livre_lang::display::fmt_g(*value, 6);
    let label_w = ui.fonts(|f| f.layout_no_wrap(label.clone(), FontId::proportional(14.0), skin.text).size().x).max(24.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width + 8.0 + label_w, 24.0), Sense::hover());
    let track = Rect::from_min_size(rect.min, vec2(width, 24.0));
    let resp = ui.interact(track, id, Sense::click_and_drag());
    let span = (max - min).max(f64::MIN_POSITIVE);
    let mut changed = false;
    if let (Some(p), true) = (resp.interact_pointer_pos(), resp.dragged() || resp.clicked()) {
        let t = ((p.x - track.left() - 10.0) / (track.width() - 20.0)).clamp(0.0, 1.0) as f64;
        let mut v = min + t * span;
        if step > 0.0 {
            v = min + ((v - min) / step).round() * step;
        }
        v = v.clamp(min, max);
        // Keep the literal short: 3.5000000000000004 → 3.5.
        v = (v * 1e9).round() / 1e9;
        if v != *value {
            *value = v;
            changed = true;
        }
    }
    let t = (((*value - min) / span).clamp(0.0, 1.0)) as f32;
    let p = ui.painter();
    let y = track.center().y;
    let x0 = track.left() + 10.0;
    let x1 = track.right() - 10.0;
    let tx = egui::lerp(x0..=x1, t);
    p.rect_filled(Rect::from_min_max(pos2(track.left(), y - 2.0), pos2(track.right(), y + 2.0)), Rounding::same(2.0), skin.slider_track);
    p.rect_filled(Rect::from_min_max(pos2(track.left(), y - 2.0), pos2(tx, y + 2.0)), Rounding::same(2.0), skin.accent);
    p.circle_filled(pos2(tx, y), 10.0, skin.slider_thumb);
    p.circle_stroke(pos2(tx, y), 9.5, Stroke::new(1.0_f32, skin.slider_thumb_border));
    let dot = if resp.is_pointer_button_down_on() { 5.0 } else if resp.hovered() { 7.0 } else { 6.0 };
    p.circle_filled(pos2(tx, y), dot, skin.accent);
    p.text(pos2(track.right() + 8.0, y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), skin.text);
    changed
}

/// ModernWinForms CheckBox: a 20 px box, accent-filled with a white check when on.
pub fn check_box(ui: &mut Ui, skin: &Skin, id: Id, on: &mut bool) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let p = ui.painter();
    if *on {
        let fill = if resp.hovered() { skin.accent_hover } else { skin.accent };
        p.rect_filled(rect, Rounding::same(RADIUS), fill);
        let c = rect.center();
        p.add(Shape::line(vec![c + vec2(-4.5, 0.2), c + vec2(-1.5, 3.2), c + vec2(4.5, -3.4)], Stroke::new(1.4_f32, skin.on_accent)));
    } else {
        p.rect_filled(rect, Rounding::same(RADIUS), if resp.hovered() { skin.subtle_hover } else { skin.check_fill });
        p.rect_stroke(rect.shrink(0.5), Rounding::same(RADIUS), Stroke::new(1.0_f32, skin.check_border));
    }
    resp.clicked()
}

/// ModernWinForms ComboBox: a field with a chevron that opens a drop-down list. Returns the
/// newly picked index.
pub fn combo_box(ui: &mut Ui, skin: &Skin, id: Id, labels: &[String], selected: usize, size: egui::Vec2) -> Option<usize> {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let resp = ui.interact(rect, id, Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(RADIUS), if resp.hovered() { skin.field_hover } else { skin.field_fill });
    elevation_stroke(p, rect, skin.button_stroke, skin.button_stroke_bottom);
    let text = labels.get(selected).cloned().unwrap_or_default();
    let clip = p.with_clip_rect(rect.shrink2(vec2(10.0, 0.0)).with_max_x(rect.right() - 28.0));
    clip.text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, text, FontId::proportional(14.0), skin.text);
    chevron_down(p, pos2(rect.right() - 14.0, rect.center().y + 0.5), skin.text_secondary);
    let popup = id.with("list");
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup));
    }
    let mut picked = None;
    // egui's popup closes on a click elsewhere and on Escape.
    egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let w = rect.width().max(96.0) - 8.0;
        for (i, label) in labels.iter().enumerate() {
            let (r, item) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
            if item.hovered() {
                ui.painter().rect_filled(r, Rounding::same(RADIUS), skin.menu_hover);
            }
            if i == selected {
                ui.painter().rect_filled(Rect::from_min_size(pos2(r.left() + 3.0, r.center().y - 7.0), vec2(3.0, 14.0)), Rounding::same(2.0), skin.accent);
            }
            ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), skin.text);
            if item.clicked() {
                picked = Some(i);
            }
        }
    });
    picked.filter(|&i| i != selected)
}

/// A standard (elevated) ModernButton, used for the live Button control.
pub fn standard_button(ui: &mut Ui, skin: &Skin, id: Id, text: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(14.0), skin.text);
    let size = vec2((galley.size().x + 32.0).max(64.0), 28.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let resp = ui.interact(rect, id, Sense::click());
    let p = ui.painter();
    let pressed = resp.is_pointer_button_down_on();
    let fill = if pressed { skin.button_pressed } else if resp.hovered() { skin.button_hover } else { skin.button_fill };
    p.rect_filled(rect, Rounding::same(RADIUS), fill);
    elevation_stroke(p, rect, skin.button_stroke, if pressed { skin.button_stroke } else { skin.button_stroke_bottom });
    p.galley(rect.center() - galley.size() / 2.0, galley, if pressed { skin.text_secondary } else { skin.text });
    resp
}

/// ModernWinForms InfoBar with error severity: tinted card, error glyph, bold title, message.
pub fn error_bar(ui: &mut Ui, skin: &Skin, title: &str, message: &str) {
    egui::Frame::none()
        .fill(skin.error_bg)
        .rounding(Rounding::same(RADIUS))
        .stroke(Stroke::new(1.0_f32, skin.separator))
        .inner_margin(egui::Margin { left: 16.0, right: 16.0, top: 14.0, bottom: 14.0 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 20.0), Sense::hover());
                error_glyph(ui.painter(), skin, pos2(r.center().x, r.top() + 10.0));
                let mut job = egui::text::LayoutJob::default();
                job.append(title, 0.0, egui::TextFormat { font_id: semibold(14.0), color: skin.text, ..Default::default() });
                job.append(&format!("  {message}"), 0.0, egui::TextFormat { font_id: FontId::proportional(14.0), color: skin.text, ..Default::default() });
                job.wrap.max_width = ui.available_width();
                ui.add(egui::Label::new(job).selectable(true));
            });
        });
}

/// The InfoBar error glyph: a filled circle with a cross.
pub fn error_glyph(p: &egui::Painter, skin: &Skin, c: Pos2) {
    p.circle_filled(c, 8.0, skin.error);
    let fg = if skin.dark { Color32::BLACK } else { Color32::WHITE };
    let s = Stroke::new(1.3_f32, fg);
    p.line_segment([c + vec2(-3.0, -3.0), c + vec2(3.0, 3.0)], s);
    p.line_segment([c + vec2(3.0, -3.0), c + vec2(-3.0, 3.0)], s);
}
