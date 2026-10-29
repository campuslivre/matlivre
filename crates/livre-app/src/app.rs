use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, FontId, Key, KeyboardShortcut, Layout, Margin, Modifiers, Rect, RichText, Rounding, Sense,
    Stroke, TextStyle,
};
use livre_doc::{BlockKind, Document};
use livre_lang::{Figure, OutputKind, VarInfo};

use crate::engine::{Command, Engine, Event};
use crate::theme::{Skin, RADIUS, RADIUS_LARGE, TITLE_BAR_HEIGHT};
use crate::widgets::{self, ButtonStyle, Caption, Icon};

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

pub struct App {
    skin: Skin,
    doc: Document,
    path: Option<PathBuf>,
    dirty: bool,
    outputs: HashMap<u64, Vec<UiOutput>>,
    current: usize,
    focus_block: Option<u64>,
    engine: Engine,
    running: bool,
    running_block: Option<u64>,
    workspace: Vec<VarInfo>,
    show_workspace: bool,
    output_right: bool,
    last_run: Option<(bool, Duration)>,
    notice: Option<(String, bool)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        crate::theme::install_fonts(&cc.egui_ctx);
        let skin = Skin::light();
        skin.apply(&cc.egui_ctx);
        let mut app = Self {
            skin,
            doc: livre_doc::read_m(WELCOME),
            path: None,
            dirty: false,
            outputs: HashMap::new(),
            current: 0,
            focus_block: None,
            engine: Engine::start(cc.egui_ctx.clone()),
            running: false,
            running_block: None,
            workspace: Vec::new(),
            show_workspace: true,
            output_right: false,
            last_run: None,
            notice: None,
        };
        if let Some(p) = open {
            app.open_path(p);
        }
        app
    }

    fn set_dark(&mut self, ctx: &egui::Context, dark: bool) {
        self.skin = if dark { Skin::dark() } else { Skin::light() };
        self.skin.apply(ctx);
    }

    // ---- files -------------------------------------------------------------------------

    fn open_path(&mut self, p: PathBuf) {
        match Document::load(&p) {
            Ok(doc) => {
                self.doc = doc;
                self.path = Some(p);
                self.dirty = false;
                self.outputs.clear();
                self.current = 0;
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

    fn save(&mut self, save_as: bool) {
        let path = match (&self.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new()
                .add_filter("Live Script", &["mlx"])
                .add_filter("Script", &["m"])
                .set_file_name(self.file_name().replace('*', ""))
                .save_file(),
        };
        if let Some(p) = path {
            match self.doc.save(&p) {
                Ok(()) => {
                    self.path = Some(p);
                    self.dirty = false;
                }
                Err(e) => self.notice = Some((format!("Could not save: {e}"), true)),
            }
        }
    }

    fn new_doc(&mut self) {
        self.doc = Document::new_script();
        self.path = None;
        self.dirty = false;
        self.outputs.clear();
        self.current = 0;
        self.focus_block = self.doc.blocks.first().map(|b| b.id);
    }

    fn file_name(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            None => "Untitled.mlx".into(),
        }
    }

    // ---- running -----------------------------------------------------------------------

    fn all_code(&self) -> Vec<String> {
        self.doc.blocks.iter().filter(|b| b.kind == BlockKind::Code).map(|b| b.text.clone()).collect()
    }

    fn run_blocks(&mut self, range: std::ops::Range<usize>) {
        if self.running {
            return;
        }
        let blocks: Vec<(u64, String)> =
            self.doc.blocks[range].iter().filter(|b| b.kind == BlockKind::Code).map(|b| (b.id, b.text.clone())).collect();
        if blocks.is_empty() {
            return;
        }
        for (id, _) in &blocks {
            self.outputs.remove(id);
        }
        self.running = true;
        self.notice = None;
        let all_code = self.all_code();
        self.engine.send(Command::Run { blocks, all_code });
    }

    fn run_section(&mut self, section: usize) {
        let secs = self.doc.sections();
        if let Some(r) = secs.get(section).cloned() {
            self.run_blocks(r);
        }
    }

    fn run_current_section(&mut self) {
        let s = self.doc.section_of(self.current);
        self.run_section(s);
    }

    fn run_and_advance(&mut self) {
        let s = self.doc.section_of(self.current);
        self.run_section(s);
        let secs = self.doc.sections();
        if let Some(next) = secs.get(s + 1) {
            if next.start < self.doc.blocks.len() {
                self.current = next.start;
                self.focus_block = Some(self.doc.blocks[next.start].id);
            }
        }
    }

    fn run_all(&mut self) {
        if self.running {
            return;
        }
        self.outputs.clear();
        let n = self.doc.blocks.len();
        self.run_blocks(0..n);
    }

    fn poll_engine(&mut self) {
        while let Ok(ev) = self.engine.rx.try_recv() {
            match ev {
                Event::BlockStarted(id) => self.running_block = Some(id),
                Event::Output(id, out) => {
                    let list = self.outputs.entry(id).or_default();
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
                    self.running_block = None;
                    self.last_run = Some((ok, elapsed));
                }
            }
        }
    }

    // ---- editing -----------------------------------------------------------------------

    fn insert_block(&mut self, kind: BlockKind) {
        let at = (self.current + 1).min(self.doc.blocks.len());
        self.apply(vec![Action::Insert { at, kind }]);
    }

    fn apply(&mut self, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::Insert { at, kind } => {
                    if kind == BlockKind::SectionBreak {
                        self.doc.insert(at, BlockKind::SectionBreak, "");
                        let id = self.doc.insert(at + 1, BlockKind::Code, "");
                        self.current = at + 1;
                        self.focus_block = Some(id);
                    } else {
                        let id = self.doc.insert(at, kind, "");
                        self.current = at;
                        self.focus_block = Some(id);
                    }
                    self.dirty = true;
                }
                Action::Delete(i) => {
                    if i < self.doc.blocks.len() {
                        let b = self.doc.blocks.remove(i);
                        self.outputs.remove(&b.id);
                        if self.doc.blocks.is_empty() {
                            self.doc.push(BlockKind::Code, "");
                        }
                        self.current = self.current.min(self.doc.blocks.len() - 1);
                        self.dirty = true;
                    }
                }
                Action::Move { from, up } => {
                    let to = if up { from.checked_sub(1) } else { Some(from + 1).filter(|&t| t < self.doc.blocks.len()) };
                    if let Some(to) = to {
                        self.doc.blocks.swap(from, to);
                        self.current = to;
                        self.dirty = true;
                    }
                }
                Action::Focus(i) => self.current = i,
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
        if self.running && pressed(sc(Modifiers::CTRL, Key::C)) && !ctx.wants_keyboard_input() {
            self.engine.stop();
        }
    }

    // ---- chrome ------------------------------------------------------------------------

    fn title_bar(&mut self, ctx: &egui::Context) {
        let skin = self.skin.clone();
        egui::TopBottomPanel::top("title")
            .exact_height(TITLE_BAR_HEIGHT)
            .frame(egui::Frame::none().fill(skin.window))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(12.0);
                    app_logo(ui, &skin);
                    ui.add_space(10.0);
                    self.menus(ui, ctx);
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
                        // The rest of the bar drags the window and shows the caption.
                        let rect = ui.available_rect_before_wrap();
                        let resp = ui.interact(rect, ui.id().with("drag"), Sense::click_and_drag());
                        if resp.double_clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        } else if resp.drag_started_by(egui::PointerButton::Primary) {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                        let p = ui.painter();
                        let name = format!("{}{}", self.file_name(), if self.dirty { " •" } else { "" });
                        let x = rect.left() + 16.0;
                        let r1 = p.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, &name, FontId::proportional(12.5), skin.text);
                        p.text(pos2(r1.right() + 6.0, rect.center().y), Align2::LEFT_CENTER, "·  matlivre", FontId::proportional(12.5), skin.text_secondary);
                    });
                });
            });
        let title = format!("{} - matlivre", self.file_name());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }

    fn menus(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.style_mut().spacing.button_padding = vec2(10.0, 6.0);
        ui.menu_button("File", |ui| {
            ui.set_min_width(220.0);
            if menu_item(ui, "New", "Ctrl+N") {
                self.new_doc();
                ui.close_menu();
            }
            if menu_item(ui, "Open…", "Ctrl+O") {
                ui.close_menu();
                self.open_dialog();
            }
            ui.separator();
            if menu_item(ui, "Save", "Ctrl+S") {
                ui.close_menu();
                self.save(false);
            }
            if menu_item(ui, "Save As…", "Ctrl+Shift+S") {
                ui.close_menu();
                self.save(true);
            }
            ui.separator();
            if menu_item(ui, "Exit", "Alt+F4") {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        ui.menu_button("Insert", |ui| {
            ui.set_min_width(240.0);
            for (label, kind, key) in [
                ("Text", BlockKind::Text, ""),
                ("Code", BlockKind::Code, ""),
                ("Section Break", BlockKind::SectionBreak, "Ctrl+Alt+Enter"),
                ("Title", BlockKind::Title, ""),
                ("Heading", BlockKind::Heading, ""),
                ("Subheading", BlockKind::Heading2, ""),
            ] {
                if menu_item(ui, label, key) {
                    self.insert_block(kind);
                    ui.close_menu();
                }
            }
        });
        ui.menu_button("View", |ui| {
            ui.set_min_width(220.0);
            let mut ws = self.show_workspace;
            if ui.checkbox(&mut ws, "Workspace").changed() {
                self.show_workspace = ws;
            }
            let mut right = self.output_right;
            if ui.checkbox(&mut right, "Output on Right").changed() {
                self.output_right = right;
            }
            let mut dark = self.skin.dark;
            if ui.checkbox(&mut dark, "Dark Theme").changed() {
                self.set_dark(ctx, dark);
            }
        });
        ui.menu_button("Run", |ui| {
            ui.set_min_width(260.0);
            ui.add_enabled_ui(!self.running, |ui| {
                if menu_item(ui, "Run Section", "Ctrl+Enter") {
                    self.run_current_section();
                    ui.close_menu();
                }
                if menu_item(ui, "Run and Advance", "Ctrl+Shift+Enter") {
                    self.run_and_advance();
                    ui.close_menu();
                }
                if menu_item(ui, "Run All", "F5") {
                    self.run_all();
                    ui.close_menu();
                }
            });
            ui.add_enabled_ui(self.running, |ui| {
                if menu_item(ui, "Stop", "") {
                    self.engine.stop();
                    ui.close_menu();
                }
            });
            ui.separator();
            if menu_item(ui, "Clear All Output", "") {
                self.outputs.clear();
                ui.close_menu();
            }
            ui.add_enabled_ui(!self.running, |ui| {
                if menu_item(ui, "Clear Workspace", "") {
                    self.engine.send(Command::ClearWorkspace);
                    ui.close_menu();
                }
            });
        });
        ui.menu_button("Help", |ui| {
            ui.set_min_width(200.0);
            if menu_item(ui, "Getting Started", "") {
                self.doc = livre_doc::read_m(WELCOME);
                self.path = None;
                self.outputs.clear();
                ui.close_menu();
            }
            if menu_item(ui, "About matlivre", "") {
                self.notice = Some((
                    "matlivre 0.1 — a clean-room live script environment written in Rust. Proof of concept.".into(),
                    false,
                ));
                ui.close_menu();
            }
        });
    }

    fn command_bar(&mut self, ctx: &egui::Context) {
        let skin = self.skin.clone();
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
                    widgets::bar_separator(ui, &skin);
                    if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::Text), "Text", true).on_hover_text("Insert text").clicked() {
                        self.insert_block(BlockKind::Text);
                    }
                    if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::Code), "Code", true).on_hover_text("Insert code").clicked() {
                        self.insert_block(BlockKind::Code);
                    }
                    if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::Heading), "Heading", true).on_hover_text("Insert heading").clicked() {
                        self.insert_block(BlockKind::Heading);
                    }
                    if widgets::button(ui, &skin, ButtonStyle::Subtle, Some(Icon::SectionBreak), "Section Break", true)
                        .on_hover_text("Insert section break (Ctrl+Alt+Enter)")
                        .clicked()
                    {
                        self.insert_block(BlockKind::SectionBreak);
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
                    if widgets::button(ui, &skin, ButtonStyle::Standard, Some(Icon::Stop), "Stop", self.running).on_hover_text("Stop execution").clicked() {
                        self.engine.stop();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
                        let secs = self.doc.sections().len();
                        let sec = self.doc.section_of(self.current) + 1;
                        ui.label(RichText::new(format!("Section {sec} of {secs}")).color(skin.text_secondary));
                        ui.label(RichText::new(match &self.path {
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
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
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
                egui::Frame::none()
                    .fill(skin.surface)
                    .rounding(Rounding::same(RADIUS_LARGE))
                    .stroke(Stroke::new(1.0_f32, skin.border))
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            ui.add_space(20.0);
                            let actions = self.blocks_ui(ui, &skin);
                            ui.add_space(ui.available_height().max(120.0));
                            self.apply(actions);
                        });
                    });
            });
    }

    fn blocks_ui(&mut self, ui: &mut egui::Ui, skin: &Skin) -> Vec<Action> {
        let mut actions = Vec::new();
        let full_w = ui.available_width();
        let content_w = if self.output_right { full_w - 64.0 } else { (full_w - 64.0).min(960.0) };
        let left = ui.max_rect().left() + ((full_w - content_w) / 2.0).max(32.0);
        let current_section = self.doc.section_of(self.current);
        let mut section = 0usize;
        let n = self.doc.blocks.len();
        let running_block = self.running_block;
        let running = self.running;
        let output_right = self.output_right;
        for i in 0..n {
            let kind = self.doc.blocks[i].kind;
            let id = self.doc.blocks[i].id;
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
                    let block = &mut self.doc.blocks[i];
                    let edit_id = egui::Id::new(("block", block.id));
                    let resp = match kind {
                        BlockKind::Code => {
                            let outs = self.outputs.get(&id);
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
                        self.dirty = true;
                    }
                    if resp.has_focus() && self.current != i {
                        actions.push(Action::Focus(i));
                    }
                    if self.focus_block == Some(id) {
                        resp.request_focus();
                        self.focus_block = None;
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
        self.poll_engine();
        self.handle_shortcuts(ctx);
        self.title_bar(ctx);
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
