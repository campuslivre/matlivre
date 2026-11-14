//! The variable viewer: workspace variables opened over the document, each shown as a grid
//! of its elements (like MATLAB's Variables editor, read-only) or as `disp` text.

use eframe::egui::text::LayoutJob;
use eframe::egui::{self, pos2, vec2, Align, Color32, FontId, Id, Layout, Rect, RichText, Rounding, Sense, Stroke, Ui, UiBuilder};
use livre_lang::{VarBody, VarView};

use crate::i18n::{tr, trf};
use crate::theme::{semibold, Skin};

const ROW_H: f32 = 26.0;
const STRIP_H: f32 = 38.0;

/// What the strip of open variables asks for.
pub enum StripAction {
    ShowDocument,
    Show(usize),
    Close(usize),
}

/// The variables open in the viewer, in the order they were opened.
#[derive(Default)]
pub struct Viewer {
    open: Vec<VarView>,
    /// The variable on show in place of the document.
    active: Option<usize>,
    /// A variable the user asked for, shown as soon as the engine sends it.
    wanted: Option<String>,
}

impl Viewer {
    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    pub fn active(&self) -> Option<&VarView> {
        self.active.and_then(|i| self.open.get(i))
    }

    /// The open variables, to ask the engine for their new contents after a run.
    pub fn names(&self) -> Vec<String> {
        self.open.iter().map(|v| v.name.clone()).collect()
    }

    /// The user opened `name`: it shows at once when already open (and its contents are
    /// asked for again), else when they arrive.
    pub fn ask(&mut self, name: &str) {
        match self.open.iter().position(|v| v.name == name) {
            Some(i) => self.active = Some(i),
            None => self.wanted = Some(name.to_string()),
        }
    }

    /// Contents from the engine: `None` when the variable is gone, which closes it.
    pub fn receive(&mut self, name: &str, view: Option<VarView>) {
        let at = self.open.iter().position(|v| v.name == name);
        let wanted = self.wanted.as_deref() == Some(name);
        if wanted {
            self.wanted = None;
        }
        match (view, at) {
            (Some(v), Some(i)) => self.open[i] = v,
            (Some(v), None) if wanted => {
                self.open.push(v);
                self.active = Some(self.open.len() - 1);
            }
            // Closed while the answer was on its way.
            (Some(_), None) => {}
            (None, Some(i)) => self.close(i),
            (None, None) => {}
        }
    }

    pub fn close(&mut self, i: usize) {
        if i >= self.open.len() {
            return;
        }
        self.open.remove(i);
        self.active = match self.active {
            Some(a) if a == i => None,
            Some(a) if a > i => Some(a - 1),
            a => a,
        };
    }

    pub fn apply(&mut self, action: StripAction) {
        match action {
            StripAction::ShowDocument => self.active = None,
            StripAction::Show(i) => self.active = Some(i).filter(|i| *i < self.open.len()),
            StripAction::Close(i) => self.close(i),
        }
    }

    /// The strip over the document: the document's own chip, then one per open variable.
    pub fn strip(&self, ui: &mut Ui, skin: &Skin, document: &str) -> Option<StripAction> {
        let mut action = None;
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), STRIP_H), Sense::hover());
        let mut inner = ui.new_child(UiBuilder::new().max_rect(row.shrink2(vec2(8.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
        inner.set_clip_rect(row.intersect(ui.clip_rect()));
        inner.spacing_mut().item_spacing.x = 4.0;
        if chip(&mut inner, skin, Id::new("var-chip-doc"), document, self.active.is_none(), false).0 {
            action = Some(StripAction::ShowDocument);
        }
        for (i, v) in self.open.iter().enumerate() {
            let (clicked, closed) = chip(&mut inner, skin, Id::new(("var-chip", &v.name)), &v.name, self.active == Some(i), true);
            if closed {
                action = Some(StripAction::Close(i));
            } else if clicked {
                action = Some(StripAction::Show(i));
            }
        }
        ui.painter().line_segment([pos2(row.left(), row.bottom() - 0.5), pos2(row.right(), row.bottom() - 0.5)], Stroke::new(1.0_f32, skin.separator));
        action
    }

    /// The variable on show, filling the rest of `ui`.
    pub fn show(&self, ui: &mut Ui, skin: &Skin) {
        let Some(v) = self.active() else {
            return;
        };
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
        let mut h = ui.new_child(UiBuilder::new().max_rect(head.shrink2(vec2(16.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
        h.spacing_mut().item_spacing.x = 8.0;
        h.label(RichText::new(&v.name).font(semibold(14.0)).color(skin.text));
        h.label(RichText::new(format!("{} {}", v.size, v.class)).size(12.0).color(skin.text_secondary));
        if let (true, Some((r, c))) = (v.truncated, v.body.shape()) {
            h.label(RichText::new(trf("·  Showing the first {} rows and {} columns", &[&r, &c])).size(12.0).color(skin.text_tertiary));
        }
        let rest = ui.available_rect_before_wrap();
        let mut body = ui.new_child(UiBuilder::new().max_rect(rest).layout(Layout::top_down(Align::Min)));
        body.set_clip_rect(rest.intersect(ui.clip_rect()));
        body.push_id(("var-view", &v.name), |ui| match &v.body {
            VarBody::Text(t) => text_view(ui, skin, t),
            grid_body => grid(ui, skin, grid_body),
        });
    }
}

/// A 26 px chip of the strip; returns (clicked, close clicked).
fn chip(ui: &mut Ui, skin: &Skin, id: Id, title: &str, active: bool, closable: bool) -> (bool, bool) {
    let font = FontId::proportional(13.0);
    let mut job = LayoutJob::simple_singleline(title.to_string(), font, skin.text);
    job.wrap = egui::text::TextWrapping::truncate_at_width(200.0);
    let galley = ui.fonts(|f| f.layout_job(job));
    let width = 10.0 + galley.size().x + if closable { 4.0 + 18.0 + 4.0 } else { 10.0 };
    let (slot, _) = ui.allocate_exact_size(vec2(width, 26.0), Sense::hover());
    let resp = ui.interact(slot, id, Sense::click());
    let close_rect = Rect::from_center_size(pos2(slot.right() - 4.0 - 9.0, slot.center().y), vec2(18.0, 18.0));
    let close = closable.then(|| ui.interact(close_rect, id.with("close"), Sense::click()));
    let close_hot = close.as_ref().is_some_and(|c| c.hovered());
    let p = ui.painter();
    if active {
        p.rect_filled(slot, Rounding::same(5.0), skin.tab_selected);
        p.rect_stroke(slot.shrink(0.5), Rounding::same(5.0), Stroke::new(1.0_f32, skin.tab_selected_border));
    } else if resp.hovered() || close_hot {
        p.rect_filled(slot, Rounding::same(5.0), skin.subtle_hover);
    }
    p.galley(pos2(slot.left() + 10.0, slot.center().y - galley.size().y / 2.0), galley, skin.text);
    let mut closed = false;
    if let Some(close) = close {
        let fg = if close_hot {
            p.rect_filled(close_rect, Rounding::same(4.0), skin.subtle_hover);
            skin.text
        } else {
            skin.text_tertiary
        };
        let c = close_rect.center();
        let s = Stroke::new(1.0_f32, fg);
        p.line_segment([pos2(c.x - 3.5, c.y - 3.5), pos2(c.x + 3.5, c.y + 3.5)], s);
        p.line_segment([pos2(c.x + 3.5, c.y - 3.5), pos2(c.x - 3.5, c.y + 3.5)], s);
        closed = close.clicked() || resp.clicked_by(egui::PointerButton::Middle);
        close.on_hover_text(tr("Close"));
    }
    (resp.clicked() && !closed, closed)
}

fn scroll_both(ui: &mut Ui, skin: &Skin) -> egui::ScrollArea {
    crate::app::style_scroll_bar(ui, skin);
    egui::ScrollArea::both().auto_shrink([false, false])
}

fn text_view(ui: &mut Ui, skin: &Skin, text: &str) {
    scroll_both(ui, skin).show(ui, |ui| {
        egui::Frame::none().inner_margin(egui::Margin { left: 16.0, right: 16.0, top: 4.0, bottom: 16.0 }).show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(text).font(FontId::monospace(13.0)).color(skin.text)).extend());
        });
    });
}

/// The widest cell text, in characters, among the first rows and columns.
fn sample_chars(body: &VarBody, rows: usize, cols: usize) -> usize {
    let mut most = 1;
    for c in 0..cols.min(24) {
        for r in 0..rows.min(64) {
            most = most.max(body.cell(r, c).chars().count());
        }
    }
    most
}

/// The rows or columns (of size `step`, `count` of them) a viewport span touches.
pub fn visible(from: f32, to: f32, step: f32, count: usize) -> std::ops::Range<usize> {
    let first = ((from / step).floor().max(0.0) as usize).min(count);
    let last = ((to / step).ceil().max(0.0) as usize).min(count);
    first..last.max(first)
}

/// A grid of the body's cells under numbered headers that stay in place while it scrolls.
/// Only the cells in view are laid out, so the size of the array does not matter.
fn grid(ui: &mut Ui, skin: &Skin, body: &VarBody) {
    let Some((rows, cols)) = body.shape() else {
        return;
    };
    let row_names = match body {
        VarBody::Cells { row_names, .. } => row_names.as_ref(),
        _ => None,
    };
    let numeric = body.is_numeric();
    let mono = FontId::monospace(13.0);
    let head_font = FontId::monospace(12.0);
    let char_w = ui.fonts(|f| f.glyph_width(&mono, '0'));
    let col_w = (sample_chars(body, rows, cols) as f32 * char_w + 24.0).clamp(if numeric { 64.0 } else { 88.0 }, 360.0);
    let gutter = match row_names {
        Some(names) => (names.iter().map(|n| n.chars().count()).max().unwrap_or(1) as f32 * char_w + 24.0).clamp(64.0, 240.0),
        None => (rows.max(1).to_string().len() as f32 * char_w + 24.0).max(44.0),
    };
    let total = vec2(gutter + cols as f32 * col_w, ROW_H + rows as f32 * ROW_H);
    let outer_clip = ui.clip_rect();
    scroll_both(ui, skin).show_viewport(ui, |ui, viewport| {
        let (area, resp) = ui.allocate_exact_size(total, Sense::hover());
        let origin = area.min;
        // The corner of the part in view, where the headers stay.
        let pin = origin + viewport.min.to_vec2();
        let view = Rect::from_min_size(pin, viewport.size()).intersect(outer_clip);
        let cells_clip = Rect::from_min_max(pos2(pin.x + gutter, pin.y + ROW_H), view.max);
        let rs = visible(viewport.top(), viewport.bottom() - ROW_H, ROW_H, rows);
        let cs = visible(viewport.left(), viewport.right() - gutter, col_w, cols);
        let cell_rect = |r: usize, c: usize| Rect::from_min_size(pos2(origin.x + gutter + c as f32 * col_w, origin.y + ROW_H + r as f32 * ROW_H), vec2(col_w, ROW_H));
        let hover = resp.hover_pos().filter(|p| cells_clip.contains(*p)).and_then(|p| {
            let (r, c) = (((p.y - origin.y - ROW_H) / ROW_H) as usize, ((p.x - origin.x - gutter) / col_w) as usize);
            (r < rows && c < cols).then_some((r, c))
        });
        let line = Stroke::new(1.0_f32, skin.separator);
        let layout = |ui: &Ui, text: String, font: &FontId, color: Color32, width: f32| {
            let mut job = LayoutJob::simple_singleline(text, font.clone(), color);
            job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(0.0));
            ui.fonts(|f| f.layout_job(job))
        };

        let p = ui.painter().with_clip_rect(cells_clip);
        if let Some((r, c)) = hover {
            p.rect_filled(cell_rect(r, c), Rounding::ZERO, skin.row_hover);
        }
        let mut elided = None;
        for r in rs.clone() {
            for c in cs.clone() {
                let rect = cell_rect(r, c);
                let text = body.cell(r, c);
                let g = layout(ui, text.clone(), &mono, skin.text, col_w - 16.0);
                if g.elided && hover == Some((r, c)) {
                    elided = Some(text);
                }
                let x = if numeric { rect.right() - 8.0 - g.size().x } else { rect.left() + 8.0 };
                p.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, skin.text);
            }
        }
        let right = (origin.x + total.x).min(view.right());
        let bottom = (origin.y + total.y).min(view.bottom());
        for r in rs.clone() {
            let y = cell_rect(r, 0).bottom() - 0.5;
            p.line_segment([pos2(cells_clip.left(), y), pos2(right, y)], line);
        }
        for c in cs.clone() {
            let x = cell_rect(0, c).right() - 0.5;
            p.line_segment([pos2(x, cells_clip.top()), pos2(x, bottom)], line);
        }

        // Column headers, over the cells scrolled under them.
        let head = Rect::from_min_max(pin, pos2(right, pin.y + ROW_H));
        let p = ui.painter().with_clip_rect(head.intersect(outer_clip));
        p.rect_filled(head, Rounding::ZERO, skin.surface);
        let p = ui.painter().with_clip_rect(Rect::from_min_max(pos2(pin.x + gutter, pin.y), head.max).intersect(outer_clip));
        for c in cs {
            let rect = Rect::from_min_size(pos2(cell_rect(0, c).left(), pin.y), vec2(col_w, ROW_H));
            let (label, font) = if row_names.is_some() { (tr("Value").to_string(), FontId::proportional(12.0)) } else { ((c + 1).to_string(), head_font.clone()) };
            let color = if hover.is_some_and(|h| h.1 == c) { skin.text } else { skin.text_secondary };
            let g = layout(ui, label, &font, color, col_w - 16.0);
            let x = if numeric { rect.right() - 8.0 - g.size().x } else { rect.left() + 8.0 };
            p.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, color);
            p.line_segment([pos2(rect.right() - 0.5, rect.top()), pos2(rect.right() - 0.5, rect.bottom())], line);
        }

        // Row headers.
        let side = Rect::from_min_max(pos2(pin.x, pin.y + ROW_H), pos2(pin.x + gutter, bottom));
        let p = ui.painter().with_clip_rect(side.intersect(outer_clip));
        p.rect_filled(side, Rounding::ZERO, skin.surface);
        for r in rs {
            let rect = Rect::from_min_size(pos2(pin.x, cell_rect(r, 0).top()), vec2(gutter, ROW_H));
            let label = row_names.map_or_else(|| (r + 1).to_string(), |n| n[r].clone());
            let color = if hover.is_some_and(|h| h.0 == r) { skin.text } else { skin.text_secondary };
            let g = layout(ui, label, &head_font, color, gutter - 16.0);
            let x = if row_names.is_some() { rect.left() + 8.0 } else { rect.right() - 8.0 - g.size().x };
            p.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, color);
            p.line_segment([pos2(rect.left(), rect.bottom() - 0.5), pos2(rect.right(), rect.bottom() - 0.5)], line);
        }

        // The corner, and the lines that set the headers apart.
        let corner = Rect::from_min_size(pin, vec2(gutter, ROW_H));
        let p = ui.painter().with_clip_rect(view);
        p.rect_filled(corner, Rounding::ZERO, skin.surface);
        if row_names.is_some() {
            p.text(pos2(corner.left() + 8.0, corner.center().y), egui::Align2::LEFT_CENTER, tr("Name"), FontId::proportional(12.0), skin.text_secondary);
        }
        let edge = Stroke::new(1.0_f32, skin.border);
        p.line_segment([pos2(pin.x, pin.y + ROW_H - 0.5), pos2(right, pin.y + ROW_H - 0.5)], edge);
        p.line_segment([pos2(pin.x + gutter - 0.5, pin.y), pos2(pin.x + gutter - 0.5, bottom)], edge);

        if let Some(text) = elided {
            resp.on_hover_text_at_pointer(RichText::new(text).font(mono));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{visible, StripAction, Viewer};
    use livre_lang::{Interpreter, VarView};

    fn views(src: &str, names: &[&str]) -> Vec<VarView> {
        let mut it = Interpreter::new();
        it.run_source(src).unwrap();
        names.iter().map(|n| it.inspect(n).unwrap()).collect()
    }

    fn open(v: &Viewer) -> (Vec<String>, Option<String>) {
        (v.names(), v.active().map(|a| a.name.clone()))
    }

    #[test]
    fn a_variable_opens_when_its_contents_arrive() {
        let [a, b]: [VarView; 2] = views("A = magic(3);\nb = 1:20;", &["A", "b"]).try_into().unwrap();
        let mut v = Viewer::default();
        assert!(v.is_empty() && v.active().is_none());
        v.ask("A");
        assert!(v.is_empty());
        v.receive("A", Some(a.clone()));
        assert_eq!(open(&v), (vec!["A".to_string()], Some("A".to_string())));
        v.ask("b");
        v.receive("b", Some(b.clone()));
        assert_eq!(open(&v), (vec!["A".to_string(), "b".to_string()], Some("b".to_string())));
        // Asking for one already open shows it at once.
        v.ask("A");
        assert_eq!(open(&v).1, Some("A".to_string()));
        // Contents nobody asked for (closed meanwhile) are dropped.
        let mut w = Viewer::default();
        w.receive("b", Some(b));
        assert!(w.is_empty());
    }

    #[test]
    fn a_refresh_keeps_the_variable_on_show_and_a_cleared_one_closes() {
        let all = views("A = magic(3);\nb = 1:20;\nc = {1, 2};", &["A", "b", "c"]);
        let mut v = Viewer::default();
        for x in &all {
            v.ask(&x.name);
            v.receive(&x.name, Some(x.clone()));
        }
        v.apply(StripAction::Show(1));
        let newer = views("b = [1 2; 3 4];", &["b"]).remove(0);
        v.receive("b", Some(newer));
        assert_eq!(open(&v).1, Some("b".to_string()));
        assert_eq!(v.active().unwrap().size, "2×2");
        // One before the active variable goes away: the active one stays on show.
        v.receive("A", None);
        assert_eq!(open(&v), (vec!["b".to_string(), "c".to_string()], Some("b".to_string())));
        // The active one goes away: back to the document.
        v.receive("b", None);
        assert_eq!(open(&v), (vec!["c".to_string()], None));
        v.apply(StripAction::Show(0));
        v.apply(StripAction::ShowDocument);
        assert_eq!(open(&v), (vec!["c".to_string()], None));
        v.apply(StripAction::Close(0));
        assert!(v.is_empty());
    }

    #[test]
    fn only_the_rows_in_view_are_laid_out() {
        assert_eq!(visible(0.0, 260.0, 26.0, 1000), 0..10);
        assert_eq!(visible(13.0, 273.0, 26.0, 1000), 0..11);
        assert_eq!(visible(26_000.0 - 260.0, 26_000.0, 26.0, 1000), 990..1000);
        assert_eq!(visible(0.0, 500.0, 26.0, 3), 0..3);
        assert_eq!(visible(0.0, -26.0, 26.0, 3), 0..0);
        assert_eq!(visible(0.0, 100.0, 26.0, 0), 0..0);
    }
}
