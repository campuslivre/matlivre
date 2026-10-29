use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, FontId, Key, KeyboardShortcut, Layout, Margin, Modifiers, PopupCloseBehavior, Pos2, Rect,
    RichText, Rounding, Sense, Stroke, TextStyle,
};
use livre_doc::{BlockKind, Document};
use livre_lang::{Figure, OutputKind, VarInfo};

use crate::engine::{Command, Engine, Event};
use crate::os;
use crate::theme::{Skin, RADIUS, RADIUS_LARGE, TITLE_BAR_HEIGHT};
use crate::transfer;
use crate::widgets::{self, ButtonStyle, Caption, Icon};
use crate::Launch;

const WELCOME: &str = include_str!("../../../examples/welcome.m");

enum UiOutput {
    Text(String),
    Value(String),
    Figure(Figure),
    Warning(String),
    Error { message: String, line: usize },
}

enum Action {
    Insert { at: usize, kind: BlockKind },
    Delete(usize),
    Move { from: usize, up: bool },
    Focus(usize),
    RunSection(usize),
}

/// One open document.
struct Tab {
    id: u64,
    doc: Document,
    path: Option<PathBuf>,
    /// The name while the document has no file: untitled.mlx, untitled2.mlx, ... as in MATLAB.
    name: String,
    dirty: bool,
    outputs: HashMap<u64, Vec<UiOutput>>,
    current: usize,
    focus_block: Option<u64>,
}

impl Tab {
    fn title(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            None => self.name.clone(),
        }
    }

    /// An untouched untitled script, which opening a file replaces instead of keeping.
    fn is_pristine(&self) -> bool {
        self.path.is_none() && !self.dirty && self.doc.blocks.iter().all(|b| b.text.trim().is_empty())
    }
}

/// A tab being dragged by its header.
struct TabDrag {
    id: u64,
    /// The pointer is away from the tab row, so letting go moves the tab to another window.
    torn: bool,
}

pub struct App {
    skin: Skin,
    tabs: Vec<Tab>,
    active: usize,
    next_tab: u64,
    engine: Engine,
    running: bool,
    /// The tab whose code is running; its output goes there even while another tab is shown.
    running_tab: Option<u64>,
    running_block: Option<u64>,
    workspace: Vec<VarInfo>,
    show_workspace: bool,
    output_right: bool,
    last_run: Option<(bool, Duration)>,
    notice: Option<(String, bool)>,
    drag: Option<TabDrag>,
    window: Option<os::Window>,
    /// Tabs dropped here from other instances.
    incoming: Option<Receiver<PathBuf>>,
    /// Where the tab headers are, to tell a reorder from a tear-off.
    tab_row: Rect,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, launch: Launch) -> Self {
        crate::theme::install_fonts(&cc.egui_ctx);
        let skin = Skin::light();
        skin.apply(&cc.egui_ctx);
        let window = os::Window::from_handle(cc);
        let incoming = transfer::listen(cc.egui_ctx.clone()).map(|(port, rx)| {
            if let Some(w) = &window {
                w.advertise(port);
            }
            rx
        });
        let mut app = Self {
            skin,
            tabs: Vec::new(),
            active: 0,
            next_tab: 1,
            engine: Engine::start(cc.egui_ctx.clone()),
            running: false,
            running_tab: None,
            running_block: None,
            workspace: Vec::new(),
            show_workspace: true,
            output_right: false,
            last_run: None,
            notice: None,
            drag: None,
            window,
            incoming,
            tab_row: Rect::NOTHING,
        };
        match launch {
            Launch::Welcome => app.getting_started(),
            Launch::Open(p) => app.open_path(p),
            Launch::Tab(m) => app.receive(&m),
        }
        if app.tabs.is_empty() {
            app.new_doc();
        }
        app
    }

    fn set_dark(&mut self, ctx: &egui::Context, dark: bool) {
        self.skin = if dark { Skin::dark() } else { Skin::light() };
        self.skin.apply(ctx);
    }

    // ---- tabs --------------------------------------------------------------------------

    fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    fn add_tab(&mut self, doc: Document, path: Option<PathBuf>, name: String, dirty: bool) {
        let id = self.next_tab;
        self.next_tab += 1;
        self.tabs.push(Tab { id, doc, path, name, dirty, outputs: HashMap::new(), current: 0, focus_block: None });
        self.active = self.tabs.len() - 1;
    }

    /// The first of untitled.mlx, untitled2.mlx, ... not already shown in a tab.
    fn untitled_name(&self) -> String {
        (1..)
            .map(|n| if n == 1 { "untitled.mlx".to_string() } else { format!("untitled{n}.mlx") })
            .find(|name| !self.tabs.iter().any(|t| t.path.is_none() && &t.name == name))
            .unwrap()
    }

    /// Asks to save a tab with unsaved changes. False if the user cancelled.
    fn confirm_discard(&mut self, i: usize) -> bool {
        if !self.tabs[i].dirty {
            return true;
        }
        let answer = rfd::MessageDialog::new()
            .set_title("matlivre")
            .set_level(rfd::MessageLevel::Warning)
            .set_description(format!("Save changes to {}?", self.tabs[i].title()))
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show();
        match answer {
            rfd::MessageDialogResult::Yes => {
                self.active = i;
                self.save(false)
            }
            rfd::MessageDialogResult::No => true,
            _ => false,
        }
    }

    fn close_tab(&mut self, ctx: &egui::Context, i: usize) {
        if self.confirm_discard(i) {
            self.remove_tab(ctx, i);
        }
    }

    /// Drops a tab without asking; the window closes with its last tab.
    fn remove_tab(&mut self, ctx: &egui::Context, i: usize) {
        let tab = self.tabs.remove(i);
        if self.running_tab == Some(tab.id) {
            self.engine.stop();
            self.running_tab = None;
        }
        if self.tabs.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.active > i || self.active == self.tabs.len() {
            self.active -= 1;
        }
    }

    /// Opens a tab handed over by another window.
    fn receive(&mut self, manifest: &Path) {
        match transfer::read(manifest) {
            Ok(p) => {
                let name = if self.tabs.iter().any(|t| t.path.is_none() && t.name == p.name) { self.untitled_name() } else { p.name };
                self.add_tab(p.doc, p.path, name, p.dirty);
            }
            Err(e) => self.notice = Some((format!("Could not take the tab: {e}"), true)),
        }
    }

    /// Where a dragged tab was let go, in screen points, and the window under it.
    fn drop_target(&self, ctx: &egui::Context) -> (Option<Pos2>, os::Under) {
        let ppp = ctx.pixels_per_point();
        match os::cursor_pos() {
            Some(c) => {
                let under = self.window.map_or(os::Under::Other, |w| w.under_cursor(c));
                (Some(pos2(c.0 as f32 / ppp, c.1 as f32 / ppp)), under)
            }
            None => {
                let inner = ctx.input(|i| i.viewport().inner_rect);
                let local = ctx.input(|i| i.pointer.latest_pos());
                let screen = inner.zip(local).map(|(r, p)| p + r.min.to_vec2());
                let inside = local.is_some_and(|p| ctx.screen_rect().contains(p));
                (screen, if inside { os::Under::Own } else { os::Under::Other })
            }
        }
    }

    /// The pointer relative to the window, also while it is outside it.
    fn pointer_local(&self, ctx: &egui::Context) -> Option<Pos2> {
        let ppp = ctx.pixels_per_point();
        match (os::cursor_pos(), ctx.input(|i| i.viewport().inner_rect)) {
            (Some(c), Some(inner)) => Some(pos2(c.0 as f32 / ppp, c.1 as f32 / ppp) - inner.min.to_vec2()),
            _ => ctx.input(|i| i.pointer.latest_pos()),
        }
    }

    /// Follows a tab drag: reorders within the row, and on release away from it moves the
    /// tab to the matlivre window under the pointer or to a new one.
    fn drag_tabs(&mut self, ctx: &egui::Context, rects: &[Rect]) {
        let Some(drag) = &self.drag else { return };
        let id = drag.id;
        let Some(from) = self.tabs.iter().position(|t| t.id == id) else {
            self.drag = None;
            return;
        };
        ctx.request_repaint();
        let pointer = self.pointer_local(ctx);
        let torn = pointer.is_some_and(|p| !self.tab_row.expand2(vec2(40.0, 28.0)).contains(p));
        ctx.set_cursor_icon(if torn { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        if let (Some(p), false) = (pointer, torn) {
            // Past the middle of a neighbour: take its place.
            let active_id = self.tab().id;
            let to = if from + 1 < rects.len() && p.x > rects[from + 1].center().x {
                Some(from + 1)
            } else if from > 0 && p.x < rects[from - 1].center().x {
                Some(from - 1)
            } else {
                None
            };
            if let Some(to) = to {
                let t = self.tabs.remove(from);
                self.tabs.insert(to, t);
                self.active = self.tabs.iter().position(|t| t.id == active_id).unwrap_or(0);
            }
        }
        if let Some(d) = &mut self.drag {
            d.torn = torn;
        }
        // egui loses the pointer once it leaves the window, so ask the system about the button.
        let down = os::left_button_down().unwrap_or_else(|| ctx.input(|i| i.pointer.primary_down()));
        if !down {
            let torn = self.drag.take().is_some_and(|d| d.torn);
            if torn {
                let i = self.tabs.iter().position(|t| t.id == id).unwrap_or(from);
                self.tear_off(ctx, i);
            }
        }
    }

    fn tear_off(&mut self, ctx: &egui::Context, i: usize) {
        let (at, under) = self.drop_target(ctx);
        // The window's top-left corner, so the pointer ends up over the tab.
        let at = at.map(|p| p - vec2(80.0, 20.0));
        if matches!(under, os::Under::Own) {
            return;
        }
        if self.running_tab == Some(self.tabs[i].id) {
            self.notice = Some(("A tab cannot move to another window while its code is running.".into(), false));
            return;
        }
        // The only tab, let go over nothing: moving it to a new window is moving this one.
        if self.tabs.len() == 1 && matches!(under, os::Under::Other) {
            if let Some(p) = at {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(p));
            }
            return;
        }
        let tab = &self.tabs[i];
        let manifest = match transfer::write(&tab.doc, tab.path.as_deref(), tab.dirty, &tab.name) {
            Ok(m) => m,
            Err(e) => {
                self.notice = Some((format!("Could not move the tab: {e}"), true));
                return;
            }
        };
        let moved = match under {
            os::Under::Instance(port) => transfer::send(port, &manifest),
            _ => transfer::spawn_window(&manifest, at),
        };
        if moved {
            self.remove_tab(ctx, i);
        } else {
            self.notice = Some(("Could not move the tab to the other window.".into(), true));
        }
    }

    // ---- files -------------------------------------------------------------------------

    fn open_path(&mut self, p: PathBuf) {
        if let Some(i) = self.tabs.iter().position(|t| t.path.as_deref() == Some(p.as_path())) {
            self.active = i;
            return;
        }
        match Document::load(&p) {
            Ok(doc) => {
                let reuse = self.tabs.get(self.active).is_some_and(|t| t.is_pristine() && self.running_tab != Some(t.id));
                if reuse {
                    let t = self.tab_mut();
                    t.doc = doc;
                    t.path = Some(p);
                    t.outputs.clear();
                    t.current = 0;
                } else {
                    self.add_tab(doc, Some(p), String::new(), false);
                }
                self.notice = None;
            }
            Err(e) => self.notice = Some((format!("Could not open {}: {e}", p.display()), true)),
        }
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Live Scripts and scripts", &["mlx", "m"])
            .add_filter("Live Script", &["mlx"])
            .add_filter("Script", &["m"])
            .pick_file()
        {
            self.open_path(p);
        }
    }

    /// Saves the active tab. False if it was not saved (cancelled or failed).
    fn save(&mut self, save_as: bool) -> bool {
        let path = match (&self.tab().path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new()
                .add_filter("Live Script", &["mlx"])
                .add_filter("Script", &["m"])
                .set_file_name(self.tab().title())
                .save_file(),
        };
        let Some(p) = path else { return false };
        match self.tab().doc.save(&p) {
            Ok(()) => {
                let t = self.tab_mut();
                t.path = Some(p);
                t.dirty = false;
                true
            }
            Err(e) => {
                self.notice = Some((format!("Could not save: {e}"), true));
                false
            }
        }
    }

    fn new_doc(&mut self) {
        let name = self.untitled_name();
        self.add_tab(Document::new_script(), None, name, false);
        let t = self.tab_mut();
        t.focus_block = t.doc.blocks.first().map(|b| b.id);
    }

    fn getting_started(&mut self) {
        let name = self.untitled_name();
        self.add_tab(livre_doc::read_m(WELCOME), None, name, false);
    }

    // ---- running -----------------------------------------------------------------------

    fn run_blocks(&mut self, range: std::ops::Range<usize>) {
        if self.running {
            return;
        }
        let tab = &mut self.tabs[self.active];
        let blocks: Vec<(u64, String)> =
            tab.doc.blocks[range].iter().filter(|b| b.kind == BlockKind::Code).map(|b| (b.id, b.text.clone())).collect();
        if blocks.is_empty() {
            return;
        }
        for (id, _) in &blocks {
            tab.outputs.remove(id);
        }
        let all_code = tab.doc.blocks.iter().filter(|b| b.kind == BlockKind::Code).map(|b| b.text.clone()).collect();
        let folder = tab.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()));
        self.running = true;
        self.running_tab = Some(tab.id);
        self.notice = None;
        self.engine.send(Command::Run { blocks, all_code, folder });
    }

    fn run_section(&mut self, section: usize) {
        let secs = self.tab().doc.sections();
        if let Some(r) = secs.get(section).cloned() {
            self.run_blocks(r);
        }
    }

    fn run_current_section(&mut self) {
        let s = self.tab().doc.section_of(self.tab().current);
        self.run_section(s);
    }

    fn run_and_advance(&mut self) {
        let s = self.tab().doc.section_of(self.tab().current);
        self.run_section(s);
        let t = self.tab_mut();
        let secs = t.doc.sections();
        if let Some(next) = secs.get(s + 1) {
            if next.start < t.doc.blocks.len() {
                t.current = next.start;
                t.focus_block = Some(t.doc.blocks[next.start].id);
            }
        }
    }

    fn run_all(&mut self) {
        if self.running {
            return;
        }
        self.tab_mut().outputs.clear();
        let n = self.tab().doc.blocks.len();
        self.run_blocks(0..n);
    }

    fn poll_engine(&mut self) {
        while let Ok(ev) = self.engine.rx.try_recv() {
            let running_tab = self.running_tab;
            let tab = self.tabs.iter_mut().find(|t| Some(t.id) == running_tab);
            match ev {
                Event::BlockStarted(id) => self.running_block = Some(id),
                Event::Output(id, out) => {
                    // The tab may have been closed while its code ran.
                    let Some(tab) = tab else { continue };
                    let list = tab.outputs.entry(id).or_default();
                    match out.kind {
                        OutputKind::Text(t) => match list.last_mut() {
                            Some(UiOutput::Text(prev)) => prev.push_str(&t),
                            _ => list.push(UiOutput::Text(t)),
                        },
                        OutputKind::Value { text, .. } => list.push(UiOutput::Value(text)),
                        OutputKind::Warning(w) => list.push(UiOutput::Warning(w)),
                        OutputKind::Error(e) => list.push(UiOutput::Error { message: e, line: out.line }),
                        OutputKind::Figure(f) => {
                            // A figure updated later in the same block replaces its earlier state.
                            match list.iter_mut().find(|o| matches!(o, UiOutput::Figure(g) if g.id == f.id)) {
                                Some(slot) => *slot = UiOutput::Figure(f),
                                None => list.push(UiOutput::Figure(f)),
                            }
                        }
                    }
                }
                Event::Finished { workspace, ok, elapsed } => {
                    self.workspace = workspace;
                    self.running = false;
                    self.running_tab = None;
                    self.running_block = None;
                    self.last_run = Some((ok, elapsed));
                }
            }
        }
    }

    // ---- editing -----------------------------------------------------------------------

    fn insert_block(&mut self, kind: BlockKind) {
        let at = (self.tab().current + 1).min(self.tab().doc.blocks.len());
        self.apply(vec![Action::Insert { at, kind }]);
    }

    fn apply(&mut self, actions: Vec<Action>) {
        for a in actions {
            let t = &mut self.tabs[self.active];
            match a {
                Action::Insert { at, kind } => {
                    if kind == BlockKind::SectionBreak {
                        t.doc.insert(at, BlockKind::SectionBreak, "");
                        let id = t.doc.insert(at + 1, BlockKind::Code, "");
                        t.current = at + 1;
                        t.focus_block = Some(id);
                    } else {
                        let id = t.doc.insert(at, kind, "");
                        t.current = at;
                        t.focus_block = Some(id);
                    }
                    t.dirty = true;
                }
                Action::Delete(i) => {
                    if i < t.doc.blocks.len() {
                        let b = t.doc.blocks.remove(i);
                        t.outputs.remove(&b.id);
                        if t.doc.blocks.is_empty() {
                            t.doc.push(BlockKind::Code, "");
                        }
                        t.current = t.current.min(t.doc.blocks.len() - 1);
                        t.dirty = true;
                    }
                }
                Action::Move { from, up } => {
                    let to = if up { from.checked_sub(1) } else { Some(from + 1).filter(|&to| to < t.doc.blocks.len()) };
                    if let Some(to) = to {
                        t.doc.blocks.swap(from, to);
                        t.current = to;
                        t.dirty = true;
                    }
                }
                Action::Focus(i) => t.current = i,
                Action::RunSection(s) => self.run_section(s),
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let sc = |m: Modifiers, k: Key| KeyboardShortcut::new(m, k);
        let pressed = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
        if pressed(sc(Modifiers::CTRL | Modifiers::ALT, Key::Enter)) {
            self.insert_block(BlockKind::SectionBreak);
        }
        if pressed(sc(Modifiers::CTRL | Modifiers::SHIFT, Key::Enter)) {
            self.run_and_advance();
        }
        if pressed(sc(Modifiers::CTRL, Key::Enter)) {
            self.run_current_section();
        }
        if pressed(sc(Modifiers::NONE, Key::F5)) {
            self.run_all();
        }
        if pressed(sc(Modifiers::CTRL | Modifiers::SHIFT, Key::S)) {
            self.save(true);
        }
        if pressed(sc(Modifiers::CTRL, Key::S)) {
            self.save(false);
        }
        if pressed(sc(Modifiers::CTRL, Key::O)) {
            self.open_dialog();
        }
        if pressed(sc(Modifiers::CTRL, Key::N)) {
            self.new_doc();
        }
        if pressed(sc(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab)) {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
        if pressed(sc(Modifiers::CTRL, Key::Tab)) {
            self.active = (self.active + 1) % self.tabs.len();
        }
        if pressed(sc(Modifiers::CTRL, Key::W)) {
            self.close_tab(ctx, self.active);
        }
        if self.running && pressed(sc(Modifiers::CTRL, Key::C)) && !ctx.wants_keyboard_input() {
            self.engine.stop();
        }
    }

    // ---- chrome ------------------------------------------------------------------------

    /// The title bar holds the document tabs (ModernTabStrip) and the caption buttons; the
    /// commands live in the command bar below it.
    fn title_bar(&mut self, ctx: &egui::Context) {
        let skin = self.skin.clone();
        let mut select = None;
        let mut close = None;
        let mut add = false;
        let mut rects = Vec::new();
        egui::TopBottomPanel::top("title")
            .exact_height(TITLE_BAR_HEIGHT)
            .frame(egui::Frame::none().fill(skin.window))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(12.0);
                    app_logo(ui, &skin);
                    ui.add_space(10.0);
                    // Tabs take their natural width, and shrink alike when they do not fit
                    // beside the + button, a grab area and the caption buttons.
                    let room = (ui.available_width() - 34.0 - 120.0 - 3.0 * 46.0).max(0.0);
                    let natural: Vec<f32> = self
                        .tabs
                        .iter()
                        .map(|t| {
                            let busy = if self.running_tab == Some(t.id) { 14.0 } else { 0.0 };
                            let font = FontId::new(13.0, egui::FontFamily::Name("semibold".into()));
                            let w = ui.fonts(|f| f.layout_no_wrap(t.title(), font, Color32::PLACEHOLDER).size().x);
                            (12.0 + busy + w + 6.0 + 26.0).clamp(96.0, 220.0)
                        })
                        .collect();
                    let total: f32 = natural.iter().map(|w| w + 4.0).sum();
                    let fit = (room / self.tabs.len() as f32 - 4.0).max(48.0);
                    let row_left = ui.cursor().left();
                    for (k, t) in self.tabs.iter().enumerate() {
                        let width = if total > room { fit.min(natural[k]) } else { natural[k] };
                        let tip = t.path.as_ref().map_or_else(|| t.title(), |p| p.display().to_string());
                        let dragging = self.drag.as_ref().is_some_and(|d| d.id == t.id);
                        let r = widgets::title_tab(ui, &skin, egui::Id::new(("tab", t.id)), &t.title(), &tip, width, k == self.active || dragging, t.dirty, self.running_tab == Some(t.id));
                        if r.close {
                            close = Some(k);
                        } else if r.selected {
                            select = Some(k);
                        }
                        if r.body.drag_started() {
                            self.drag = Some(TabDrag { id: t.id, torn: false });
                        }
                        rects.push(r.body.rect);
                        ui.add_space(4.0);
                    }
                    if widgets::add_tab_button(ui, &skin).on_hover_text("New live script (Ctrl+N)").clicked() {
                        add = true;
                    }
                    self.tab_row = Rect::from_x_y_ranges(row_left..=ui.cursor().left(), ui.max_rect().y_range());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        if widgets::caption_button(ui, &skin, Caption::Close).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        let mk = if maximized { Caption::Restore } else { Caption::Maximize };
                        if widgets::caption_button(ui, &skin, mk).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        }
                        if widgets::caption_button(ui, &skin, Caption::Minimize).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        // The rest of the bar drags the window.
                        let rect = ui.available_rect_before_wrap();
                        let resp = ui.interact(rect, ui.id().with("drag"), Sense::click_and_drag());
                        if resp.double_clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        } else if resp.drag_started_by(egui::PointerButton::Primary) {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                    });
                });
            });
        if let Some(k) = select {
            self.active = k;
        }
        if add {
            self.new_doc();
        }
        if let Some(k) = close {
            self.close_tab(ctx, k);
            return;
        }
        self.drag_tabs(ctx, &rects);
    }

    fn command_bar(&mut self, ctx: &egui::Context) {
        let skin = self.skin.clone();
        let mut insert = None;
        egui::TopBottomPanel::top("commands")
            .frame(egui::Frame::none().fill(skin.window).inner_margin(Margin { left: 12.0, right: 12.0, top: 2.0, bottom: 8.0 }))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::New), "New", true).on_hover_text("New live script (Ctrl+N)").clicked() {
                        self.new_doc();
                    }
                    if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::Open), "Open", true).on_hover_text("Open .mlx or .m (Ctrl+O)").clicked() {
                        self.open_dialog();
                    }
                    if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::Save), "Save", true).on_hover_text("Save (Ctrl+S)").clicked() {
                        self.save(false);
                    }
                    if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::SaveAs), "", true).on_hover_text("Save As (Ctrl+Shift+S)").clicked() {
                        self.save(true);
                    }
                    widgets::bar_separator(ui, &skin);
                    if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::Text), "Text", true).on_hover_text("Insert text").clicked() {
                        insert = Some(BlockKind::Text);
                    }
                    if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::Code), "Code", true).on_hover_text("Insert code").clicked() {
                        insert = Some(BlockKind::Code);
                    }
                    let heading = widgets::dropdown_button(ui, &skin, Some(Icon::Heading), "Heading", true).on_hover_text("Insert a title or heading");
                    dropdown(ui, &heading, "heading_menu", |ui| {
                        for (label, kind) in [("Title", BlockKind::Title), ("Heading", BlockKind::Heading), ("Subheading", BlockKind::Heading2)] {
                            if menu_item(ui, label, "") {
                                insert = Some(kind);
                            }
                        }
                    });
                    if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::SectionBreak), "Section Break", true)
                        .on_hover_text("Insert section break (Ctrl+Alt+Enter)")
                        .clicked()
                    {
                        insert = Some(BlockKind::SectionBreak);
                    }
                    widgets::bar_separator(ui, &skin);
                    let idle = !self.running;
                    if widgets::button(ui, &skin, ButtonStyle::Accent, Some(Icon::Run), "Run Section", idle).on_hover_text("Run the current section (Ctrl+Enter)").clicked() {
                        self.run_current_section();
                    }
                    if widgets::button(ui, &skin, ButtonStyle::Standard, Some(Icon::RunAdvance), "Advance", idle)
                        .on_hover_text("Run the section and move to the next (Ctrl+Shift+Enter)")
                        .clicked()
                    {
                        self.run_and_advance();
                    }
                    if widgets::button(ui, &skin, ButtonStyle::Standard, Some(Icon::RunAll), "Run All", idle).on_hover_text("Run the whole script (F5)").clicked() {
                        self.run_all();
                    }
                    if widgets::button(ui, &skin, ButtonStyle::Standard, Some(Icon::Stop), "Stop", self.running).on_hover_text("Stop execution (Ctrl+C)").clicked() {
                        self.engine.stop();
                    }
                    if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::ClearOutput), "", true).on_hover_text("Clear all output").clicked() {
                        self.tab_mut().outputs.clear();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let help = widgets::dropdown_button(ui, &skin, Some(Icon::Help), "", true).on_hover_text("Help");
                        let mut help_choice = None;
                        dropdown(ui, &help, "help_menu", |ui| {
                            if menu_item(ui, "Getting Started", "") {
                                help_choice = Some(0);
                            }
                            if menu_item(ui, "About matlivre", "") {
                                help_choice = Some(1);
                            }
                        });
                        match help_choice {
                            Some(0) => self.getting_started(),
                            Some(_) => {
                                self.notice = Some((
                                    "matlivre 0.1 — a clean-room live script environment written in Rust. Proof of concept.".into(),
                                    false,
                                ))
                            }
                            None => {}
                        }
                        let ws_style = if self.show_workspace { ButtonStyle::Toggled } else { ButtonStyle::Subtle };
                        if widgets::button(ui, &skin, ws_style, Some(Icon::Workspace), "", true).on_hover_text("Show or hide the workspace").clicked() {
                            self.show_workspace = !self.show_workspace;
                        }
                        ui.add_space(8.0);
                        let mut dark = skin.dark;
                        if widgets::toggle_switch(ui, &skin, &mut dark, "Dark").changed() {
                            self.set_dark(ctx, dark);
                        }
                        ui.add_space(12.0);
                        if let Some(i) = widgets::segmented(ui, &skin, &["Inline", "Right"], if self.output_right { 1 } else { 0 }) {
                            self.output_right = i == 1;
                        }
                    });
                });
            });
        if let Some(kind) = insert {
            self.insert_block(kind);
        }
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let skin = self.skin.clone();
        egui::TopBottomPanel::bottom("status")
            .exact_height(28.0)
            .frame(egui::Frame::none().fill(skin.window).inner_margin(Margin::symmetric(12.0, 0.0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if self.running {
                        widgets::progress_ring(ui, &skin, 16.0);
                        ui.label(RichText::new("Running…").color(skin.text));
                    } else {
                        match self.last_run {
                            Some((true, d)) => ui.label(RichText::new(format!("Ready   ·   last run completed in {:.2} s", d.as_secs_f64())).color(skin.text)),
                            Some((false, _)) => ui.label(RichText::new("Ready   ·   last run stopped with an error").color(skin.text)),
                            None => ui.label(RichText::new("Ready").color(skin.text)),
                        };
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let t = self.tab();
                        let secs = t.doc.sections().len();
                        let sec = t.doc.section_of(t.current) + 1;
                        ui.label(RichText::new(format!("Section {sec} of {secs}")).color(skin.text_secondary));
                        ui.label(RichText::new(match &t.path {
                            Some(p) => p.display().to_string(),
                            None => "Not saved".into(),
                        }).color(skin.text_secondary));
                    });
                });
            });
    }

    fn workspace_panel(&mut self, ctx: &egui::Context) {
        if !self.show_workspace {
            return;
        }
        let skin = self.skin.clone();
        egui::SidePanel::right("workspace")
            .resizable(true)
            .default_width(300.0)
            .min_width(200.0)
            .frame(egui::Frame::none().fill(skin.window).inner_margin(Margin { left: 0.0, right: 12.0, top: 0.0, bottom: 8.0 }))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(skin.surface)
                    .rounding(Rounding::same(RADIUS_LARGE))
                    .stroke(Stroke::new(1.0_f32, skin.border))
                    .inner_margin(Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.set_min_height(ui.available_height());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Workspace").font(FontId::new(15.0, egui::FontFamily::Name("semibold".into()))));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::Clear), "", !self.running)
                                    .on_hover_text("Clear workspace")
                                    .clicked()
                                {
                                    self.engine.send(Command::ClearWorkspace);
                                }
                            });
                        });
                        ui.add_space(4.0);
                        if self.workspace.is_empty() {
                            ui.add_space(8.0);
                            ui.label(RichText::new("Variables appear here after you run code.").color(skin.text_secondary));
                            return;
                        }
                        scroll_area(ui, &skin, |ui| {
                            let w = ui.available_width();
                            let header = |ui: &mut egui::Ui, cols: [&str; 3]| {
                                let (r, _) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
                                let p = ui.painter();
                                let xs = [r.left() + 6.0, r.left() + w * 0.36, r.left() + w * 0.76];
                                for (x, c) in xs.iter().zip(cols) {
                                    p.text(pos2(*x, r.center().y), Align2::LEFT_CENTER, c, FontId::proportional(12.0), skin.text_secondary);
                                }
                                p.line_segment([pos2(r.left(), r.bottom()), pos2(r.right(), r.bottom())], Stroke::new(1.0_f32, skin.separator));
                            };
                            header(ui, ["Name", "Value", "Size"]);
                            for v in &self.workspace {
                                let (r, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
                                let p = ui.painter();
                                if resp.hovered() {
                                    p.rect_filled(r, Rounding::same(RADIUS), skin.subtle_hover);
                                }
                                let col = |x: f32, width: f32, text: &str, color: Color32, mono: bool| {
                                    let font = if mono { FontId::monospace(12.5) } else { FontId::proportional(13.0) };
                                    let clip = Rect::from_min_size(pos2(x, r.top()), vec2(width - 8.0, r.height()));
                                    p.with_clip_rect(clip).text(pos2(x, r.center().y), Align2::LEFT_CENTER, text, font, color);
                                };
                                col(r.left() + 6.0, w * 0.36, &v.name, skin.text, false);
                                col(r.left() + w * 0.36, w * 0.40, &v.value, skin.text, true);
                                col(r.left() + w * 0.76, w * 0.24, &v.size, skin.text_secondary, false);
                                resp.on_hover_text(format!("{}  ·  {} {}  ·  {} bytes", v.name, v.size, v.class, v.bytes));
                            }
                        });
                    });
            });
    }

    fn document(&mut self, ctx: &egui::Context) {
        let skin = self.skin.clone();
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(skin.window).inner_margin(Margin { left: 12.0, right: 12.0, top: 0.0, bottom: 8.0 }))
            .show(ctx, |ui| {
                if let Some((msg, is_error)) = self.notice.clone() {
                    if info_bar(ui, &skin, &msg, is_error) {
                        self.notice = None;
                    }
                    ui.add_space(8.0);
                }
                let (output_right, running) = (self.output_right, self.running);
                let running_block = if self.running_tab == Some(self.tab().id) { self.running_block } else { None };
                let tab = &mut self.tabs[self.active];
                let mut actions = Vec::new();
                egui::Frame::none()
                    .fill(skin.surface)
                    .rounding(Rounding::same(RADIUS_LARGE))
                    .stroke(Stroke::new(1.0_f32, skin.border))
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        // One scroll position per tab.
                        ui.push_id(tab.id, |ui| {
                            scroll_area(ui, &skin, |ui| {
                                ui.add_space(20.0);
                                actions = blocks_ui(tab, ui, &skin, output_right, running, running_block);
                                ui.add_space(ui.available_height().max(120.0));
                            });
                        });
                    });
                self.apply(actions);
            });
    }

    fn confirm_close_all(&mut self) -> bool {
        (0..self.tabs.len()).all(|i| self.confirm_discard(i))
    }
    /// Resize handles for the borderless window.
    fn window_edges(&self, ctx: &egui::Context) {
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        if maximized {
            return;
        }
        let screen = ctx.screen_rect();
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else { return };
        let m = 5.0;
        let (l, r, t, b) = (pos.x < screen.left() + m, pos.x > screen.right() - m, pos.y < screen.top() + m, pos.y > screen.bottom() - m);
        use egui::{CursorIcon as C, ResizeDirection as D};
        let dir = match (l, r, t, b) {
            (true, _, true, _) => Some((D::NorthWest, C::ResizeNorthWest)),
            (_, true, true, _) => Some((D::NorthEast, C::ResizeNorthEast)),
            (true, _, _, true) => Some((D::SouthWest, C::ResizeSouthWest)),
            (_, true, _, true) => Some((D::SouthEast, C::ResizeSouthEast)),
            (true, ..) => Some((D::West, C::ResizeWest)),
            (_, true, ..) => Some((D::East, C::ResizeEast)),
            (_, _, true, _) => Some((D::North, C::ResizeNorth)),
            (_, _, _, true) => Some((D::South, C::ResizeSouth)),
            _ => None,
        };
        if let Some((d, cursor)) = dir {
            ctx.set_cursor_icon(cursor);
            if ctx.input(|i| i.pointer.primary_pressed()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(d));
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let received: Vec<PathBuf> = self.incoming.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        for m in received {
            self.receive(&m);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        // The last tab was closed or moved away; the window is closing.
        if self.tabs.is_empty() {
            return;
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.confirm_close_all() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.poll_engine();
        self.handle_shortcuts(ctx);
        if self.tabs.is_empty() {
            return;
        }
        self.title_bar(ctx);
        if self.tabs.is_empty() {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{} - matlivre", self.tab().title())));
        self.command_bar(ctx);
        self.status_bar(ctx);
        self.workspace_panel(ctx);
        self.document(ctx);
        self.window_edges(ctx);
        // Thin window border (the window is borderless).
        if !ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            let r = ctx.screen_rect();
            ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("border")))
                .rect_stroke(r.shrink(0.5), Rounding::ZERO, Stroke::new(1.0_f32, if self.skin.dark { Color32::from_rgb(60, 60, 60) } else { Color32::from_rgb(200, 200, 200) }));
        }
    }
}

/// A vertical scroll area whose bar looks like ModernScrollBar: a rounded thumb in the skin's
/// thumb colour over a transparent gutter. egui paints the thumb with the widget fills, which
/// are near the panel colour, so they are swapped for the bar and restored for the content.
fn scroll_area<R>(ui: &mut egui::Ui, skin: &Skin, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        let content_style = ui.style().clone();
        let v = &mut ui.style_mut().visuals;
        v.extreme_bg_color = Color32::TRANSPARENT;
        for (w, fill) in [
            (&mut v.widgets.inactive, skin.scroll_thumb),
            (&mut v.widgets.hovered, skin.scroll_thumb_hover),
            (&mut v.widgets.active, skin.scroll_thumb_hover),
        ] {
            w.bg_fill = fill;
            w.rounding = Rounding::same(3.0);
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_style(content_style);
                add_contents(ui)
            })
            .inner
    })
    .inner
}

/// Shows a menu under `button` while it is open; a click on the button toggles it.
fn dropdown(ui: &mut egui::Ui, button: &egui::Response, id: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    let id = ui.make_persistent_id(id);
    if button.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    egui::popup_below_widget(ui, id, button, PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(180.0);
        add_contents(ui);
    });
}

fn blocks_ui(tab: &mut Tab, ui: &mut egui::Ui, skin: &Skin, output_right: bool, running: bool, running_block: Option<u64>) -> Vec<Action> {
    let mut actions = Vec::new();
    let full_w = ui.available_width();
    let content_w = if output_right { full_w - 64.0 } else { (full_w - 64.0).min(960.0) };
    let left = ui.max_rect().left() + ((full_w - content_w) / 2.0).max(32.0);
    let current_section = tab.doc.section_of(tab.current);
    let mut section = 0usize;
    let n = tab.doc.blocks.len();
    for i in 0..n {
        let kind = tab.doc.blocks[i].kind;
        let id = tab.doc.blocks[i].id;
        if kind == BlockKind::SectionBreak {
            section += 1;
            let (r, resp) = ui.allocate_exact_size(vec2(full_w, 22.0), Sense::hover());
            let y = r.center().y;
            let line = Rect::from_min_max(pos2(left, y), pos2(left + content_w, y));
            ui.painter().extend(egui::Shape::dashed_line(&[line.left_center(), line.right_center()], Stroke::new(1.0_f32, skin.border), 6.0, 4.0));
            if resp.hovered() || ui.rect_contains_pointer(r) {
                let br = Rect::from_center_size(pos2(left + content_w - 14.0, y), vec2(28.0, 22.0));
                // A detached child so the overlay does not move the layout cursor.
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(br));
                if widgets::button(&mut child, skin, ButtonStyle::Subtle, Some(Icon::Delete), "", true).on_hover_text("Remove section break").clicked() {
                    actions.push(Action::Delete(i));
                }
            }
            continue;
        }
        let is_current_section = section == current_section;
        let block_top = ui.cursor().top();
        ui.horizontal_top(|ui| {
            ui.add_space(left - ui.max_rect().left());
            ui.vertical(|ui| {
                ui.set_width(content_w);
                let block = &mut tab.doc.blocks[i];
                let edit_id = egui::Id::new(("block", block.id));
                let resp = match kind {
                    BlockKind::Code => {
                        let outs = tab.outputs.get(&id);
                        if output_right {
                            let mut r = None;
                            ui.columns(2, |cols| {
                                r = Some(code_editor(&mut cols[0], skin, &mut block.text, edit_id));
                                if let Some(o) = outs {
                                    outputs_ui(&mut cols[1], skin, o);
                                }
                            });
                            r.unwrap()
                        } else {
                            let r = code_editor(ui, skin, &mut block.text, edit_id);
                            if let Some(o) = outs {
                                ui.add_space(2.0);
                                outputs_ui(ui, skin, o);
                            }
                            r
                        }
                    }
                    _ => {
                        let style = match kind {
                            BlockKind::Title => TextStyle::Name("title".into()),
                            BlockKind::Heading => TextStyle::Name("h1".into()),
                            BlockKind::Heading2 => TextStyle::Name("h2".into()),
                            _ => TextStyle::Body,
                        };
                        let hint = match kind {
                            BlockKind::Title => "Title",
                            BlockKind::Heading | BlockKind::Heading2 => "Heading",
                            _ => "Enter text",
                        };
                        ui.add_space(if kind == BlockKind::Text { 2.0 } else { 6.0 });
                        ui.add(
                            egui::TextEdit::multiline(&mut block.text)
                                .id(edit_id)
                                .font(style)
                                .frame(false)
                                .margin(Margin::symmetric(4.0, 2.0))
                                .desired_rows(1)
                                .desired_width(f32::INFINITY)
                                .hint_text(RichText::new(hint).color(skin.text_disabled)),
                        )
                    }
                };
                if resp.changed() {
                    tab.dirty = true;
                }
                if resp.has_focus() && tab.current != i {
                    actions.push(Action::Focus(i));
                }
                if tab.focus_block == Some(id) {
                    resp.request_focus();
                    tab.focus_block = None;
                }
            });
        });
        let block_rect = Rect::from_min_max(pos2(left - 28.0, block_top), pos2(left + content_w + 4.0, ui.cursor().top()));
        // Section gutter: accent bar for the current section, animated while running.
        let bar = Rect::from_min_max(pos2(left - 16.0, block_top + 2.0), pos2(left - 13.0, block_rect.bottom() - 2.0));
        if running && running_block == Some(id) {
            let t = ui.input(|inp| inp.time) as f32;
            let a = 0.55 + 0.45 * (t * 6.0).sin();
            ui.painter().rect_filled(bar, Rounding::same(1.5), skin.accent.gamma_multiply(a));
            ui.ctx().request_repaint();
        } else if is_current_section {
            ui.painter().rect_filled(bar, Rounding::same(1.5), skin.accent);
        } else if ui.rect_contains_pointer(block_rect) {
            ui.painter().rect_filled(bar, Rounding::same(1.5), skin.border);
        }
        // Clicking the gutter of a section runs it, as in the Live Editor.
        let gutter = Rect::from_min_max(pos2(left - 22.0, block_top), pos2(left - 8.0, block_rect.bottom()));
        let g = ui.interact(gutter, egui::Id::new(("gutter", id)), Sense::click()).on_hover_text("Click to run this section");
        if g.clicked() && !running {
            actions.push(Action::RunSection(section));
            actions.push(Action::Focus(i));
        }
        // Hover tools: move up / down, delete.
        if ui.rect_contains_pointer(block_rect) {
            let y = block_top + 4.0;
            let x = left + content_w - 3.0 * 30.0;
            for (k, (icon, tip)) in [(Icon::Up, "Move up"), (Icon::Down, "Move down"), (Icon::Delete, "Delete block")].into_iter().enumerate() {
                let r = Rect::from_min_size(pos2(x + k as f32 * 30.0, y), vec2(28.0, 28.0));
                if !output_right || kind != BlockKind::Code {
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
                    let clicked = widgets::button(&mut child, skin, ButtonStyle::Subtle, Some(icon), "", true).on_hover_text(tip).clicked();
                    if clicked {
                        actions.push(match icon {
                            Icon::Up => Action::Move { from: i, up: true },
                            Icon::Down => Action::Move { from: i, up: false },
                            _ => Action::Delete(i),
                        });
                    }
                }
            }
        }
        ui.add_space(6.0);
    }
    actions
}

fn app_logo(ui: &mut egui::Ui, skin: &Skin) {
    let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, Rounding::same(4.0), skin.accent);
    let pts: Vec<egui::Pos2> = (0..=16)
        .map(|k| {
            let t = k as f32 / 16.0;
            pos2(r.left() + 3.0 + t * 12.0, r.center().y + 1.0 - (t * std::f32::consts::TAU).sin() * 4.0)
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.6_f32, skin.on_accent)));
}

fn menu_item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    let mut b = egui::Button::new(label).min_size(vec2(ui.available_width(), 30.0));
    if !shortcut.is_empty() {
        b = b.shortcut_text(shortcut);
    }
    ui.add(b).clicked()
}

fn code_editor(ui: &mut egui::Ui, skin: &Skin, text: &mut String, id: egui::Id) -> egui::Response {
    let font = FontId::monospace(13.5);
    let row_h = ui.fonts(|f| f.row_height(&font));
    let lines = text.split('\n').count().max(1);
    let mut resp = None;
    egui::Frame::none()
        .fill(skin.code_bg)
        .rounding(Rounding::same(RADIUS))
        .stroke(Stroke::new(1.0_f32, skin.code_border))
        .inner_margin(Margin { left: 0.0, right: 8.0, top: 7.0, bottom: 7.0 })
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                // Line numbers.
                let num_w = 40.0;
                let (r, _) = ui.allocate_exact_size(vec2(num_w, row_h * lines as f32), Sense::hover());
                for k in 0..lines {
                    ui.painter().text(
                        pos2(r.right() - 12.0, r.top() + row_h * k as f32),
                        Align2::RIGHT_TOP,
                        (k + 1).to_string(),
                        font.clone(),
                        skin.text_disabled,
                    );
                }
                let skin2 = skin.clone();
                let font2 = font.clone();
                let mut layouter = move |ui: &egui::Ui, s: &str, wrap: f32| {
                    let mut job = crate::highlight::highlight(s, &skin2, font2.clone());
                    job.wrap.max_width = wrap;
                    ui.fonts(|f| f.layout_job(job))
                };
                let r = ui.add(
                    egui::TextEdit::multiline(text)
                        .id(id)
                        .code_editor()
                        .frame(false)
                        .margin(Margin::ZERO)
                        .lock_focus(true)
                        .desired_rows(1)
                        .desired_width(f32::INFINITY)
                        .layouter(&mut layouter),
                );
                resp = Some(r);
            });
        });
    resp.unwrap()
}

fn outputs_ui(ui: &mut egui::Ui, skin: &Skin, outs: &[UiOutput]) {
    egui::Frame::none().inner_margin(Margin { left: 12.0, right: 8.0, top: 6.0, bottom: 4.0 }).show(ui, |ui| {
        for o in outs {
            match o {
                UiOutput::Text(t) => {
                    let t = t.trim_end_matches('\n');
                    if !t.is_empty() {
                        ui.add(egui::Label::new(RichText::new(t).font(FontId::monospace(13.0)).color(skin.text)).selectable(true));
                    }
                }
                UiOutput::Value(t) => {
                    ui.add(egui::Label::new(RichText::new(t).font(FontId::monospace(13.0)).color(skin.text)).selectable(true));
                }
                UiOutput::Warning(w) => {
                    egui::Frame::none().fill(skin.warning_bg).rounding(Rounding::same(RADIUS)).inner_margin(Margin::symmetric(8.0, 4.0)).show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(format!("Warning: {w}")).font(FontId::monospace(13.0)).color(skin.warning)).selectable(true));
                    });
                }
                UiOutput::Error { message, line } => {
                    info_bar(ui, skin, &format!("Error (line {line}): {message}"), true);
                }
                UiOutput::Figure(f) => {
                    let w = ui.available_width().min(600.0);
                    crate::plot::figure(ui, skin, f, w);
                }
            }
            ui.add_space(4.0);
        }
    });
}

/// ModernInfoBar (informational / error). Returns true when dismissed.
fn info_bar(ui: &mut egui::Ui, skin: &Skin, msg: &str, error: bool) -> bool {
    let mut closed = false;
    let (bg, icon) = if error { (skin.error_bg, skin.error) } else { (if skin.dark { Color32::from_rgb(43, 43, 43) } else { Color32::from_rgb(246, 246, 246) }, skin.accent) };
    egui::Frame::none()
        .fill(bg)
        .rounding(Rounding::same(RADIUS))
        .stroke(Stroke::new(1.0_f32, skin.separator))
        .inner_margin(Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 8.0, icon);
                let glyph = if error { "!" } else { "i" };
                ui.painter().text(r.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(11.0), if skin.dark { Color32::BLACK } else { Color32::WHITE });
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(msg).color(skin.text)).wrap());
                if !error {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("✕").clicked() {
                            closed = true;
                        }
                    });
                }
            });
        });
    closed
}
