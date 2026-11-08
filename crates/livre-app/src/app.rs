use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui::text::{CCursor, LayoutJob, TextFormat};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, FontId, Id, Key, KeyboardShortcut, Layout, Margin, Modifiers, Pos2, Rect, RichText, Rounding,
    Sense, Stroke, Ui, UiBuilder,
};
use livre_doc::{Block, BlockKind, ControlKind, Document};
use livre_lang::{Figure, OutputKind, VarInfo};

use crate::chat::{self, Chat};
use crate::complete::{self, Completer};
use crate::engine::{Command, Engine, Event, COMMAND_ID};
use crate::mcp::{self, ToolResult};
use crate::files::FileTree;
use crate::icons::{self, Ic};
use crate::os;
use crate::theme::{semibold, Skin, RADIUS, RADIUS_LARGE, TITLE_BAR_HEIGHT};
use crate::transfer;
use crate::widgets::{self, Caption, MenuEntry};
use crate::Launch;

const WELCOME: &str = include_str!("../../../examples/welcome.m");

pub const DEFAULT_SIZE: [f32; 2] = [1360.0, 860.0];
pub const MIN_SIZE: [f32; 2] = [760.0, 480.0];

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Live,
    /// A plain `.m` file: one code block holding the whole text, `%%` lines start sections.
    Script,
}

/// One open document.
struct Tab {
    id: u64,
    kind: Kind,
    doc: Document,
    path: Option<PathBuf>,
    /// The name while the document has no file: untitled.mlx, untitled2.mlx, ... as in MATLAB.
    name: String,
    dirty: bool,
    outputs: HashMap<u64, Vec<UiOutput>>,
    /// Blocks being run again: their old outputs stay, dimmed, until new ones arrive.
    stale: HashSet<u64>,
    /// Live script: the current block. Script: the line holding the caret.
    current: usize,
    focus_block: Option<u64>,
    /// The caret in the focused code block, where Insert › Control puts a control.
    caret: Option<(u64, egui::text::CCursorRange)>,
    /// The file as last read or written here, to notice changes made by other programs.
    disk: Option<Disk>,
    /// The file changed on disk while the tab had unsaved changes: the user picks a version.
    conflict: bool,
}

/// What a file on disk looked like: its time and size are checked often, its content
/// only when they change.
#[derive(Clone, PartialEq)]
struct Disk {
    modified: Option<SystemTime>,
    len: u64,
    hash: u64,
}

fn hash_bytes(b: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
}

fn disk_state(path: &Path) -> Option<Disk> {
    let meta = std::fs::metadata(path).ok()?;
    let bytes = std::fs::read(path).ok()?;
    Some(Disk { modified: meta.modified().ok(), len: meta.len(), hash: hash_bytes(&bytes) })
}

/// How a file on disk relates to the tab showing it.
#[derive(Debug, PartialEq)]
enum DiskChange {
    None,
    /// Only its time changed (or it was written by this window).
    Touched,
    /// Another program changed it: the tab reloads, or asks when it has unsaved changes.
    Changed,
}

fn disk_change(known: &Disk, now: &Disk) -> DiskChange {
    if known == now {
        DiskChange::None
    } else if known.hash == now.hash && known.len == now.len {
        DiskChange::Touched
    } else {
        DiskChange::Changed
    }
}

/// Suggested code shown after the caret, which Tab inserts.
#[derive(Clone)]
struct Ghost {
    block: u64,
    /// Char index of the caret.
    at: usize,
    text: String,
}

/// The focused editor's caret: block, char index, and a hash of the block's text.
#[derive(Clone, Copy, PartialEq)]
struct Caret {
    block: u64,
    index: usize,
    text: u64,
}

fn hash_text(s: &str) -> u64 {
    hash_bytes(s.as_bytes())
}

/// Code completion: what the editor did lately and what was asked.
struct Completion {
    on: bool,
    ghost: Option<Ghost>,
    seen: Option<Caret>,
    /// When the text last changed; a request goes out after a pause.
    edited_at: Option<Instant>,
    /// The request in flight, the caret it was made at, and the text before and after it.
    asked: Option<(u64, Caret, String, String)>,
    next_key: u64,
    /// Alt+\ asks now, wherever the caret is.
    force: bool,
}

const COMPLETION_PAUSE: Duration = Duration::from_millis(600);

impl Tab {
    fn title(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            None => self.name.clone(),
        }
    }

    /// An untouched untitled document, which opening a file replaces instead of keeping.
    fn is_pristine(&self) -> bool {
        self.path.is_none() && !self.dirty && self.doc.blocks.iter().all(|b| b.text.trim().is_empty())
    }

    fn script_text(&self) -> &str {
        self.doc.blocks.first().map_or("", |b| b.text.as_str())
    }

    fn section_count(&self) -> usize {
        match self.kind {
            Kind::Live => self.doc.sections().len(),
            Kind::Script => script_sections(self.script_text()).len(),
        }
    }

    fn current_section(&self) -> usize {
        match self.kind {
            Kind::Live => self.doc.section_of(self.current),
            Kind::Script => script_section_of(self.script_text(), self.current),
        }
    }
}

fn is_section_header(line: &str) -> bool {
    line.trim_start().starts_with("%%")
}

/// Line ranges of a plain script's sections: a `%%` line starts one, except on the first line.
fn script_sections(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    let lines: Vec<&str> = text.split('\n').collect();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 && is_section_header(l) {
            out.push(start..i);
            start = i;
        }
    }
    out.push(start..lines.len());
    out
}

fn script_section_of(text: &str, line: usize) -> usize {
    script_sections(text).iter().position(|r| r.contains(&line)).unwrap_or(0)
}

/// A tab being dragged by its header.
struct TabDrag {
    id: u64,
    /// The pointer is away from the tab row, so letting go moves the tab to another window.
    torn: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HistKind {
    Input,
    Output,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    New,
    Save,
    Insert,
    More,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlTag {
    Slider,
    DropDown,
    CheckBox,
    EditField,
    Button,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuCmd {
    NewLive,
    NewScript,
    Save,
    SaveAs,
    ExportM,
    Insert(BlockKind),
    Control(ControlTag),
    ClearOutput,
    ToggleTheme,
    ToggleCompletions,
    ConnectClaude,
    GettingStarted,
}

/// What is running and where its output goes.
#[derive(Clone, Copy)]
enum Origin {
    Tab { id: u64, kind: Kind, line_offset: usize },
    Command,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FigAction {
    Copy,
    Save,
}

/// Editing a control's settings (right-click on it).
struct ControlEdit {
    block: u64,
    index: usize,
    /// Below the control; set once the control has been laid out.
    at: Option<Pos2>,
    fields: [String; 3],
}

/// Frame state the document views read and write, kept apart from the tab they borrow.
struct View {
    skin: Skin,
    output_right: bool,
    running: bool,
    /// The section running in this tab.
    running_section: Option<usize>,
    hover_section: Option<usize>,
    hover_block: Option<u64>,
    toolbar_rect: Rect,
    next_hover_section: Option<usize>,
    next_hover_block: Option<u64>,
    next_toolbar_rect: Rect,
    copied: Option<(u64, u32, f64)>,
    now: f64,
    figure_action: Option<(Figure, FigAction, u64)>,
    control_runs: Vec<usize>,
    control_edit: Option<ControlEdit>,
    /// The completion on show, and whether Tab took it this frame.
    ghost: Option<Ghost>,
    ghost_taken: bool,
    /// The caret of the focused code editor.
    caret: Option<Caret>,
}

pub struct App {
    skin: Skin,
    tabs: Vec<Tab>,
    active: usize,
    next_tab: u64,
    engine: Engine,
    running: bool,
    origin: Option<Origin>,
    running_block: Option<u64>,
    /// A section to run as soon as the current run ends (a control changed meanwhile).
    pending: Option<(u64, usize)>,
    workspace: Vec<VarInfo>,
    output_right: bool,
    last_run: Option<(bool, Duration)>,
    /// Section and line (1-based) of the last run's error.
    last_error: Option<(usize, usize)>,
    notice: Option<(String, bool)>,
    drag: Option<TabDrag>,
    window: Option<os::Window>,
    /// DWM rounds the window and draws its border (Windows 11).
    rounded: bool,
    /// Tabs dropped here from other instances.
    incoming: Option<Receiver<PathBuf>>,
    /// Files opened from Finder or the Dock (macOS).
    #[cfg(target_os = "macos")]
    open_files: Receiver<PathBuf>,
    /// Where the tab headers are, to tell a reorder from a tear-off.
    tab_row: Rect,
    files_open: bool,
    cmd_open: bool,
    ws_open: bool,
    files_w: f32,
    ws_w: f32,
    cmd_h: f32,
    tree: FileTree,
    hist: Vec<(String, HistKind)>,
    cmd_input: String,
    cmd_history: Vec<String>,
    cmd_recall: Option<usize>,
    menu: Option<(Menu, Rect)>,
    submenu_rect: Option<Rect>,
    hover_section: Option<usize>,
    hover_block: Option<u64>,
    toolbar_rect: Rect,
    copied: Option<(u64, u32, f64)>,
    control_edit: Option<ControlEdit>,
    /// Figures drawn by scripts and the command window, shown in floating windows.
    figures: Vec<(Figure, bool)>,
    /// The save-changes dialog, while it is up.
    confirm: Option<Confirm>,
    /// The layout last written to the settings file.
    saved_layout: SavedLayout,
    /// This window's MCP server, its tool calls waiting to run, and the one running.
    mcp: Option<mcp::Server>,
    mcp_queue: VecDeque<mcp::Request>,
    mcp_run: Option<(Sender<ToolResult>, mcp::Collector)>,
    chat: Chat,
    chat_open: bool,
    chat_w: f32,
    completer: Completer,
    completion: Completion,
    /// When open files were last compared with the disk.
    watched: Instant,
}

/// The window layout remembered between runs (`%APPDATA%\matlivre\settings.txt`, one
/// `key=value` per line). A first launch starts with the Files drawer closed.
#[derive(Clone, PartialEq)]
struct SavedLayout {
    files_open: bool,
    cmd_open: bool,
    ws_open: bool,
    files_w: f32,
    ws_w: f32,
    cmd_h: f32,
    output_right: bool,
    dark: bool,
    chat_open: bool,
    chat_w: f32,
    completions: bool,
    /// The project folder shown in the Files panel.
    folder: Option<PathBuf>,
}

impl Default for SavedLayout {
    fn default() -> Self {
        SavedLayout {
            files_open: false,
            cmd_open: true,
            ws_open: true,
            files_w: 248.0,
            ws_w: 300.0,
            cmd_h: 150.0,
            output_right: false,
            dark: false,
            chat_open: false,
            chat_w: 380.0,
            completions: true,
            folder: None,
        }
    }
}

impl SavedLayout {
    fn path() -> Option<PathBuf> {
        app_dir().map(|d| d.join("settings.txt"))
    }

    /// The saved layout; unknown or malformed entries keep their defaults.
    fn load() -> SavedLayout {
        let mut l = SavedLayout::default();
        let Some(text) = SavedLayout::path().and_then(|p| std::fs::read_to_string(p).ok()) else { return l };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let flag = v == "true";
            let size = v.parse::<f32>().ok().filter(|x| x.is_finite() && *x > 0.0);
            match k {
                "files_open" => l.files_open = flag,
                "command_open" => l.cmd_open = flag,
                "workspace_open" => l.ws_open = flag,
                "files_width" => l.files_w = size.unwrap_or(l.files_w),
                "workspace_width" => l.ws_w = size.unwrap_or(l.ws_w),
                "command_height" => l.cmd_h = size.unwrap_or(l.cmd_h),
                "output" => l.output_right = v == "right",
                "theme" => l.dark = v == "dark",
                "claude_open" => l.chat_open = flag,
                "claude_width" => l.chat_w = size.unwrap_or(l.chat_w),
                "completions" => l.completions = flag,
                "folder" => l.folder = Some(PathBuf::from(v)).filter(|p| !v.is_empty() && p.is_dir()),
                _ => {}
            }
        }
        l
    }

    fn save(&self) {
        let Some(path) = SavedLayout::path() else { return };
        let text = format!(
            "files_open={}\ncommand_open={}\nworkspace_open={}\nfiles_width={}\nworkspace_width={}\ncommand_height={}\noutput={}\ntheme={}\nclaude_open={}\nclaude_width={}\ncompletions={}\nfolder={}\n",
            self.files_open,
            self.cmd_open,
            self.ws_open,
            self.files_w.round(),
            self.ws_w.round(),
            self.cmd_h.round(),
            if self.output_right { "right" } else { "inline" },
            if self.dark { "dark" } else { "light" },
            self.chat_open,
            self.chat_w.round(),
            self.completions,
            self.folder.as_deref().map(|p| p.display().to_string()).unwrap_or_default(),
        );
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, text);
    }
}

/// A tab with unsaved changes the app asks about before closing it.
struct Confirm {
    tab: u64,
    /// Closing the window: the tabs whose changes the user chose to drop so far.
    window: Option<HashSet<u64>>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, launch: Launch, place: bool) -> Self {
        crate::theme::install_fonts(&cc.egui_ctx);
        let skin = Skin::light();
        skin.apply(&cc.egui_ctx);
        let window = os::Window::from_handle(cc);
        let rounded = window.is_some_and(|w| {
            if place {
                w.place((DEFAULT_SIZE[0], DEFAULT_SIZE[1]), (MIN_SIZE[0], MIN_SIZE[1]));
            }
            w.use_exe_icon();
            w.round_corners(rgb(skin.frame))
        });
        let incoming = transfer::listen(cc.egui_ctx.clone()).map(|(port, rx)| {
            if let Some(w) = &window {
                w.advertise(port);
            }
            rx
        });
        let layout = SavedLayout::load();
        let tree = FileTree::new(layout.folder.clone());
        let mut app = Self {
            skin,
            tabs: Vec::new(),
            active: 0,
            next_tab: 1,
            engine: Engine::start(cc.egui_ctx.clone()),
            running: false,
            origin: None,
            running_block: None,
            pending: None,
            workspace: Vec::new(),
            output_right: layout.output_right,
            last_run: None,
            last_error: None,
            notice: None,
            drag: None,
            window,
            rounded,
            incoming,
            #[cfg(target_os = "macos")]
            open_files: crate::macos::receive_open_files(&cc.egui_ctx),
            tab_row: Rect::NOTHING,
            files_open: layout.files_open,
            cmd_open: layout.cmd_open,
            ws_open: layout.ws_open,
            files_w: layout.files_w,
            ws_w: layout.ws_w,
            cmd_h: layout.cmd_h,
            tree,
            hist: Vec::new(),
            cmd_input: String::new(),
            cmd_history: Vec::new(),
            cmd_recall: None,
            menu: None,
            submenu_rect: None,
            hover_section: None,
            hover_block: None,
            toolbar_rect: Rect::NOTHING,
            copied: None,
            control_edit: None,
            figures: Vec::new(),
            confirm: None,
            saved_layout: layout.clone(),
            mcp: mcp::start(cc.egui_ctx.clone()),
            mcp_queue: VecDeque::new(),
            mcp_run: None,
            chat: Chat::new(),
            chat_open: layout.chat_open,
            chat_w: layout.chat_w,
            completer: Completer::new(),
            completion: Completion { on: layout.completions, ghost: None, seen: None, edited_at: None, asked: None, next_key: 1, force: false },
            watched: Instant::now(),
        };
        if layout.dark {
            app.set_dark(&cc.egui_ctx, true);
        }
        match launch {
            // Getting started opens once; later launches start with an empty live script (it
            // stays in the … menu).
            Launch::Welcome => {
                if first_launch() {
                    app.getting_started();
                }
            }
            Launch::Open(p) => app.open_path(p),
            Launch::Tab(m) => app.receive(&m),
        }
        if app.tabs.is_empty() {
            app.new_doc(Kind::Live);
        }
        app
    }

    fn layout(&self) -> SavedLayout {
        SavedLayout {
            files_open: self.files_open,
            cmd_open: self.cmd_open,
            ws_open: self.ws_open,
            files_w: self.files_w,
            ws_w: self.ws_w,
            cmd_h: self.cmd_h,
            output_right: self.output_right,
            dark: self.skin.dark,
            chat_open: self.chat_open,
            chat_w: self.chat_w,
            completions: self.completion.on,
            folder: self.tree.root().map(Path::to_path_buf),
        }
    }

    /// Writes the layout when a panel was toggled or resized (once the drag has ended).
    fn remember_layout(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let now = self.layout();
        if now != self.saved_layout {
            now.save();
            self.saved_layout = now;
        }
    }

    fn set_dark(&mut self, ctx: &egui::Context, dark: bool) {
        self.skin = if dark { Skin::dark() } else { Skin::light() };
        self.skin.apply(ctx);
        if let Some(w) = &self.window {
            w.set_border(rgb(self.skin.frame));
        }
    }

    // ---- tabs --------------------------------------------------------------------------

    fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    fn add_tab(&mut self, kind: Kind, doc: Document, path: Option<PathBuf>, name: String, dirty: bool) {
        let id = self.next_tab;
        self.next_tab += 1;
        let disk = path.as_deref().and_then(disk_state);
        self.tabs.push(Tab {
            id,
            kind,
            doc,
            path,
            name,
            dirty,
            outputs: HashMap::new(),
            stale: HashSet::new(),
            current: 0,
            focus_block: None,
            caret: None,
            disk,
            conflict: false,
        });
        self.active = self.tabs.len() - 1;
    }

    /// The first of untitled.mlx, untitled2.mlx, ... (or .m) not already shown in a tab.
    fn untitled_name(&self, kind: Kind) -> String {
        let ext = if kind == Kind::Script { "m" } else { "mlx" };
        (1..)
            .map(|n| if n == 1 { format!("untitled.{ext}") } else { format!("untitled{n}.{ext}") })
            .find(|name| !self.tabs.iter().any(|t| t.path.is_none() && &t.name == name))
            .unwrap()
    }

    fn is_running_tab(&self, id: u64) -> bool {
        matches!(self.origin, Some(Origin::Tab { id: t, .. }) if t == id)
    }

    /// Closes a tab, first asking to save unsaved changes.
    fn close_tab(&mut self, ctx: &egui::Context, i: usize) {
        if self.tabs[i].dirty {
            self.active = i;
            self.confirm = Some(Confirm { tab: self.tabs[i].id, window: None });
            // egui lays a new area out unseen on its first frame; show it on the next.
            ctx.request_repaint();
        } else {
            self.remove_tab(ctx, i);
        }
    }

    /// Closing the window: asks about the next tab with unsaved changes not yet settled, or
    /// closes the window once there is none.
    fn close_window(&mut self, ctx: &egui::Context, dropped: HashSet<u64>) {
        match self.tabs.iter().position(|t| t.dirty && !dropped.contains(&t.id)) {
            Some(i) => {
                self.active = i;
                self.confirm = Some(Confirm { tab: self.tabs[i].id, window: Some(dropped) });
                ctx.request_repaint();
            }
            None => {
                self.tabs.clear();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// The save-changes dialog, while one is up: Save, Don't save, or Cancel.
    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let Some(i) = self.tabs.iter().position(|t| t.id == c.tab) else {
            self.confirm = None;
            return;
        };
        self.menu = None;
        // Keep typing out of the document behind the dialog.
        ctx.memory_mut(|m| {
            if let Some(f) = m.focused() {
                m.surrender_focus(f);
            }
        });
        let title = format!("Save changes to {}?", self.tabs[i].title());
        let buttons = ["Save", "Don't save", "Cancel"];
        let Some(answer) = widgets::content_dialog(ctx, &self.skin, Id::new("confirm"), &title, "Your changes will be lost if you don't save them.", &buttons) else {
            return;
        };
        let Some(c) = self.confirm.take() else { return };
        let go = match answer {
            0 => {
                self.active = i;
                self.save(false)
            }
            1 => true,
            _ => false,
        };
        if !go {
            return;
        }
        match c.window {
            None => self.remove_tab(ctx, i),
            Some(mut dropped) => {
                dropped.insert(c.tab);
                self.close_window(ctx, dropped);
            }
        }
    }

    /// Drops a tab without asking; the window closes with its last tab.
    fn remove_tab(&mut self, ctx: &egui::Context, i: usize) {
        let tab = self.tabs.remove(i);
        if self.is_running_tab(tab.id) {
            self.engine.stop();
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
                let kind = if p.script { Kind::Script } else { Kind::Live };
                let name = if self.tabs.iter().any(|t| t.path.is_none() && t.name == p.name) { self.untitled_name(kind) } else { p.name };
                let doc = if p.script { script_doc(&p.doc.blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n")) } else { p.doc };
                self.add_tab(kind, doc, p.path, name, p.dirty);
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
    /// tab to the MatLivre window under the pointer or to a new one.
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
        if self.is_running_tab(self.tabs[i].id) {
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
        let manifest = match transfer::write(&tab.doc, tab.path.as_deref(), tab.dirty, &tab.name, tab.kind == Kind::Script) {
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
        let script = p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("m"));
        let loaded = if script {
            std::fs::read_to_string(&p).map(|t| script_doc(&t.replace("\r\n", "\n"))).map_err(|e| e.to_string())
        } else {
            Document::load(&p).map_err(|e| e.to_string())
        };
        match loaded {
            Ok(doc) => {
                let kind = if script { Kind::Script } else { Kind::Live };
                if let Some(dir) = p.parent() {
                    self.tree.reveal(dir);
                }
                let reuse = self.tabs.get(self.active).is_some_and(|t| t.is_pristine() && !self.is_running_tab(t.id));
                if reuse {
                    let t = self.tab_mut();
                    t.kind = kind;
                    t.doc = doc;
                    t.disk = disk_state(&p);
                    t.conflict = false;
                    t.path = Some(p);
                    t.outputs.clear();
                    t.stale.clear();
                    t.current = 0;
                } else {
                    self.add_tab(kind, doc, Some(p), String::new(), false);
                }
                self.notice = None;
            }
            Err(e) => self.notice = Some((format!("Could not open {}: {e}", p.display()), true)),
        }
    }

    fn open_folder_dialog(&mut self) {
        let mut d = rfd::FileDialog::new();
        if let Some(r) = self.tree.root() {
            d = d.set_directory(r);
        }
        if let Some(p) = d.pick_folder() {
            self.tree.set_root(Some(p));
        }
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Live scripts and scripts", &["mlx", "m"])
            .add_filter("Live script", &["mlx"])
            .add_filter("Script", &["m"])
            .pick_file()
        {
            self.open_path(p);
        }
    }

    /// Saves the active tab. False if it was not saved (cancelled or failed).
    fn save(&mut self, save_as: bool) -> bool {
        let script = self.tab().kind == Kind::Script;
        let path = match (&self.tab().path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => {
                let d = rfd::FileDialog::new().set_file_name(self.tab().title());
                let d = if script { d.add_filter("Script", &["m"]) } else { d.add_filter("Live script", &["mlx"]).add_filter("Script", &["m"]) };
                d.save_file()
            }
        };
        let Some(p) = path else { return false };
        // Saving over a change made by another program needs the user's choice first.
        if self.tab().path.as_ref() == Some(&p) {
            let changed = match (&self.tab().disk, disk_state(&p)) {
                (Some(known), Some(now)) => disk_change(known, &now) == DiskChange::Changed,
                _ => false,
            };
            if changed || self.tab().conflict {
                self.tab_mut().conflict = true;
                return false;
            }
        }
        let result = if script { std::fs::write(&p, self.tab().script_text()).map_err(|e| e.to_string()) } else { self.tab().doc.save(&p).map_err(|e| e.to_string()) };
        match result {
            Ok(()) => {
                let t = self.tab_mut();
                t.disk = disk_state(&p);
                t.conflict = false;
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

    fn export_m(&mut self) {
        let stem = self.tab().title();
        let stem = stem.rsplit_once('.').map_or(stem.as_str(), |(s, _)| s).to_string();
        if let Some(p) = rfd::FileDialog::new().add_filter("Script", &["m"]).set_file_name(format!("{stem}.m")).save_file() {
            if let Err(e) = std::fs::write(&p, livre_doc::write_m(&self.tab().doc)) {
                self.notice = Some((format!("Could not export: {e}"), true));
            }
        }
    }

    fn new_doc(&mut self, kind: Kind) {
        // An untouched untitled tab gives way to the new one rather than piling up.
        let i = self.active;
        if self.tabs.get(i).is_some_and(|t| t.is_pristine() && !self.is_running_tab(t.id)) {
            self.tabs.remove(i);
        }
        let name = self.untitled_name(kind);
        match kind {
            Kind::Live => {
                let mut doc = Document::new();
                let title = doc.push(BlockKind::Title, "");
                doc.push(BlockKind::Text, "");
                doc.push(BlockKind::Code, "");
                self.add_tab(kind, doc, None, name, false);
                self.tab_mut().focus_block = Some(title);
            }
            Kind::Script => {
                let doc = script_doc("");
                let id = doc.blocks[0].id;
                self.add_tab(kind, doc, None, name, false);
                self.tab_mut().focus_block = Some(id);
            }
        }
    }

    fn getting_started(&mut self) {
        let name = self.untitled_name(Kind::Live);
        self.add_tab(Kind::Live, livre_doc::read_m(WELCOME), None, name, false);
    }

    // ---- running -----------------------------------------------------------------------

    fn start(&mut self, blocks: Vec<(u64, String)>, all_code: Vec<String>, origin: Origin) {
        let folder = self.tab().path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()));
        self.running = true;
        self.origin = Some(origin);
        self.running_block = None;
        self.last_error = None;
        self.notice = None;
        self.engine.send(Command::Run { blocks, all_code, folder });
    }

    fn run_blocks(&mut self, range: Range<usize>) {
        if self.running {
            return;
        }
        let tab = &mut self.tabs[self.active];
        let blocks: Vec<(u64, String)> =
            tab.doc.blocks[range].iter().filter(|b| b.kind == BlockKind::Code).map(|b| (b.id, b.text.clone())).collect();
        if blocks.is_empty() {
            return;
        }
        tab.stale.extend(blocks.iter().map(|(id, _)| *id));
        let all_code = tab.doc.blocks.iter().filter(|b| b.kind == BlockKind::Code).map(|b| b.text.clone()).collect();
        let origin = Origin::Tab { id: tab.id, kind: Kind::Live, line_offset: 0 };
        self.start(blocks, all_code, origin);
    }

    /// Runs lines `range` of a plain script; output goes to the command window.
    fn run_script_lines(&mut self, range: Range<usize>, echo: bool) {
        if self.running {
            return;
        }
        let tab = self.tab();
        let text = tab.script_text();
        let lines: Vec<&str> = text.split('\n').collect();
        let src = lines[range.start.min(lines.len())..range.end.min(lines.len())].join("\n");
        let id = tab.doc.blocks[0].id;
        let origin = Origin::Tab { id: tab.id, kind: Kind::Script, line_offset: range.start };
        let all_code = vec![text.to_string()];
        if echo {
            let stem = tab.title();
            let stem = stem.rsplit_once('.').map_or(stem.as_str(), |(s, _)| s).to_string();
            self.hist.push((format!(">> {stem}"), HistKind::Input));
        }
        self.figures.clear();
        self.start(vec![(id, src)], all_code, origin);
    }

    fn run_section(&mut self, section: usize) {
        match self.tab().kind {
            Kind::Live => {
                if let Some(r) = self.tab().doc.sections().get(section).cloned() {
                    self.run_blocks(r);
                }
            }
            Kind::Script => {
                if let Some(r) = script_sections(self.tab().script_text()).get(section).cloned() {
                    self.run_script_lines(r, false);
                }
            }
        }
    }

    fn run_current_section(&mut self) {
        let s = self.tab().current_section();
        self.run_section(s);
    }

    fn run_and_advance(&mut self) {
        let s = self.tab().current_section();
        self.run_section(s);
        let t = self.tab_mut();
        match t.kind {
            Kind::Live => {
                if let Some(next) = t.doc.sections().get(s + 1) {
                    if next.start < t.doc.blocks.len() {
                        t.current = next.start;
                        t.focus_block = Some(t.doc.blocks[next.start].id);
                    }
                }
            }
            Kind::Script => {
                if let Some(next) = script_sections(t.script_text()).get(s + 1) {
                    t.current = next.start;
                }
            }
        }
    }

    fn run_all(&mut self) {
        if self.running {
            return;
        }
        match self.tab().kind {
            Kind::Live => {
                let n = self.tab().doc.blocks.len();
                self.run_blocks(0..n);
            }
            Kind::Script => {
                let n = self.tab().script_text().split('\n').count();
                self.run_script_lines(0..n, true);
            }
        }
    }

    fn submit_command(&mut self) {
        let c = self.cmd_input.trim().to_string();
        if c.is_empty() || self.running {
            return;
        }
        self.cmd_input.clear();
        self.cmd_recall = None;
        if self.cmd_history.last() != Some(&c) {
            self.cmd_history.push(c.clone());
        }
        if c == "clc" {
            self.hist.clear();
            return;
        }
        self.hist.push((format!(">> {c}"), HistKind::Input));
        self.running = true;
        self.origin = Some(Origin::Command);
        self.engine.send(Command::Eval(c));
    }

    fn push_history(&mut self, text: &str, kind: HistKind) {
        let text = text.trim_end_matches('\n');
        for l in text.split('\n') {
            self.hist.push((l.to_string(), kind));
        }
    }

    fn poll_engine(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.engine.rx.try_recv() {
            let origin = self.origin;
            match ev {
                Event::BlockStarted(id) => self.running_block = Some(id),
                Event::Output(id, out) => {
                    if let Some((_, c)) = &mut self.mcp_run {
                        c.push(&out);
                    }
                    let tab_index = match origin {
                        Some(Origin::Tab { id: t, .. }) => self.tabs.iter().position(|x| x.id == t),
                        _ => None,
                    };
                    if let OutputKind::Error(_) = &out.kind {
                        match (origin, tab_index) {
                            (Some(Origin::Tab { kind: Kind::Live, .. }), Some(ti)) => {
                                let doc = &self.tabs[ti].doc;
                                if let Some(b) = doc.index_of(id) {
                                    self.last_error = Some((doc.section_of(b) + 1, out.line));
                                }
                            }
                            (Some(Origin::Tab { kind: Kind::Script, line_offset, .. }), Some(ti)) => {
                                let line = line_offset + out.line;
                                self.last_error = Some((script_section_of(self.tabs[ti].script_text(), line - 1) + 1, line));
                            }
                            _ => {}
                        }
                    }
                    let to_command = id == COMMAND_ID || matches!(origin, Some(Origin::Tab { kind: Kind::Script, .. }));
                    if to_command {
                        match out.kind {
                            OutputKind::Text(t) => {
                                // Text arrives in pieces; continue an unfinished last line.
                                let mut rest = t.as_str();
                                if let Some((last, HistKind::Output)) = self.hist.last_mut() {
                                    if !last.ends_with('\u{0}') {
                                        if let Some((head, tail)) = rest.split_once('\n') {
                                            last.push_str(head);
                                            last.push('\u{0}');
                                            rest = tail;
                                        } else {
                                            last.push_str(rest);
                                            rest = "";
                                        }
                                    }
                                }
                                if !rest.is_empty() {
                                    let ends = rest.ends_with('\n');
                                    let lines: Vec<&str> = rest.trim_end_matches('\n').split('\n').collect();
                                    let n = lines.len();
                                    for (k, l) in lines.into_iter().enumerate() {
                                        let done = k + 1 < n || ends;
                                        self.hist.push((if done { format!("{l}\u{0}") } else { l.to_string() }, HistKind::Output));
                                    }
                                }
                            }
                            OutputKind::Value { text, .. } => self.push_history(&format!("{text}\u{0}"), HistKind::Output),
                            OutputKind::Warning(w) => self.push_history(&format!("Warning: {w}"), HistKind::Output),
                            OutputKind::Error(e) => self.push_history(&e, HistKind::Error),
                            OutputKind::Figure(f) => match self.figures.iter_mut().find(|(g, _)| g.id == f.id) {
                                Some(slot) => *slot = (f, true),
                                None => self.figures.push((f, true)),
                            },
                        }
                        continue;
                    }
                    // The tab may have been closed while its code ran.
                    let Some(ti) = tab_index else { continue };
                    let tab = &mut self.tabs[ti];
                    if tab.stale.remove(&id) {
                        tab.outputs.remove(&id);
                    }
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
                    if let Some((reply, c)) = self.mcp_run.take() {
                        let _ = reply.send(c.finish(ctx, ok));
                    }
                    self.running_block = None;
                    self.origin = None;
                    if let Some(Origin::Tab { id, .. }) = origin {
                        self.last_run = Some((ok, elapsed));
                        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
                            for b in tab.stale.drain() {
                                tab.outputs.remove(&b);
                            }
                        }
                    }
                    if let Some((tab, section)) = self.pending.take() {
                        if let Some(i) = self.tabs.iter().position(|t| t.id == tab) {
                            let keep = self.active;
                            self.active = i;
                            self.run_section(section);
                            self.active = keep;
                        }
                    }
                }
            }
        }
    }

    // ---- Claude ------------------------------------------------------------------------

    /// Runs code from the Claude panel or a tool call as command-window input.
    fn eval_command(&mut self, code: &str) {
        for (k, l) in code.trim_end().split('\n').enumerate() {
            self.hist.push((format!("{} {l}", if k == 0 { ">>" } else { "  " }), HistKind::Input));
        }
        self.running = true;
        self.origin = Some(Origin::Command);
        self.engine.send(Command::Eval(code.to_string()));
    }

    /// The tab a tool call names (file name, stem or path), or the active one.
    fn find_tab(&self, args: &serde_json::Value) -> Result<usize, String> {
        let Some(name) = args.get("document").and_then(|d| d.as_str()).map(str::trim).filter(|d| !d.is_empty()) else { return Ok(self.active) };
        let same = |a: &str| a.eq_ignore_ascii_case(name);
        self.tabs
            .iter()
            .position(|t| same(&t.title()) || t.path.as_ref().is_some_and(|p| same(&p.display().to_string()) || p.file_stem().is_some_and(|s| same(&s.to_string_lossy()))))
            .ok_or_else(|| format!("No open document is called {name}. Open documents: {}.", self.tabs.iter().map(|t| t.title()).collect::<Vec<_>>().join(", ")))
    }

    fn doc_text(tab: &Tab) -> String {
        match tab.kind {
            Kind::Script => tab.script_text().to_string(),
            Kind::Live => livre_doc::write_m(&tab.doc),
        }
    }

    fn set_doc_text(tab: &mut Tab, text: &str) {
        let text = text.replace("\r\n", "\n");
        match tab.kind {
            Kind::Script => tab.doc.blocks[0].text = text,
            Kind::Live => {
                tab.doc = livre_doc::read_m(&text);
                if tab.doc.blocks.is_empty() {
                    tab.doc.push(BlockKind::Code, "");
                }
                tab.outputs.clear();
                tab.stale.clear();
                tab.current = 0;
                tab.caret = None;
            }
        }
        tab.dirty = true;
    }

    /// Answers a tool call that does not run code.
    fn mcp_tool(&mut self, name: &str, args: &serde_json::Value) -> ToolResult {
        let arg = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        match name {
            "get_workspace" => ToolResult::text(mcp::workspace_text(&self.workspace)),
            "list_documents" => ToolResult::text(
                self.tabs
                    .iter()
                    .enumerate()
                    .map(|(k, t)| {
                        let kind = if t.kind == Kind::Live { "live script" } else { "script" };
                        let path = t.path.as_ref().map_or_else(|| "not saved yet".to_string(), |p| p.display().to_string());
                        let flags = [(k == self.active).then_some("active"), t.dirty.then_some("unsaved changes")].into_iter().flatten().collect::<Vec<_>>().join(", ");
                        format!("{}  ({kind}, {path}){}", t.title(), if flags.is_empty() { String::new() } else { format!("  [{flags}]") })
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            "read_document" => match self.find_tab(args) {
                Ok(i) => ToolResult::text(Self::doc_text(&self.tabs[i])),
                Err(e) => ToolResult::error(e),
            },
            "edit_document" | "write_document" => {
                let i = match self.find_tab(args) {
                    Ok(i) => i,
                    Err(e) => return ToolResult::error(e),
                };
                let title = self.tabs[i].title();
                if self.is_running_tab(self.tabs[i].id) {
                    return ToolResult::error(format!("{title} is running; try again when it has finished."));
                }
                let new = if name == "write_document" {
                    arg("text")
                } else {
                    let (old, new) = (arg("old_text").replace("\r\n", "\n"), arg("new_text").replace("\r\n", "\n"));
                    let text = Self::doc_text(&self.tabs[i]);
                    match if old.is_empty() { 0 } else { text.matches(old.as_str()).count() } {
                        1 => text.replacen(old.as_str(), &new, 1),
                        0 => return ToolResult::error(format!("old_text is not in {title}. Read the document again; it may have changed.")),
                        n => return ToolResult::error(format!("old_text occurs {n} times in {title}; include more of the surrounding text.")),
                    }
                };
                Self::set_doc_text(&mut self.tabs[i], &new);
                self.active = i;
                ToolResult::text(format!("Edited {title} in the editor; it is not saved yet."))
            }
            "open_document" => {
                let p = PathBuf::from(arg("path"));
                let ext = p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
                if !matches!(ext.as_deref(), Some("m") | Some("mlx")) {
                    return ToolResult::error("MatLivre opens .m and .mlx files.");
                }
                if p.exists() {
                    self.open_path(p.clone());
                    if let Some((msg, true)) = &self.notice {
                        return ToolResult::error(msg.clone());
                    }
                } else {
                    let (kind, doc) = if ext.as_deref() == Some("mlx") {
                        let mut d = Document::new();
                        d.push(BlockKind::Code, "");
                        (Kind::Live, d)
                    } else {
                        (Kind::Script, script_doc(""))
                    };
                    self.add_tab(kind, doc, Some(p.clone()), String::new(), true);
                }
                ToolResult::text(format!("Opened {}.", p.display()))
            }
            _ => ToolResult::error(format!("Unknown tool {name}.")),
        }
    }

    /// Starts a tool call that runs code; its result goes out when the run finishes.
    fn mcp_start(&mut self, r: mcp::Request) {
        if r.tool == "run_code" {
            let code = r.args.get("code").and_then(|c| c.as_str()).unwrap_or("").to_string();
            if code.trim().is_empty() {
                let _ = r.reply.send(ToolResult::error("No code to run."));
                return;
            }
            self.cmd_open = true;
            self.eval_command(&code);
        } else {
            let i = match self.find_tab(&r.args) {
                Ok(i) => i,
                Err(e) => {
                    let _ = r.reply.send(ToolResult::error(e));
                    return;
                }
            };
            self.active = i;
            match r.args.get("section").and_then(|s| s.as_u64()) {
                Some(n) if n >= 1 && (n as usize) <= self.tab().section_count() => self.run_section(n as usize - 1),
                Some(n) => {
                    let _ = r.reply.send(ToolResult::error(format!("{} has {} sections; there is no section {n}.", self.tab().title(), self.tab().section_count())));
                    return;
                }
                None => self.run_all(),
            }
            if !self.running {
                let _ = r.reply.send(ToolResult::text("Nothing to run."));
                return;
            }
        }
        self.mcp_run = Some((r.reply, mcp::Collector::default()));
    }

    /// Tool calls from the MCP server: runs wait their turn, the rest are answered now.
    fn serve_mcp(&mut self) {
        if let Some(s) = &self.mcp {
            self.mcp_queue.extend(s.rx.try_iter());
        }
        let mut waiting = VecDeque::new();
        while let Some(r) = self.mcp_queue.pop_front() {
            match r.tool.as_str() {
                mcp::APPROVE => {
                    self.chat_open = true;
                    self.chat.ask(r);
                }
                "run_code" | "run_document" => {
                    if self.running || self.mcp_run.is_some() || !waiting.is_empty() {
                        waiting.push_back(r);
                    } else {
                        self.mcp_start(r);
                    }
                }
                _ => {
                    let res = self.mcp_tool(&r.tool, &r.args);
                    let _ = r.reply.send(res);
                }
            }
        }
        self.mcp_queue = waiting;
    }

    fn chat_send(&mut self, ctx: &egui::Context, text: String) {
        let Some(url) = self.mcp.as_ref().map(|m| m.dock_url.clone()) else {
            self.chat.items.push(chat::Item::Error("MatLivre could not start its MCP server, which Claude needs.".into()));
            return;
        };
        let t = self.tab();
        let cwd = t.path.as_ref().and_then(|p| p.parent().map(Path::to_path_buf)).or_else(|| std::env::current_dir().ok()).unwrap_or_else(std::env::temp_dir);
        let path = t.path.as_ref().map_or_else(|| "not saved yet".to_string(), |p| p.display().to_string());
        let note = format!(
            "The active document is {} ({}, {path}{}), section {} of {}.",
            t.title(),
            if t.kind == Kind::Live { "live script" } else { "script" },
            if t.dirty { ", unsaved changes" } else { "" },
            t.current_section() + 1,
            t.section_count()
        );
        self.chat.send(text, &note, &url, &cwd, ctx);
    }

    fn chat_panel(&mut self, ui: &mut Ui, rect: Rect) {
        let skin = self.skin.clone();
        let mut ui = Self::card(ui, &skin, rect);
        let mut clear = false;
        let has_items = !self.chat.items.is_empty();
        Self::drawer_header(&mut ui, &skin, Some(Ic::Chat), "Claude", |ui| {
            if has_items && widgets::tool_button(ui, &skin, Ic::New, 16.0, "New conversation").clicked() {
                clear = true;
            }
        });
        if clear {
            self.chat.clear();
        }
        let area = ui.available_rect_before_wrap();
        let body = Rect::from_min_max(area.min + vec2(12.0, 4.0), area.max - vec2(12.0, 12.0));
        if self.chat.exe().is_none() {
            let mut c = ui.new_child(UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
            c.spacing_mut().item_spacing.y = 6.0;
            c.add(egui::Label::new(RichText::new("Claude Code is not installed on this computer.").color(skin.text)).wrap());
            c.add(egui::Label::new(RichText::new("Install it from claude.com/claude-code and run claude once in a terminal to sign in. The panel uses that installation; no API key is needed.").size(12.0).color(skin.text_secondary)).wrap());
            return;
        }
        let composer_h = 92.0;
        let list = Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - composer_h - 8.0));
        let bottom = Rect::from_min_max(pos2(body.left(), body.bottom() - composer_h), body.max);
        let mut l = ui.new_child(UiBuilder::new().max_rect(list).layout(Layout::top_down(Align::Min)));
        l.set_clip_rect(list);
        let mut actions = Vec::new();
        egui::ScrollArea::vertical().id_salt("chat").auto_shrink([false, false]).stick_to_bottom(true).show(&mut l, |ui| {
            actions = chat::transcript(ui, &skin, &mut self.chat);
            ui.add_space(4.0);
        });
        let mut b = ui.new_child(UiBuilder::new().max_rect(bottom).layout(Layout::top_down(Align::Min)));
        if let Some(a) = chat::composer(&mut b, &skin, &mut self.chat, Id::new("chat-input")) {
            actions.push(a);
        }
        for a in actions {
            match a {
                chat::Action::Send(text) => self.chat_send(ui.ctx(), text),
                chat::Action::Run(code) => {
                    if self.running {
                        self.notice = Some(("Wait for the current run to finish.".into(), false));
                    } else {
                        self.cmd_open = true;
                        self.eval_command(&code);
                    }
                }
            }
        }
    }

    // ---- code completion ---------------------------------------------------------------

    /// The text before and after the caret (a live script's other code blocks included) and
    /// the document's name.
    fn completion_context(&self, c: Caret) -> Option<(String, String, String)> {
        let t = self.tabs.iter().find(|t| t.doc.blocks.iter().any(|b| b.id == c.block))?;
        let mut before = String::new();
        let mut after = String::new();
        let mut seen = false;
        for b in t.doc.blocks.iter().filter(|b| b.kind == BlockKind::Code) {
            if b.id == c.block {
                let at = b.text.char_indices().nth(c.index).map_or(b.text.len(), |(i, _)| i);
                before.push_str(&b.text[..at]);
                after.push_str(&b.text[at..]);
                seen = true;
            } else if seen {
                after.push_str("\n\n");
                after.push_str(&b.text);
            } else {
                before.push_str(&b.text);
                before.push_str("\n\n");
            }
        }
        Some((before, after, t.title()))
    }

    /// Asks for a completion after a pause in typing, and shows the answer while the caret
    /// and the text stay as they were when it was asked.
    fn complete(&mut self, ctx: &egui::Context, caret: Option<Caret>) {
        if let Some((key, text)) = self.completer.poll() {
            if let Some((k, at, before, after)) = self.completion.asked.take() {
                if k == key && Some(at) == self.completion.seen {
                    let text = complete::clean(&text, &before, &after);
                    if !text.is_empty() {
                        self.completion.ghost = Some(Ghost { block: at.block, at: at.index, text });
                    }
                } else {
                    self.completion.asked = Some((k, at, before, after));
                }
            }
        }
        let c = &mut self.completion;
        if caret != c.seen {
            let typed = matches!((caret, c.seen), (Some(a), Some(b)) if a.block == b.block && a.text != b.text);
            c.ghost = None;
            if c.asked.take().is_some() {
                self.completer.cancel();
            }
            c.seen = caret;
            c.edited_at = typed.then(Instant::now);
            if typed && c.on {
                self.completer.warm_up();
            }
        }
        let c = &mut self.completion;
        let Some(at) = caret else {
            c.force = false;
            return;
        };
        let due = c.force || (c.on && c.edited_at.is_some_and(|t| t.elapsed() >= COMPLETION_PAUSE));
        if let (Some(t), false) = (c.edited_at, due) {
            ctx.request_repaint_after(COMPLETION_PAUSE.saturating_sub(t.elapsed()));
        }
        if !due || c.asked.is_some() || c.ghost.is_some() {
            return;
        }
        let force = std::mem::take(&mut c.force);
        c.edited_at = None;
        let Some((before, after, file)) = self.completion_context(at) else { return };
        if !force && !wants_completion(&before, &after) {
            return;
        }
        let key = self.completion.next_key;
        self.completion.next_key += 1;
        let vars: Vec<String> = self.workspace.iter().map(|v| v.name.clone()).collect();
        self.completer.request(key, complete::prompt(&file, &before, &after, &vars), ctx);
        self.completion.asked = Some((key, at, before, after));
    }

    // ---- files on disk -----------------------------------------------------------------

    /// Puts the file's current content in tab `i`, which then has no unsaved changes.
    fn reload(&mut self, i: usize, now: Disk) {
        let running = self.is_running_tab(self.tabs[i].id);
        let t = &mut self.tabs[i];
        let Some(path) = t.path.clone() else { return };
        match t.kind {
            Kind::Script => {
                let Ok(bytes) = std::fs::read(&path) else { return };
                // The block keeps its id, so the editor keeps its caret and undo history.
                t.doc.blocks[0].text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
            }
            Kind::Live => {
                if running {
                    return;
                }
                let Ok(doc) = Document::load(&path) else { return };
                t.doc = doc;
                if t.doc.blocks.is_empty() {
                    t.doc.push(BlockKind::Code, "");
                }
                t.outputs.clear();
                t.stale.clear();
                t.current = t.current.min(t.doc.blocks.len() - 1);
                t.caret = None;
            }
        }
        t.disk = Some(now);
        t.dirty = false;
        t.conflict = false;
    }

    /// Once a second, compares the open files with the disk, as VS Code and MATLAB do: a tab
    /// without unsaved changes takes the new version; one with changes asks which to keep.
    fn watch_files(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_secs(1));
        if self.watched.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.watched = Instant::now();
        for i in 0..self.tabs.len() {
            let Some(path) = self.tabs[i].path.clone() else { continue };
            // A file that is gone (or being replaced) keeps the last known state.
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            let Some(known) = self.tabs[i].disk.clone() else {
                self.tabs[i].disk = disk_state(&path);
                continue;
            };
            if meta.modified().ok() == known.modified && meta.len() == known.len {
                continue;
            }
            let Some(now) = disk_state(&path) else { continue };
            match disk_change(&known, &now) {
                DiskChange::None => {}
                DiskChange::Touched => self.tabs[i].disk = Some(now),
                DiskChange::Changed => {
                    if self.tabs[i].dirty {
                        self.tabs[i].conflict = true;
                    } else {
                        self.reload(i, now);
                    }
                }
            }
        }
    }

    // ---- editing -----------------------------------------------------------------------

    fn insert_block(&mut self, kind: BlockKind) {
        if self.tab().kind != Kind::Live {
            return;
        }
        let at = (self.tab().current + 1).min(self.tab().doc.blocks.len());
        self.apply(vec![Action::Insert { at, kind }]);
    }

    /// Puts a live control at the caret of the focused code block: over the selected literal,
    /// or over a default literal typed at the caret.
    fn insert_control(&mut self, tag: ControlTag) {
        let t = &mut self.tabs[self.active];
        let target = t.caret.and_then(|(id, range)| t.doc.index_of(id).map(|i| (i, range))).filter(|(i, _)| t.doc.blocks[*i].kind == BlockKind::Code);
        let Some((bi, range)) = target else {
            self.notice = Some(("Click in a code block where the control should go, then insert it.".into(), false));
            return;
        };
        let block = &mut t.doc.blocks[bi];
        if tag == ControlTag::Button {
            // A button goes on an empty line of its own, after the caret's line.
            let line = line_col(&block.text, range.primary.index).0;
            let line_text = block.text.split('\n').nth(line).unwrap_or("");
            let target = if line_text.trim().is_empty() {
                line
            } else {
                let old = block.text.clone();
                let at = char_index(&block.text, line, usize::MAX);
                let mut chars: Vec<char> = block.text.chars().collect();
                chars.insert(at, '\n');
                block.text = chars.into_iter().collect();
                block.reanchor_controls(&old);
                line + 1
            };
            let kind = ControlKind::Button { label: "Run".into() };
            let fields = control_fields(&kind);
            if t.doc.add_control(bi, target, 0, 0, kind) {
                t.dirty = true;
                let block = &t.doc.blocks[bi];
                if let Some(index) = block.controls.iter().position(|c| c.line == target && c.start == 0) {
                    self.control_edit = Some(ControlEdit { block: block.id, index, at: None, fields });
                }
            }
            return;
        }
        let [a, b] = range.sorted();
        let (a, b) = (a.index.min(block.text.chars().count()), b.index.min(block.text.chars().count()));
        let (line, col) = line_col(&block.text, a);
        let (line_b, col_b) = line_col(&block.text, b);
        let selected: String = block.text.chars().skip(a).take(b - a).collect();
        let selected = if line_b == line { selected.trim().to_string() } else { String::new() };
        let (literal, kind) = match tag {
            ControlTag::Slider => {
                let v = selected.parse::<f64>().ok();
                let v0 = v.unwrap_or(5.0);
                let max = if v0 > 10.0 { (v0 * 2.0).ceil() } else { 10.0 };
                let min = v0.min(0.0);
                (v.map_or("5".to_string(), |_| selected.clone()), ControlKind::Slider { min, max, step: if max - min <= 20.0 { 0.5 } else { 1.0 } })
            }
            ControlTag::DropDown => {
                let lit = if selected.is_empty() { "'one'".to_string() } else { selected.clone() };
                let mut items = vec![lit.clone()];
                for d in ["'one'", "'two'", "'three'"] {
                    if !items.iter().any(|i| i == d) {
                        items.push(d.to_string());
                    }
                }
                (lit, ControlKind::DropDown { labels: items.clone(), items })
            }
            ControlTag::CheckBox => {
                let lit = if selected == "true" || selected == "false" { selected.clone() } else { "true".into() };
                (lit, ControlKind::CheckBox)
            }
            ControlTag::EditField => (if selected.is_empty() { "0".into() } else { selected.clone() }, ControlKind::EditField),
            ControlTag::Button => unreachable!("buttons are placed above"),
        };
        let reuse = !selected.is_empty() && literal == selected;
        let (start, end) = if reuse {
            // The trimmed selection's columns.
            let line_text: Vec<char> = block.text.split('\n').nth(line).unwrap_or("").chars().collect();
            let lead = line_text[col..col_b.min(line_text.len())].iter().take_while(|c| c.is_whitespace()).count();
            (col + lead, col + lead + literal.chars().count())
        } else {
            // Replace the selection (if any, on one line) with the default literal.
            let old = block.text.clone();
            let (from, to) = if line_b == line { (a, b) } else { (a, a) };
            let mut chars: Vec<char> = block.text.chars().collect();
            chars.splice(from..to, literal.chars());
            block.text = chars.into_iter().collect();
            block.reanchor_controls(&old);
            let col = line_col(&block.text, from).1;
            (col, col + literal.chars().count())
        };
        let fields = control_fields(&kind);
        let configurable = !matches!(kind, ControlKind::CheckBox | ControlKind::EditField);
        if t.doc.add_control(bi, line, start, end, kind) {
            t.dirty = true;
            let block = &t.doc.blocks[bi];
            // Drop-downs, sliders and buttons open their settings, since the defaults rarely fit.
            if let Some(index) = block.controls.iter().position(|c| c.line == line && c.start == start).filter(|_| configurable) {
                self.control_edit = Some(ControlEdit { block: block.id, index, at: None, fields });
            }
        } else {
            self.notice = Some(("A control cannot go there: it would overlap another control.".into(), false));
        }
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
        // The save-changes dialog takes the keyboard while it is up.
        if self.confirm.is_some() {
            return;
        }
        if self.completion.ghost.is_some() && pressed(sc(Modifiers::NONE, Key::Escape)) {
            self.completion.ghost = None;
        }
        if pressed(sc(Modifiers::ALT, Key::Backslash)) {
            self.completion.force = true;
        }
        if pressed(sc(Modifiers::COMMAND, Key::L)) {
            self.chat_open = !self.chat_open;
            if self.chat_open {
                ctx.memory_mut(|m| m.request_focus(Id::new("chat-input")));
            }
        }
        if (self.menu.is_some() || self.control_edit.is_some()) && pressed(sc(Modifiers::NONE, Key::Escape)) {
            self.menu = None;
            self.control_edit = None;
        }
        if pressed(sc(Modifiers::COMMAND | Modifiers::ALT, Key::Enter)) {
            self.insert_block(BlockKind::SectionBreak);
        }
        if pressed(sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::Enter)) {
            self.run_and_advance();
        }
        if pressed(sc(Modifiers::COMMAND, Key::Enter)) {
            self.run_current_section();
        }
        if pressed(sc(Modifiers::NONE, Key::F5)) {
            self.run_all();
        }
        if pressed(sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)) {
            self.save(true);
        }
        if pressed(sc(Modifiers::COMMAND, Key::S)) {
            self.save(false);
        }
        if pressed(sc(Modifiers::COMMAND, Key::O)) {
            self.open_dialog();
        }
        if pressed(sc(Modifiers::COMMAND | Modifiers::ALT, Key::N)) {
            self.new_doc(Kind::Script);
        }
        if pressed(sc(Modifiers::COMMAND, Key::N)) {
            self.new_doc(Kind::Live);
        }
        if pressed(sc(Modifiers::CTRL, Key::Backtick)) {
            self.cmd_open = !self.cmd_open;
        }
        if pressed(sc(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab)) {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
        if pressed(sc(Modifiers::CTRL, Key::Tab)) {
            self.active = (self.active + 1) % self.tabs.len();
        }
        if pressed(sc(Modifiers::COMMAND, Key::W)) {
            self.close_tab(ctx, self.active);
        }
        if self.running && pressed(sc(Modifiers::COMMAND, if cfg!(target_os = "macos") { Key::Period } else { Key::C })) && !ctx.wants_keyboard_input() {
            self.engine.stop();
        }
    }

    fn run_menu(&mut self, ctx: &egui::Context, cmd: MenuCmd) {
        match cmd {
            MenuCmd::NewLive => self.new_doc(Kind::Live),
            MenuCmd::NewScript => self.new_doc(Kind::Script),
            MenuCmd::Save => {
                self.save(false);
            }
            MenuCmd::SaveAs => {
                self.save(true);
            }
            MenuCmd::ExportM => self.export_m(),
            MenuCmd::Insert(kind) => self.insert_block(kind),
            MenuCmd::Control(tag) => self.insert_control(tag),
            MenuCmd::ClearOutput => {
                let t = self.tab_mut();
                t.outputs.clear();
                t.stale.clear();
            }
            MenuCmd::ToggleTheme => {
                let dark = !self.skin.dark;
                self.set_dark(ctx, dark);
            }
            MenuCmd::ToggleCompletions => {
                self.completion.on = !self.completion.on;
                self.completion.ghost = None;
            }
            MenuCmd::ConnectClaude => {
                let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "matlivre".into());
                ctx.copy_text(format!("claude mcp add matlivre -- \"{exe}\" --mcp"));
                self.notice = Some((
                    "Copied a command that adds MatLivre to Claude Code. Run it once in a terminal: Claude Code then works in this window's workspace and documents, or in a MatLivre without a window when none is open.".into(),
                    false,
                ));
            }
            MenuCmd::GettingStarted => self.getting_started(),
        }
    }

    // ---- chrome ------------------------------------------------------------------------

    /// The title bar: app mark, the document tabs (TabStrip) and the caption buttons.
    fn title_bar(&mut self, ctx: &egui::Context) {
        let skin = self.skin.clone();
        let mut select = None;
        let mut close = None;
        let mut add = false;
        let mut rects = Vec::new();
        egui::TopBottomPanel::top("title")
            .exact_height(TITLE_BAR_HEIGHT)
            .frame(egui::Frame::none().fill(skin.window))
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(12.0);
                    let start = ui.cursor().left();
                    if cfg!(target_os = "macos") {
                        // Room for the native traffic lights.
                        ui.add_space(72.0);
                    } else {
                        app_logo(ui, &skin);
                        ui.add_space(10.0);
                        // The program name in the Windows caption style (12 px).
                        ui.label(RichText::new("MatLivre").size(12.0).color(skin.text));
                        ui.add_space(16.0);
                    }
                    // Like the empty part of the bar, the icon and name move the window.
                    let mark = Rect::from_x_y_ranges(start..=ui.cursor().left(), ui.max_rect().y_range());
                    let resp = ui.interact(mark, ui.id().with("mark-drag"), Sense::click_and_drag());
                    if resp.double_clicked() {
                        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    } else if resp.drag_started_by(egui::PointerButton::Primary) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    // Tabs take their natural width, and shrink alike when they do not fit
                    // beside the + button, a grab area and the caption buttons.
                    let room = (ui.available_width() - 34.0 - 120.0 - if cfg!(target_os = "macos") { 0.0 } else { 3.0 * 46.0 }).max(0.0);
                    let natural: Vec<f32> = self
                        .tabs
                        .iter()
                        .map(|t| {
                            let busy = if self.is_running_tab(t.id) { 14.0 } else { 0.0 };
                            let w = ui.fonts(|f| f.layout_no_wrap(t.title(), FontId::proportional(14.0), Color32::PLACEHOLDER).size().x);
                            (12.0 + busy + w + 4.0 + 20.0 + 6.0).clamp(96.0, 240.0)
                        })
                        .collect();
                    let total: f32 = natural.iter().map(|w| w + 4.0).sum();
                    let fit = (room / self.tabs.len() as f32 - 4.0).max(48.0);
                    let row_left = ui.cursor().left();
                    for (k, t) in self.tabs.iter().enumerate() {
                        let width = if total > room { fit.min(natural[k]) } else { natural[k] };
                        let tip = t.path.as_ref().map_or_else(|| t.title(), |p| p.display().to_string());
                        let dragging = self.drag.as_ref().is_some_and(|d| d.id == t.id);
                        let r = widgets::title_tab(
                            ui,
                            &skin,
                            Id::new(("tab", t.id)),
                            &t.title(),
                            &tip,
                            width,
                            k == self.active || dragging,
                            t.dirty,
                            self.is_running_tab(t.id),
                        );
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
                    if widgets::add_tab_button(ui, &skin).on_hover_text(crate::os::keys("New live script (Ctrl+N)")).clicked() {
                        add = true;
                    }
                    self.tab_row = Rect::from_x_y_ranges(row_left..=ui.cursor().left(), ui.max_rect().y_range());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        if !cfg!(target_os = "macos") {
                        if widgets::caption_button(ui, &skin, Caption::Close).on_hover_text("Close").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        let (mk, tip) = if maximized { (Caption::Restore, "Restore") } else { (Caption::Maximize, "Maximize") };
                        if widgets::caption_button(ui, &skin, mk).on_hover_text(tip).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        }
                        if widgets::caption_button(ui, &skin, Caption::Minimize).on_hover_text("Minimize").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
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
            self.new_doc(Kind::Live);
        }
        if let Some(k) = close {
            self.close_tab(ctx, k);
            return;
        }
        self.drag_tabs(ctx, &rects);
    }

    fn toggle_menu(&mut self, m: Menu, button: &egui::Response) {
        if button.clicked() {
            self.menu = if self.menu.is_some_and(|(open, _)| open == m) { None } else { Some((m, button.rect)) };
            self.submenu_rect = None;
        }
    }

    /// The command bar: every command is a transparent button whose colour icon carries its
    /// meaning.
    fn command_bar(&mut self, ctx: &egui::Context) {
        let skin = self.skin.clone();
        // Narrow windows drop labels (tooltips still name every command): first on the run
        // commands, then on the file commands.
        let room = ctx.screen_rect().width() - 24.0;
        let (short_run, short_file) = (room < 1220.0, room < 900.0);
        let run_label = |s: &'static str| if short_run { "" } else { s };
        let file_label = |s: &'static str| if short_file { "" } else { s };
        let live = self.tab().kind == Kind::Live;
        egui::TopBottomPanel::top("commands")
            .exact_height(48.0)
            .frame(egui::Frame::none().fill(skin.window).inner_margin(Margin { left: 12.0, right: 12.0, top: 0.0, bottom: 4.0 }))
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let open = |m: Menu| self.menu.is_some_and(|(o, _)| o == m);
                    let r = widgets::cmd_button(ui, &skin, Some(Ic::New), file_label("New"), true, true, open(Menu::New)).on_hover_text("New live script or script");
                    self.toggle_menu(Menu::New, &r);
                    if widgets::cmd_button(ui, &skin, Some(Ic::Open), file_label("Open"), true, false, false).on_hover_text(crate::os::keys("Open .mlx or .m (Ctrl+O)")).clicked() {
                        self.menu = None;
                        self.open_dialog();
                    }
                    let open = |m: Menu| self.menu.is_some_and(|(o, _)| o == m);
                    let r = widgets::cmd_button(ui, &skin, Some(Ic::Save), file_label("Save"), true, true, open(Menu::Save)).on_hover_text(crate::os::keys("Save (Ctrl+S), Save as, Export"));
                    self.toggle_menu(Menu::Save, &r);
                    if live {
                        widgets::bar_divider(ui, &skin);
                        let open = |m: Menu| self.menu.is_some_and(|(o, _)| o == m);
                        let r = widgets::cmd_button(ui, &skin, Some(Ic::Add), file_label("Insert"), true, true, open(Menu::Insert))
                            .on_hover_text("Insert text, code, a heading or a section break");
                        self.toggle_menu(Menu::Insert, &r);
                    }
                    widgets::bar_divider(ui, &skin);
                    let idle = !self.running;
                    if widgets::cmd_button(ui, &skin, Some(Ic::Run), run_label("Run section"), idle, false, false).on_hover_text(crate::os::keys("Run the current section (Ctrl+Enter)")).clicked() {
                        self.run_current_section();
                    }
                    if widgets::cmd_button(ui, &skin, Some(Ic::Advance), run_label("Run and advance"), idle, false, false)
                        .on_hover_text(crate::os::keys("Run the section and move to the next (Ctrl+Shift+Enter)"))
                        .clicked()
                    {
                        self.run_and_advance();
                    }
                    if widgets::cmd_button(ui, &skin, Some(Ic::RunAll), run_label("Run all"), idle, false, false).on_hover_text("Run the whole script (F5)").clicked() {
                        self.run_all();
                    }
                    if widgets::cmd_button(ui, &skin, Some(Ic::Stop), run_label("Stop"), self.running, false, false).on_hover_text(crate::os::keys("Stop execution (Ctrl+C)")).clicked() {
                        self.engine.stop();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let more = more_button(ui, &skin, self.menu.is_some_and(|(o, _)| o == Menu::More)).on_hover_text("More");
                        self.toggle_menu(Menu::More, &more);
                        if widgets::cmd_button(ui, &skin, Some(Ic::Chat), "", true, false, self.chat_open).on_hover_text(crate::os::keys("Show or hide Claude (Ctrl+L)")).clicked() {
                            self.chat_open = !self.chat_open;
                        }
                        if widgets::cmd_button(ui, &skin, Some(Ic::Workspace), "", true, false, self.ws_open).on_hover_text("Show or hide the workspace").clicked() {
                            self.ws_open = !self.ws_open;
                        }
                        if widgets::cmd_button(ui, &skin, Some(Ic::CmdPanel), "", true, false, self.cmd_open)
                            .on_hover_text(crate::os::keys("Show or hide the command window (Ctrl+`)"))
                            .clicked()
                        {
                            self.cmd_open = !self.cmd_open;
                        }
                        if widgets::cmd_button(ui, &skin, Some(Ic::FilesPanel), "", true, false, self.files_open).on_hover_text("Show or hide files").clicked() {
                            self.files_open = !self.files_open;
                        }
                        if live {
                            widgets::bar_divider(ui, &skin);
                            if let Some(i) = widgets::segmented(ui, &skin, &["Inline", "Right"], usize::from(self.output_right)) {
                                self.output_right = i == 1;
                            }
                            if !short_file {
                                ui.label(RichText::new("Output").size(12.0).color(skin.text_secondary));
                            }
                        }
                    });
                });
            });
    }

    /// The open drop-down menu, 36 px below its button (the overflow menu right-aligned).
    fn menus(&mut self, ctx: &egui::Context) {
        let Some((m, anchor)) = self.menu else { return };
        let skin = self.skin.clone();
        let live = self.tab().kind == Kind::Live;
        let dark = skin.dark;
        let width = if m == Menu::Insert { 300.0 } else { 240.0 };
        let pos = if m == Menu::More { pos2(anchor.right() - width, anchor.top() + 36.0) } else { pos2(anchor.left(), anchor.top() + 36.0) };
        let mut chosen = None;
        let mut control_item = None;
        let mut submenu_hot = false;
        let rect = widgets::menu_flyout(ctx, &skin, Id::new("menu").with(m as u8), pos, width, |ui| {
            let mut item = |ui: &mut Ui, e: MenuEntry, cmd: MenuCmd| {
                if widgets::menu_item(ui, &skin, e, false).clicked() {
                    chosen = Some(cmd);
                }
            };
            match m {
                Menu::New => {
                    item(ui, MenuEntry::new(Ic::Mlx, "Live script (.mlx)").shortcut(&crate::os::keys("Ctrl+N")), MenuCmd::NewLive);
                    item(ui, MenuEntry::new(Ic::MFile, "Script (.m)").shortcut(&crate::os::keys("Ctrl+Alt+N")), MenuCmd::NewScript);
                }
                Menu::Save => {
                    item(ui, MenuEntry::new(Ic::Save, "Save").shortcut(&crate::os::keys("Ctrl+S")), MenuCmd::Save);
                    item(ui, MenuEntry::new(Ic::SaveAs, "Save as…").shortcut(&crate::os::keys("Ctrl+Shift+S")), MenuCmd::SaveAs);
                    widgets::menu_separator(ui, &skin);
                    item(ui, MenuEntry::new(Ic::ExportM, "Export as script (.m)…").enabled(live), MenuCmd::ExportM);
                    let pdf = widgets::menu_item(ui, &skin, MenuEntry::new(Ic::Pdf, "Export as PDF…").enabled(false), false);
                    pdf.on_hover_text("PDF export is not available yet.");
                }
                Menu::Insert => {
                    item(ui, MenuEntry::new(Ic::Text, "Text"), MenuCmd::Insert(BlockKind::Text));
                    item(ui, MenuEntry::new(Ic::Code, "Code"), MenuCmd::Insert(BlockKind::Code));
                    widgets::menu_separator(ui, &skin);
                    item(ui, MenuEntry::new(Ic::Title, "Title"), MenuCmd::Insert(BlockKind::Title));
                    item(ui, MenuEntry::new(Ic::Heading, "Heading"), MenuCmd::Insert(BlockKind::Heading));
                    item(ui, MenuEntry::new(Ic::Subheading, "Subheading"), MenuCmd::Insert(BlockKind::Heading2));
                    widgets::menu_separator(ui, &skin);
                    let hot = self.submenu_rect.is_some();
                    let r = widgets::menu_item(ui, &skin, MenuEntry::new(Ic::Control, "Control").submenu(), hot);
                    if r.hovered() || r.clicked() {
                        submenu_hot = true;
                    }
                    control_item = Some(r.rect);
                    widgets::menu_separator(ui, &skin);
                    item(ui, MenuEntry::new(Ic::SectionBreak, "Section break").shortcut(&crate::os::keys("Ctrl+Alt+Enter")), MenuCmd::Insert(BlockKind::SectionBreak));
                }
                Menu::More => {
                    item(ui, MenuEntry::new(Ic::ClearOutput, "Clear all output"), MenuCmd::ClearOutput);
                    widgets::menu_separator(ui, &skin);
                    item(ui, MenuEntry::new(Ic::Theme, "Dark theme").checked(dark), MenuCmd::ToggleTheme);
                    widgets::menu_separator(ui, &skin);
                    item(ui, MenuEntry::new(Ic::Code, "Code completions").checked(self.completion.on).shortcut("Alt+\\"), MenuCmd::ToggleCompletions);
                    item(ui, MenuEntry::new(Ic::Chat, "Connect Claude Code…"), MenuCmd::ConnectClaude);
                    widgets::menu_separator(ui, &skin);
                    item(ui, MenuEntry::new(Ic::Help, "Getting started"), MenuCmd::GettingStarted);
                }
            }
        });
        // The Control › submenu stays open while the pointer is on its item or inside it.
        let pointer = ctx.pointer_hover_pos();
        let in_sub = self.submenu_rect.zip(pointer).is_some_and(|(r, p)| r.contains(p));
        let other_item = pointer.is_some_and(|p| rect.contains(p)) && !submenu_hot;
        if let (Some(item), true) = (control_item, submenu_hot || (self.submenu_rect.is_some() && (in_sub || !other_item))) {
            let r = widgets::menu_flyout(ctx, &skin, Id::new("submenu"), pos2(item.right() - 2.0, item.top() - 5.0), 240.0, |ui| {
                for (ic, label, tag) in [
                    (Ic::Slider, "Slider", ControlTag::Slider),
                    (Ic::Dropdown, "Drop-down", ControlTag::DropDown),
                    (Ic::Checkbox, "Check box", ControlTag::CheckBox),
                    (Ic::EditField, "Edit field", ControlTag::EditField),
                    (Ic::ButtonCtl, "Button", ControlTag::Button),
                ] {
                    if widgets::menu_item(ui, &skin, MenuEntry::new(ic, label), false).clicked() {
                        chosen = Some(MenuCmd::Control(tag));
                    }
                }
            });
            self.submenu_rect = Some(r);
        } else {
            self.submenu_rect = None;
        }
        let outside = ctx.input(|i| i.pointer.any_pressed())
            && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !rect.contains(p) && !anchor.contains(p) && !self.submenu_rect.is_some_and(|r| r.contains(p)));
        if let Some(cmd) = chosen {
            self.menu = None;
            self.submenu_rect = None;
            self.run_menu(ctx, cmd);
        } else if outside {
            self.menu = None;
            self.submenu_rect = None;
        }
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let skin = self.skin.clone();
        egui::TopBottomPanel::bottom("status")
            .exact_height(28.0)
            .frame(egui::Frame::none().fill(skin.window).inner_margin(Margin::symmetric(12.0, 0.0)))
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let small = |t: &str, c: Color32| RichText::new(t).size(12.0).color(c);
                    if self.running {
                        widgets::progress_ring(ui, &skin, 16.0);
                        let text = match self.origin {
                            Some(Origin::Command) => "Running command…".to_string(),
                            _ => format!("Running section {}…", self.running_section().map_or(1, |s| s + 1)),
                        };
                        ui.label(small(&text, skin.text));
                    } else if let (Some((false, _)), Some((s, l))) = (self.last_run, self.last_error) {
                        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                        widgets::error_glyph(ui.painter(), &skin, r.center());
                        ui.label(small(&format!("Stopped with an error in section {s}, line {l}"), skin.text));
                    } else {
                        let text = match self.last_run {
                            Some((true, d)) => format!("Ready  ·  Last run completed in {:.2} s", d.as_secs_f64()),
                            Some((false, _)) => "Ready  ·  Last run stopped".into(),
                            None => "Ready".into(),
                        };
                        ui.label(small(&text, skin.text));
                    }
                    if self.completion.ghost.is_some() {
                        ui.label(small("·  Tab inserts the suggestion, Esc dismisses it", skin.text_secondary));
                    } else if self.completion.asked.is_some() {
                        ui.label(small("·  Claude is suggesting…", skin.text_tertiary));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let divider = |ui: &mut Ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(1.0, 12.0), Sense::hover());
                            ui.painter().rect_filled(r, Rounding::ZERO, skin.separator);
                        };
                        // Made in Brazil, with the flag before the label.
                        ui.label(small("Made in Brazil", skin.text_secondary));
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                        icons::paint(ui, &skin, Ic::BrazilFlag, r.center(), 16.0, false);
                        ui.spacing_mut().item_spacing.x = 8.0;
                        divider(ui);
                        let t = self.tab();
                        let path = match &t.path {
                            Some(p) => {
                                let parts: Vec<String> = p.iter().rev().take(2).map(|s| s.to_string_lossy().into_owned()).collect();
                                parts.into_iter().rev().collect::<Vec<_>>().join(std::path::MAIN_SEPARATOR_STR)
                            }
                            None => "Not saved".into(),
                        };
                        let r = ui.label(small(&path, skin.text_secondary));
                        if let Some(p) = &t.path {
                            r.on_hover_text(p.display().to_string());
                        }
                        divider(ui);
                        ui.label(small(&format!("Section {} of {}", t.current_section() + 1, t.section_count()), skin.text_secondary));
                        divider(ui);
                        ui.label(small(if t.kind == Kind::Live { "Live script" } else { "Script" }, skin.text_secondary));
                    });
                });
            });
    }

    /// The section running in the active tab.
    fn running_section(&self) -> Option<usize> {
        let tab = self.tabs.iter().find(|t| self.is_running_tab(t.id))?;
        match self.origin? {
            Origin::Tab { kind: Kind::Live, .. } => self.running_block.and_then(|b| tab.doc.index_of(b)).map(|i| tab.doc.section_of(i)),
            Origin::Tab { kind: Kind::Script, line_offset, .. } => Some(script_section_of(tab.script_text(), line_offset)),
            Origin::Command => None,
        }
    }

    // ---- body --------------------------------------------------------------------------

    /// Files | document over command window | workspace, with 12 px splitters.
    fn body(&mut self, ctx: &egui::Context) {
        let skin = self.skin.clone();
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(skin.window).inner_margin(Margin { left: 12.0, right: 12.0, top: 0.0, bottom: 8.0 }))
            .show(ctx, |ui| {
                let full = ui.max_rect();
                let min_center = 320.0;
                let (mut x0, mut x1) = (full.left(), full.right());
                let mut files_rect = None;
                let mut ws_rect = None;
                let mut chat_rect = None;
                if self.files_open {
                    let w = self.files_w.min((full.width() - min_center - 12.0).max(160.0));
                    files_rect = Some(Rect::from_min_max(pos2(x0, full.top()), pos2(x0 + w, full.bottom())));
                    let split = Rect::from_min_max(pos2(x0 + w, full.top()), pos2(x0 + w + 12.0, full.bottom()));
                    let d = widgets::splitter(ui, &skin, Id::new("split-files"), split, true);
                    self.files_w = (self.files_w + d).clamp(160.0, 420.0);
                    x0 += w + 12.0;
                }
                if self.chat_open {
                    let w = self.chat_w.min((x1 - x0 - min_center - 12.0).max(260.0));
                    chat_rect = Some(Rect::from_min_max(pos2(x1 - w, full.top()), pos2(x1, full.bottom())));
                    let split = Rect::from_min_max(pos2(x1 - w - 12.0, full.top()), pos2(x1 - w, full.bottom()));
                    let d = widgets::splitter(ui, &skin, Id::new("split-chat"), split, true);
                    self.chat_w = (self.chat_w - d).clamp(280.0, 640.0);
                    x1 -= w + 12.0;
                }
                if self.ws_open {
                    let w = self.ws_w.min((x1 - x0 - min_center - 12.0).max(200.0));
                    ws_rect = Some(Rect::from_min_max(pos2(x1 - w, full.top()), pos2(x1, full.bottom())));
                    let split = Rect::from_min_max(pos2(x1 - w - 12.0, full.top()), pos2(x1 - w, full.bottom()));
                    let d = widgets::splitter(ui, &skin, Id::new("split-ws"), split, true);
                    self.ws_w = (self.ws_w - d).clamp(200.0, 480.0);
                    x1 -= w + 12.0;
                }
                let mut doc_rect = Rect::from_min_max(pos2(x0, full.top()), pos2(x1, full.bottom()));
                if self.cmd_open {
                    let h = self.cmd_h.min((doc_rect.height() - 120.0 - 12.0).max(60.0));
                    let cmd_rect = Rect::from_min_max(pos2(x0, full.bottom() - h), pos2(x1, full.bottom()));
                    let split = Rect::from_min_max(pos2(x0, cmd_rect.top() - 12.0), pos2(x1, cmd_rect.top()));
                    let d = widgets::splitter(ui, &skin, Id::new("split-cmd"), split, false);
                    self.cmd_h = (self.cmd_h - d).clamp(60.0, 420.0);
                    doc_rect.max.y = split.top();
                    self.command_window(ui, cmd_rect);
                }
                if let Some(r) = files_rect {
                    self.files_panel(ui, r);
                }
                if let Some(r) = ws_rect {
                    self.workspace_panel(ui, r);
                }
                if let Some(r) = chat_rect {
                    self.chat_panel(ui, r);
                }
                self.document(ui, doc_rect);
            });
    }

    fn card(ui: &mut Ui, skin: &Skin, rect: Rect) -> Ui {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS_LARGE), skin.card);
        ui.painter().rect_stroke(rect.shrink(0.5), Rounding::same(RADIUS_LARGE), Stroke::new(1.0_f32, skin.border));
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect.shrink(1.0)).layout(Layout::top_down(Align::Min)));
        child.set_clip_rect(rect.shrink(1.0));
        child
    }

    /// The ModernWinForms Drawer header: 8 px top inset and a 26 px row with the semibold title.
    fn drawer_header(ui: &mut Ui, skin: &Skin, icon: Option<Ic>, title: &str, right: impl FnOnce(&mut Ui)) {
        ui.add_space(8.0);
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
        let mut child = ui.new_child(UiBuilder::new().max_rect(row.shrink2(vec2(12.0, 0.0)).with_max_x(row.right() - 8.0)).layout(Layout::left_to_right(Align::Center)));
        child.spacing_mut().item_spacing.x = 8.0;
        if let Some(ic) = icon {
            let (r, _) = child.allocate_exact_size(vec2(17.0, 17.0), Sense::hover());
            icons::paint(&child, skin, ic, r.center(), 17.0, false);
        }
        child.label(RichText::new(title).font(semibold(14.0)).color(skin.text));
        child.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            right(ui);
        });
    }

    fn files_panel(&mut self, ui: &mut Ui, rect: Rect) {
        let skin = self.skin.clone();
        let mut ui = Self::card(ui, &skin, rect);
        let title = match self.tree.root() {
            Some(r) => r.file_name().map_or_else(|| r.display().to_string(), |n| n.to_string_lossy().into_owned()),
            None => "Files".to_string(),
        };
        let has_root = self.tree.root().is_some();
        let mut pick = false;
        let mut close = false;
        Self::drawer_header(&mut ui, &skin, Some(Ic::Folder), &title, |ui| {
            if has_root {
                if widgets::cmd_button(ui, &skin, Some(Ic::Clear), "", true, false, false).on_hover_text("Close folder").clicked() {
                    close = true;
                }
                if widgets::cmd_button(ui, &skin, Some(Ic::Open), "", true, false, false).on_hover_text("Open folder").clicked() {
                    pick = true;
                }
            }
        });
        if !has_root {
            egui::Frame::none().inner_margin(Margin { left: 16.0, right: 12.0, top: 12.0, bottom: 0.0 }).show(&mut ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.add(egui::Label::new(RichText::new("No folder opened.").color(skin.text_secondary)).wrap());
                if widgets::cmd_button(ui, &skin, Some(Ic::Open), "Open Folder", true, false, false).on_hover_text("Choose the project folder").clicked() {
                    pick = true;
                }
            });
        }
        if pick {
            self.open_folder_dialog();
        }
        if close {
            self.tree.set_root(None);
        }
        let selected = self.tab().path.clone();
        let mut toggle = None;
        let mut open = None;
        let rows = self.tree.rows();
        let mut inner = ui.new_child(UiBuilder::new().max_rect(ui.available_rect_before_wrap().shrink2(vec2(4.0, 0.0))).layout(Layout::top_down(Align::Min)));
        scroll_area(&mut inner, &skin, "files", |ui| {
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in &rows {
                let id = Id::new(("file", row.path.display().to_string()));
                let sel = selected.as_ref() == Some(&row.path);
                let r = widgets::tree_row(ui, &skin, id, row.depth, row.dir, row.open, row.icon, &row.name, sel);
                if r.clicked() {
                    if row.dir {
                        toggle = Some(row.path.clone());
                    } else {
                        open = Some(row.path.clone());
                    }
                }
                r.on_hover_text(row.path.display().to_string());
            }
            ui.add_space(8.0);
        });
        if let Some(p) = toggle {
            self.tree.toggle(&p);
        }
        if let Some(p) = open {
            let ext = p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
            if matches!(ext.as_deref(), Some("mlx") | Some("m")) {
                self.open_path(p);
            }
        }
    }

    fn workspace_panel(&mut self, ui: &mut Ui, rect: Rect) {
        let skin = self.skin.clone();
        let card = Self::card(ui, &skin, rect);
        let mut ui = card;
        let inner = ui.max_rect();
        let mut ui = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(inner.min + vec2(12.0, 8.0), inner.max - vec2(8.0, 8.0))).layout(Layout::top_down(Align::Min)));
        let (header, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
        let mut h = ui.new_child(UiBuilder::new().max_rect(header).layout(Layout::left_to_right(Align::Center)));
        h.label(RichText::new("Workspace").font(semibold(14.0)).color(skin.text));
        let empty = self.workspace.is_empty();
        let count = self.workspace.len();
        h.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if widgets::cmd_button(ui, &skin, Some(Ic::Clear), "", !empty && !self.running, false, false).on_hover_text("Clear workspace").clicked() {
                self.engine.send(Command::ClearWorkspace);
            }
            if !empty {
                ui.label(RichText::new(format!("{count} variable{}", if count == 1 { "" } else { "s" })).size(12.0).color(skin.text_secondary));
            }
        });
        if empty {
            egui::Frame::none().inner_margin(Margin { left: 4.0, right: 4.0, top: 16.0, bottom: 0.0 }).show(&mut ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.add(egui::Label::new(RichText::new("Variables appear here after you run code.").color(skin.text_secondary)).wrap());
                ui.add(egui::Label::new(RichText::new(crate::os::keys("Press Ctrl+Enter to run the current section, or F5 to run all.")).size(12.0).color(skin.text_tertiary)).wrap());
            });
            return;
        }
        // Columns: Name 0.8fr, Value 1.2fr, Size 56 px, 8 px gaps, 8 px row padding.
        let cols = |left: f32, w: f32| {
            let flex = (w - 16.0 - 56.0 - 16.0).max(0.0);
            let name_w = flex * 0.4;
            let value_w = flex * 0.6;
            [(left + 8.0, name_w), (left + 8.0 + name_w + 8.0, value_w), (left + 8.0 + name_w + 8.0 + value_w + 8.0, 56.0)]
        };
        let w = ui.available_width() - 4.0;
        let (hr, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
        for ((x, _), label) in cols(hr.left(), w).into_iter().zip(["Name", "Value", "Size"]) {
            ui.painter().text(pos2(x, hr.center().y), Align2::LEFT_CENTER, label, FontId::proportional(12.0), skin.text_secondary);
        }
        ui.painter().line_segment([pos2(hr.left(), hr.bottom() - 0.5), pos2(hr.right(), hr.bottom() - 0.5)], Stroke::new(1.0_f32, skin.separator));
        scroll_area(&mut ui, &skin, "workspace", |ui| {
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            for v in &self.workspace {
                let (r, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
                let p = ui.painter();
                if resp.hovered() {
                    p.rect_filled(r, Rounding::same(RADIUS), skin.subtle_hover);
                }
                let [name, value, size] = cols(r.left(), w);
                let cell = |(x, cw): (f32, f32), text: &str, font: FontId, color: Color32| {
                    let mut job = LayoutJob::simple_singleline(text.to_string(), font, color);
                    job.wrap = egui::text::TextWrapping::truncate_at_width(cw);
                    let g = ui.fonts(|f| f.layout_job(job));
                    p.galley(pos2(x, r.center().y - g.size().y / 2.0), g, color);
                };
                cell(name, &v.name, FontId::proportional(14.0), skin.text);
                cell(value, &v.value, FontId::monospace(13.0), skin.text);
                cell(size, &v.size, FontId::proportional(12.0), skin.text_secondary);
                resp.on_hover_text(format!("{}  ·  {} {}  ·  {}", v.name, v.size, v.class, v.value));
            }
        });
    }

    fn command_window(&mut self, ui: &mut Ui, rect: Rect) {
        let skin = self.skin.clone();
        let mut ui = Self::card(ui, &skin, rect);
        let mut clear = false;
        let empty = self.hist.is_empty();
        Self::drawer_header(&mut ui, &skin, None, "Command window", |ui| {
            clear = clear_button(ui, &skin, empty).on_hover_text("Clear command window (clc)").clicked();
        });
        if clear {
            self.hist.clear();
        }
        let area = ui.available_rect_before_wrap();
        let body = Rect::from_min_max(area.min + vec2(12.0, 0.0), area.max - vec2(12.0, 8.0));
        let prompt = Rect::from_min_max(pos2(body.left(), body.bottom() - 32.0), body.max);
        let history = Rect::from_min_max(body.min, pos2(body.right(), prompt.top()));
        let mut h = ui.new_child(UiBuilder::new().max_rect(history).layout(Layout::top_down(Align::Min)));
        h.set_clip_rect(history);
        egui::ScrollArea::vertical().id_salt("cmd-history").auto_shrink([false, false]).stick_to_bottom(true).show(&mut h, |ui| {
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            if self.hist.is_empty() {
                ui.label(RichText::new("Run a command against the current workspace. Try total * 2, or type clc to clear.").size(12.0).color(skin.text_tertiary));
            }
            for (text, kind) in &self.hist {
                let color = match kind {
                    HistKind::Input => skin.text_secondary,
                    HistKind::Output => skin.text,
                    HistKind::Error => skin.error,
                };
                let t = text.trim_end_matches('\u{0}');
                let mut job = LayoutJob::default();
                job.append(if t.is_empty() { " " } else { t }, 0.0, TextFormat { font_id: FontId::monospace(13.0), color, line_height: Some(20.0), ..Default::default() });
                ui.add(egui::Label::new(job).selectable(true).extend());
            }
            ui.add_space(4.0);
        });
        let mut p = ui.new_child(UiBuilder::new().max_rect(prompt).layout(Layout::left_to_right(Align::Center)));
        p.spacing_mut().item_spacing.x = 8.0;
        p.label(RichText::new(">>").font(FontId::monospace(13.0)).color(skin.text_secondary));
        let id = Id::new("command-input");
        let size = vec2(p.available_width(), 32.0);
        let enabled = !self.running;
        if p.memory(|m| m.has_focus(id)) && !self.cmd_history.is_empty() {
            let (up, down) = p.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::ArrowDown)));
            if up || down {
                let n = self.cmd_history.len();
                let k = match (self.cmd_recall, up) {
                    (None, true) => Some(n - 1),
                    (Some(k), true) => Some(k.saturating_sub(1)),
                    (Some(k), false) if k + 1 < n => Some(k + 1),
                    _ => None,
                };
                self.cmd_recall = k;
                self.cmd_input = k.map(|k| self.cmd_history[k].clone()).unwrap_or_default();
            }
        }
        let resp = widgets::text_box(&mut p, &skin, &mut self.cmd_input, id, "Enter a command", true, enabled, size);
        if resp.lost_focus() && p.input(|i| i.key_pressed(Key::Enter)) {
            self.submit_command();
            resp.request_focus();
        }
    }

    fn document(&mut self, ui: &mut Ui, rect: Rect) {
        let skin = self.skin.clone();
        let mut ui = Self::card(ui, &skin, rect);
        if let Some((msg, is_error)) = self.notice.clone() {
            let mut close = false;
            egui::Frame::none().inner_margin(Margin::same(8.0)).show(&mut ui, |ui| {
                close = info_bar(ui, &skin, &msg, is_error);
            });
            if close {
                self.notice = None;
            }
        }
        if self.tab().conflict {
            let name = self.tab().title();
            let mut pick = None;
            egui::Frame::none().inner_margin(Margin::same(8.0)).show(&mut ui, |ui| {
                pick = conflict_bar(ui, &skin, &name);
            });
            let i = self.active;
            let now = self.tabs[i].path.as_deref().and_then(disk_state);
            match pick {
                Some(true) => {
                    if let Some(now) = now {
                        self.reload(i, now);
                    }
                }
                Some(false) => {
                    let t = &mut self.tabs[i];
                    t.disk = now;
                    t.conflict = false;
                }
                None => {}
            }
        }
        let running_section = self.running_section();
        let active_id = self.tab().id;
        let this_running = self.is_running_tab(active_id);
        let mut v = View {
            skin: skin.clone(),
            output_right: self.output_right,
            running: self.running,
            running_section: if this_running { running_section } else { None },
            hover_section: self.hover_section,
            hover_block: self.hover_block,
            toolbar_rect: self.toolbar_rect,
            next_hover_section: None,
            next_hover_block: None,
            next_toolbar_rect: Rect::NOTHING,
            copied: self.copied,
            now: ui.input(|i| i.time),
            figure_action: None,
            control_runs: Vec::new(),
            control_edit: self.control_edit.take(),
            ghost: self.completion.ghost.clone(),
            ghost_taken: false,
            caret: None,
        };
        let tab = &mut self.tabs[self.active];
        let mut actions = Vec::new();
        let mut script_run = None;
        ui.push_id(tab.id, |ui| match tab.kind {
            Kind::Live => {
                scroll_area(ui, &skin, "doc", |ui| {
                    actions = live_view(ui, tab, &mut v);
                });
            }
            Kind::Script => {
                script_run = script_view(ui, tab, &mut v);
            }
        });
        let pointer_in = ui.ctx().pointer_hover_pos().is_some_and(|p| rect.contains(p));
        if (self.hover_section, self.hover_block) != (v.next_hover_section, v.next_hover_block) {
            ui.ctx().request_repaint();
        }
        self.hover_section = if pointer_in { v.next_hover_section } else { None };
        self.hover_block = if pointer_in { v.next_hover_block } else { None };
        self.toolbar_rect = v.next_toolbar_rect;
        self.copied = v.copied;
        self.control_edit = v.control_edit;
        self.apply(actions);
        if let Some(s) = script_run {
            self.run_section(s);
        }
        for s in v.control_runs {
            if self.running {
                self.pending = Some((active_id, s));
            } else {
                self.run_section(s);
            }
        }
        if let Some((fig, action, block)) = v.figure_action {
            self.figure_action(ui.ctx(), &fig, action, block);
        }
        if v.ghost_taken {
            self.completion.ghost = None;
        }
        self.complete(ui.ctx(), v.caret);
    }

    fn figure_action(&mut self, ctx: &egui::Context, fig: &Figure, action: FigAction, block: u64) {
        // Exports use the light skin so they read well in documents.
        let skin = Skin::light();
        let size = crate::plot::figure_size(560.0);
        match action {
            FigAction::Copy => match crate::plot::copy_to_clipboard(ctx, &skin, fig, size) {
                Ok(()) => self.copied = Some((block, fig.id, ctx.input(|i| i.time))),
                Err(e) => self.notice = Some((format!("Could not copy the figure: {e}"), true)),
            },
            FigAction::Save => {
                let Some(p) = rfd::FileDialog::new().add_filter("PNG image", &["png"]).add_filter("SVG image", &["svg"]).set_file_name(format!("figure{}.png", fig.id)).save_file() else {
                    return;
                };
                let svg = p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("svg"));
                let result = if svg {
                    std::fs::write(&p, crate::plot::to_svg(ctx, &skin, fig, size)).map_err(|e| e.to_string())
                } else {
                    crate::plot::to_png(ctx, &skin, fig, size, 2.0).and_then(|bytes| std::fs::write(&p, bytes).map_err(|e| e.to_string()))
                };
                if let Err(e) = result {
                    self.notice = Some((format!("Could not save the figure: {e}"), true));
                }
            }
        }
    }

    /// Figures from scripts and the command window, in floating windows.
    fn figure_windows(&mut self, ctx: &egui::Context) {
        let skin = self.skin.clone();
        for (f, open) in &mut self.figures {
            let title = if f.name.is_empty() { format!("Figure {}", f.id) } else { format!("Figure {}: {}", f.id, f.name) };
            egui::Window::new(RichText::new(title).font(semibold(14.0)))
                .id(Id::new(("figure", f.id)))
                .open(open)
                .resizable(false)
                .collapsible(false)
                .default_pos(ctx.screen_rect().center() - vec2(290.0, 170.0))
                .frame(egui::Frame::window(&ctx.style()).fill(skin.card).inner_margin(Margin::same(12.0)))
                .show(ctx, |ui| {
                    crate::plot::figure(ui, &skin, f, 560.0);
                });
        }
        self.figures.retain(|(_, open)| *open);
    }

    /// Resize handles for the borderless window.
    fn window_edges(&self, ctx: &egui::Context) {
        if cfg!(target_os = "macos") {
            return; // the native window resizes itself
        }
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
        self.remember_layout(ctx);
        #[cfg(target_os = "macos")]
        {
            let files: Vec<PathBuf> = self.open_files.try_iter().collect();
            for p in files {
                self.open_path(p);
            }
        }
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
        if ctx.input(|i| i.viewport().close_requested()) && self.tabs.iter().any(|t| t.dirty) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.confirm.is_none() {
                self.close_window(ctx, HashSet::new());
            }
        }
        self.poll_engine(ctx);
        self.serve_mcp();
        self.chat.poll();
        self.watch_files(ctx);
        self.handle_shortcuts(ctx);
        if self.tabs.is_empty() {
            return;
        }
        self.title_bar(ctx);
        if self.tabs.is_empty() {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{} - MatLivre", self.tab().title())));
        self.command_bar(ctx);
        self.status_bar(ctx);
        self.body(ctx);
        self.menus(ctx);
        self.figure_windows(ctx);
        self.window_edges(ctx);
        self.confirm_dialog(ctx);
        // The window frame where DWM does not draw it (the window is borderless).
        if !cfg!(target_os = "macos") && !self.rounded && !ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            let r = ctx.screen_rect();
            ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("border")))
                .rect_stroke(r.shrink(0.5), Rounding::ZERO, Stroke::new(1.0_f32, self.skin.frame));
        }
    }
}

fn rgb(c: Color32) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}

/// True on the user's first launch of MatLivre, which a marker file in the per-user app data
/// folder (`%APPDATA%\matlivre`) then records.
/// The per-user app data folder (`%APPDATA%\matlivre`).
pub(crate) fn app_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    if std::env::var_os("APPDATA").is_none() {
        if let Some(h) = std::env::var_os("HOME") {
            return Some(PathBuf::from(h).join("Library").join("Application Support").join("matlivre"));
        }
    }
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.map(|b| b.join("matlivre"))
}

fn first_launch() -> bool {
    let Some(dir) = app_dir() else { return true };
    let marker = dir.join("welcomed");
    if marker.exists() {
        return false;
    }
    let _ = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&marker, ""));
    true
}

/// A plain script as a document: one code block holding the whole text.
fn script_doc(text: &str) -> Document {
    let mut d = Document::new();
    d.push(BlockKind::Code, text);
    d
}

/// The 0-based line and column of char index `at`.
fn line_col(text: &str, at: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for c in text.chars().take(at) {
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// The char index of a 0-based line and column.
fn char_index(text: &str, line: usize, col: usize) -> usize {
    let mut idx = 0;
    for (i, l) in text.split('\n').enumerate() {
        if i == line {
            return idx + col.min(l.chars().count());
        }
        idx += l.chars().count() + 1;
    }
    idx
}

/// A vertical scroll area whose bar looks like ModernScrollBar: a rounded thumb in the skin's
/// thumb colour over a transparent gutter. egui paints the thumb with the widget fills, which
/// are near the panel colour, so they are swapped for the bar and restored for the content.
fn scroll_area<R>(ui: &mut Ui, skin: &Skin, salt: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope(|ui| {
        let content_style = ui.style().clone();
        style_scroll_bar(ui, skin);
        egui::ScrollArea::vertical()
            .id_salt(salt)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_style(content_style);
                add_contents(ui)
            })
            .inner
    })
    .inner
}

fn style_scroll_bar(ui: &mut Ui, skin: &Skin) {
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
}

/// The MatLivre icon at 18 px, from its 16 px variant.
fn app_logo(ui: &mut Ui, skin: &Skin) {
    let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    icons::paint(ui, skin, Ic::AppMark, r.center(), 18.0, false);
}

/// The overflow "…" button, 32 × 32.
fn more_button(ui: &mut Ui, skin: &Skin, open: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    let p = ui.painter();
    let fill = if open || resp.is_pointer_button_down_on() {
        skin.subtle_pressed
    } else if resp.hovered() {
        skin.subtle_hover
    } else {
        Color32::TRANSPARENT
    };
    p.rect_filled(rect, Rounding::same(RADIUS), fill);
    for dx in [-5.0, 0.0, 5.0] {
        p.circle_filled(rect.center() + vec2(dx, 0.0), 1.1, skin.text);
    }
    resp
}

/// The command window's Clear button: eraser icon and label, 24 px tall, 12 px text.
fn clear_button(ui: &mut Ui, skin: &Skin, disabled: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap("Clear".into(), FontId::proportional(12.0), skin.text);
    let (rect, resp) = ui.allocate_exact_size(vec2(8.0 + 14.0 + 6.0 + galley.size().x + 8.0, 24.0), Sense::click());
    let p = ui.painter();
    if resp.is_pointer_button_down_on() {
        p.rect_filled(rect, Rounding::same(RADIUS), skin.subtle_pressed);
    } else if resp.hovered() {
        p.rect_filled(rect, Rounding::same(RADIUS), skin.subtle_hover);
    }
    icons::paint(ui, skin, Ic::Clear, pos2(rect.left() + 8.0 + 7.0, rect.center().y), 14.0, disabled);
    p.galley(pos2(rect.left() + 8.0 + 14.0 + 6.0, rect.center().y - galley.size().y / 2.0), galley, skin.text);
    resp
}

// ---- live script view --------------------------------------------------------------------

fn live_view(ui: &mut Ui, tab: &mut Tab, v: &mut View) -> Vec<Action> {
    let mut actions = Vec::new();
    let skin = v.skin.clone();
    let avail = ui.available_width();
    let col_w = avail.min(if v.output_right { 1400.0 } else { 960.0 });
    let left = ui.max_rect().left() + ((avail - col_w) / 2.0).max(0.0);
    // Padding 24 / 40 / 160 / 12, then a 44 px section gutter.
    let gutter_x = left + 12.0;
    let content_left = gutter_x + 44.0;
    let content_w = (col_w - 12.0 - 40.0 - 44.0).max(200.0);
    ui.add_space(24.0);
    let sections = tab.doc.sections();
    let current_section = tab.doc.section_of(tab.current);
    let pointer = ui.ctx().pointer_hover_pos();
    for (si, range) in sections.iter().enumerate() {
        if si > 0 {
            let (r, _) = ui.allocate_exact_size(vec2(avail, 28.0), Sense::hover());
            let y = r.center().y.round() + 0.5;
            ui.painter().extend(egui::Shape::dashed_line(&[pos2(content_left, y), pos2(content_left + content_w, y)], Stroke::new(1.0_f32, skin.border), 4.0, 3.0));
        }
        let top = ui.cursor().top();
        let mut sec = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(content_left, top), vec2(content_w, f32::INFINITY))).layout(Layout::top_down(Align::Min)));
        sec.spacing_mut().item_spacing.y = 6.0;
        sec.add_space(2.0);
        for i in range.clone() {
            block_ui(&mut sec, tab, i, content_w, v, &mut actions);
        }
        let bottom = sec.min_rect().bottom() + 2.0;
        ui.advance_cursor_after_rect(Rect::from_min_max(pos2(left, top), pos2(left + col_w, bottom)));
        let sec_rect = Rect::from_min_max(pos2(gutter_x, top), pos2(content_left + content_w, bottom));
        if pointer.is_some_and(|p| sec_rect.contains(p)) {
            v.next_hover_section = Some(si);
        }
        // The section bar: accent on the current section (pulsing while it runs), border on
        // a hovered one.
        let bar = Rect::from_min_max(pos2(gutter_x + 32.0, top + 2.0), pos2(gutter_x + 35.0, bottom - 2.0));
        let p = ui.painter();
        if v.running_section == Some(si) {
            let t = v.now as f32;
            let a = 0.675 + 0.325 * (t * std::f32::consts::TAU / 1.1).cos();
            p.rect_filled(bar, Rounding::same(1.5), skin.accent.gamma_multiply(a));
            ui.ctx().request_repaint();
        } else if si == current_section {
            p.rect_filled(bar, Rounding::same(1.5), skin.accent);
        } else if v.hover_section == Some(si) {
            p.rect_filled(bar, Rounding::same(1.5), skin.border);
        }
        let has_code = range.clone().any(|i| tab.doc.blocks[i].kind == BlockKind::Code);
        if has_code && !v.running && (v.hover_section == Some(si) || si == current_section) {
            let r = Rect::from_min_size(pos2(gutter_x + 2.0, top + 2.0), vec2(24.0, 24.0));
            let mut child = ui.new_child(UiBuilder::new().max_rect(r));
            if widgets::tool_button(&mut child, &skin, Ic::RunSmall, 14.0, &crate::os::keys(&format!("Run section {} (Ctrl+Enter)", si + 1))).clicked() {
                actions.push(Action::RunSection(si));
                if let Some(first) = range.clone().find(|&i| tab.doc.blocks[i].kind == BlockKind::Code) {
                    actions.push(Action::Focus(first));
                }
            }
        }
    }
    ui.add_space(160.0);
    actions
}

fn block_ui(ui: &mut Ui, tab: &mut Tab, i: usize, content_w: f32, v: &mut View, actions: &mut Vec<Action>) {
    let skin = v.skin.clone();
    let kind = tab.doc.blocks[i].kind;
    let id = tab.doc.blocks[i].id;
    let section = tab.doc.section_of(i);
    let edit_id = Id::new(("block", id));
    let top = ui.cursor().top();
    let focused = i == tab.current && v.running_section.is_none();
    let resp = if kind == BlockKind::Code {
        code_block(ui, tab, i, content_w, focused, v)
    } else {
        text_block(ui, &skin, &mut tab.doc.blocks[i], edit_id)
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
    let block_rect = Rect::from_min_max(pos2(ui.max_rect().left(), top), pos2(ui.max_rect().left() + content_w, ui.min_rect().bottom()));
    let pointer = ui.ctx().pointer_hover_pos();
    let on_toolbar = v.hover_block == Some(id) && pointer.is_some_and(|p| v.toolbar_rect.contains(p));
    if pointer.is_some_and(|p| block_rect.contains(p)) || on_toolbar {
        v.next_hover_block = Some(id);
    }
    // Block toolbar: run | up, down | delete, floating 15 px above the top-right corner.
    let show = v.hover_block == Some(id) || (v.hover_block.is_none() && focused && kind == BlockKind::Code);
    if show {
        let right = if v.output_right && kind == BlockKind::Code { block_rect.left() + (content_w - 16.0) / 2.0 } else { block_rect.right() } - 8.0;
        let size = vec2(2.0 + 24.0 * 4.0 + 5.0 * 2.0 + 5.0 * 2.0 + 2.0 + 2.0, 30.0);
        let tb = Rect::from_min_size(pos2(right - size.x, top + if kind == BlockKind::Code { 2.0 } else { 0.0 } - 15.0), size);
        v.next_toolbar_rect = tb;
        let mut child = ui.new_child(UiBuilder::new().max_rect(tb).layout(Layout::left_to_right(Align::Center)));
        toolbar_frame(&skin).show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if widgets::tool_button(ui, &skin, Ic::RunSmall, 14.0, &crate::os::keys("Run section (Ctrl+Enter)")).clicked() && !v.running {
                actions.push(Action::RunSection(section));
            }
            toolbar_separator(ui, &skin);
            if widgets::tool_button(ui, &skin, Ic::MoveUp, 16.0, "Move up").clicked() {
                actions.push(Action::Move { from: i, up: true });
            }
            if widgets::tool_button(ui, &skin, Ic::MoveDown, 16.0, "Move down").clicked() {
                actions.push(Action::Move { from: i, up: false });
            }
            toolbar_separator(ui, &skin);
            if widgets::tool_button(ui, &skin, Ic::Delete, 16.0, "Delete block").clicked() {
                actions.push(Action::Delete(i));
            }
        });
    }
}

fn toolbar_frame(skin: &Skin) -> egui::Frame {
    egui::Frame::none()
        .fill(skin.menu_bg)
        .stroke(Stroke::new(1.0_f32, skin.menu_border))
        .rounding(Rounding::same(RADIUS))
        .inner_margin(Margin::same(2.0))
        .shadow(egui::epaint::Shadow { offset: vec2(0.0, 4.0), blur: 8.0, spread: 0.0, color: Color32::from_black_alpha(36) })
}

fn toolbar_separator(ui: &mut Ui, skin: &Skin) {
    let (r, _) = ui.allocate_exact_size(vec2(5.0, 24.0), Sense::hover());
    ui.painter().rect_filled(Rect::from_center_size(r.center(), vec2(1.0, 16.0)), Rounding::ZERO, skin.separator);
}

fn text_block(ui: &mut Ui, skin: &Skin, block: &mut Block, edit_id: Id) -> egui::Response {
    let (font, line_height, margin, hint) = match block.kind {
        BlockKind::Title => (semibold(28.0), 35.0, Margin { left: 4.0, right: 4.0, top: 2.0, bottom: 2.0 }, "Title"),
        BlockKind::Heading => (semibold(20.0), 26.0, Margin { left: 4.0, right: 4.0, top: 8.0, bottom: 0.0 }, "Heading"),
        BlockKind::Heading2 => (semibold(16.0), 21.0, Margin { left: 4.0, right: 4.0, top: 6.0, bottom: 0.0 }, "Subheading"),
        _ => (FontId::proportional(14.0), 20.3, Margin { left: 4.0, right: 4.0, top: 0.0, bottom: 2.0 }, "Enter text"),
    };
    let color = skin.text;
    let f2 = font.clone();
    let mut layouter = move |ui: &Ui, s: &str, wrap: f32| {
        let mut job = LayoutJob::default();
        job.append(s, 0.0, TextFormat { font_id: f2.clone(), color, line_height: Some(line_height), ..Default::default() });
        job.wrap.max_width = wrap;
        ui.fonts(|f| f.layout_job(job))
    };
    ui.add(
        egui::TextEdit::multiline(&mut block.text)
            .id(edit_id)
            .frame(false)
            .margin(margin)
            .desired_rows(1)
            .desired_width(f32::INFINITY)
            .hint_text(RichText::new(hint).font(font).color(skin.text_tertiary))
            .layouter(&mut layouter),
    )
}

/// The width a live control takes in its code line, with its 4 px margins.
fn control_width(ui: &Ui, block: &Block, i: usize) -> f32 {
    let c = &block.controls[i];
    let text_w = |s: &str| ui.fonts(|f| f.layout_no_wrap(s.to_string(), FontId::proportional(14.0), Color32::WHITE).size().x);
    let w = match &c.kind {
        ControlKind::Slider { .. } => {
            let v: f64 = block.control_text(i).trim().parse().unwrap_or(0.0);
            150.0 + 8.0 + text_w(&livre_lang::display::fmt_g(v, 6)).max(24.0)
        }
        ControlKind::DropDown { .. } => 110.0,
        ControlKind::CheckBox => 20.0,
        ControlKind::EditField => (ui.fonts(|f| f.layout_no_wrap(block.control_text(i).to_string(), FontId::monospace(13.0), Color32::WHITE).size().x) + 24.0).max(80.0),
        ControlKind::Button { label } => (text_w(label) + 32.0).max(64.0),
    };
    w + 8.0
}

/// Space a value control takes beyond its literal's own glyphs.
fn control_lead(width: f32, chars: usize, char_w: f32) -> f32 {
    (width - chars as f32 * char_w).max(0.0)
}

/// Lays the code out for its live controls: each control's literal is hidden behind leading
/// space as wide as the control, and its line is 36 px tall with the rest of the text centred.
/// A button sits on an empty line of its own, which is made 36 px tall.
fn control_layout(mut job: LayoutJob, text: &str, spans: &[(usize, usize, usize, f32)], char_w: f32) -> LayoutJob {
    if spans.is_empty() {
        return job;
    }
    // Byte offsets of each line start.
    let mut line_starts = vec![0];
    line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    let line_end = |line: usize| line_starts.get(line + 1).map_or(text.len(), |e| e - 1);
    let byte_of = |line: usize, col: usize| -> usize {
        let start = line_starts.get(line).copied().unwrap_or(text.len());
        text[start..line_end(line).max(start)].char_indices().nth(col).map_or(line_end(line).max(start), |(b, _)| start + b)
    };
    let mut cuts: Vec<usize> = Vec::new();
    let mut hidden: Vec<(Range<usize>, f32)> = Vec::new();
    let mut centred: Vec<Range<usize>> = Vec::new();
    let mut tall: Vec<Range<usize>> = Vec::new();
    for &(line, start, end, width) in spans {
        let ls = line_starts.get(line).copied().unwrap_or(text.len());
        let le = line_starts.get(line + 1).map_or(text.len(), |e| *e);
        if end > start {
            let (a, b) = (byte_of(line, start), byte_of(line, end));
            let n = text[a..b].chars().count();
            hidden.push((a..b, control_lead(width, n, char_w)));
            centred.push(ls..le);
            cuts.extend([ls, le, a, b]);
        } else if ls >= line_end(line) {
            // An empty line: its paragraph height comes from the newlines around it.
            let from = ls.saturating_sub(1);
            tall.push(from..le.max(from + 1));
            cuts.extend([from, le]);
        }
    }
    let sections = std::mem::take(&mut job.sections);
    for s in sections {
        let mut pieces: Vec<usize> = cuts.iter().copied().filter(|&c| c > s.byte_range.start && c < s.byte_range.end).collect();
        pieces.sort_unstable();
        pieces.dedup();
        let mut a = s.byte_range.start;
        for b in pieces.into_iter().chain(std::iter::once(s.byte_range.end)) {
            let mut format = s.format.clone();
            let mut leading_space = 0.0;
            if let Some((_, lead)) = hidden.iter().find(|(r, _)| r.contains(&a)) {
                format.color = Color32::TRANSPARENT;
                format.line_height = Some(36.0);
                leading_space = *lead;
            } else if centred.iter().any(|r| r.contains(&a)) {
                format.valign = Align::Center;
            }
            if tall.iter().any(|r| r.contains(&a)) {
                format.line_height = Some(36.0);
            }
            job.sections.push(egui::text::LayoutSection { leading_space, byte_range: a..b, format });
            a = b;
        }
    }
    job
}

fn code_block(ui: &mut Ui, tab: &mut Tab, i: usize, content_w: f32, focused: bool, v: &mut View) -> egui::Response {
    let skin = v.skin.clone();
    let id = tab.doc.blocks[i].id;
    let stale = tab.stale.contains(&id);
    let err_line = tab.outputs.get(&id).and_then(|o| {
        o.iter().find_map(|x| match x {
            UiOutput::Error { line, .. } => Some(*line),
            _ => None,
        })
    });
    ui.add_space(2.0);
    let top = ui.cursor().top();
    let left = ui.max_rect().left();
    let (code_w, out_rect_x) = if v.output_right { let half = (content_w - 16.0) / 2.0; (half, Some(left + half + 16.0)) } else { (content_w, None) };
    let mut code_ui = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(left, top), vec2(code_w, f32::INFINITY))).layout(Layout::top_down(Align::Min)));
    let resp = code_editor(&mut code_ui, tab, i, focused && !stale, err_line, v);
    let mut bottom = code_ui.min_rect().bottom();
    if let Some(outs) = tab.outputs.get(&id) {
        let (rect, pad) = match out_rect_x {
            Some(x) => (Rect::from_min_size(pos2(x, top), vec2(code_w, f32::INFINITY)), Margin { left: 0.0, right: 0.0, top: 8.0, bottom: 0.0 }),
            None => (Rect::from_min_size(pos2(left, bottom), vec2(content_w, f32::INFINITY)), Margin { left: 12.0, right: 0.0, top: 6.0, bottom: 4.0 }),
        };
        let mut out_ui = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
        egui::Frame::none().inner_margin(pad).show(&mut out_ui, |ui| {
            if stale {
                ui.set_opacity(0.4);
            }
            outputs_ui(ui, &skin, outs, id, stale, v);
        });
        bottom = bottom.max(out_ui.min_rect().bottom());
    }
    ui.advance_cursor_after_rect(Rect::from_min_max(pos2(left, top), pos2(left + content_w, bottom)));
    resp
}

fn code_editor(ui: &mut Ui, tab: &mut Tab, i: usize, focused: bool, err_line: Option<usize>, v: &mut View) -> egui::Response {
    let skin = v.skin.clone();
    let font = FontId::monospace(13.0);
    let char_w = ui.fonts(|f| f.glyph_width(&font, '0'));
    let block_id = tab.doc.blocks[i].id;
    let edit_id = Id::new(("block", block_id));
    if let Some(old) = take_ghost(ui, edit_id, block_id, &mut tab.doc.blocks[i].text, v) {
        if !tab.doc.blocks[i].controls.is_empty() {
            tab.doc.blocks[i].reanchor_controls(&old);
        }
        tab.dirty = true;
    }
    let section = tab.doc.section_of(i);
    let widths: Vec<f32> = (0..tab.doc.blocks[i].controls.len()).map(|k| control_width(ui, &tab.doc.blocks[i], k)).collect();
    let spans: Vec<(usize, usize, usize, f32)> = tab.doc.blocks[i].controls.iter().zip(&widths).map(|(c, w)| (c.line, c.start, c.end, *w)).collect();
    let old_text = (!spans.is_empty()).then(|| tab.doc.blocks[i].text.clone());
    let frame = egui::Frame::none()
        .fill(skin.code_bg)
        .rounding(Rounding::same(RADIUS))
        .stroke(Stroke::new(1.0_f32, skin.code_border))
        .inner_margin(Margin { left: 0.0, right: 8.0, top: 7.0, bottom: 7.0 });
    let mut output = None;
    let frame_resp = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        let tint = ui.painter().add(egui::Shape::Noop);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_exact_size(vec2(40.0, 20.0), Sense::hover());
            let skin2 = skin.clone();
            let f2 = font.clone();
            let spans2 = spans.clone();
            let mut layouter = move |ui: &Ui, s: &str, wrap: f32| {
                let job = crate::highlight::highlight(s, &skin2, f2.clone(), Some(20.0), false);
                let mut job = control_layout(job, s, &spans2, char_w);
                job.wrap.max_width = wrap;
                ui.fonts(|f| f.layout_job(job))
            };
            let out = egui::TextEdit::multiline(&mut tab.doc.blocks[i].text)
                .id(edit_id)
                .code_editor()
                .frame(false)
                .margin(Margin::ZERO)
                .lock_focus(true)
                .desired_rows(1)
                .desired_width(f32::INFINITY)
                .layouter(&mut layouter)
                .show(ui);
            output = Some(out);
        });
        (tint, ui.max_rect())
    });
    let out = output.unwrap();
    let (tint, inner) = frame_resp.inner;
    let box_rect = frame_resp.response.rect;
    let galley = out.galley.clone();
    let origin = out.galley_pos;
    // Line numbers (40 px, right-aligned, 12 px padding) and the failing line's tint. Text
    // sits at the top of a 20 px row, and centred in a control's 36 px row.
    let fh = ui.fonts(|f| f.row_height(&font));
    let text_y = |r: Rect| r.top() + ((r.height() - 20.0) / 2.0).max(0.0) + fh / 2.0;
    let mut line = 0usize;
    let mut first_row = true;
    let p = ui.painter();
    for row in &galley.rows {
        let r = row.rect.translate(origin.to_vec2());
        if first_row {
            let number_color = if err_line == Some(line + 1) { skin.error } else { skin.text_tertiary };
            if err_line == Some(line + 1) {
                p.set(tint, egui::Shape::rect_filled(Rect::from_min_max(pos2(box_rect.left() + 1.0, r.top()), pos2(box_rect.right() - 1.0, r.bottom())), Rounding::ZERO, skin.error_bg));
            }
            p.text(pos2(inner.left() + 28.0, text_y(r)), Align2::RIGHT_CENTER, (line + 1).to_string(), font.clone(), number_color);
        }
        first_row = row.ends_with_newline;
        if row.ends_with_newline {
            line += 1;
        }
    }
    if focused {
        p.rect_filled(Rect::from_min_max(pos2(box_rect.left(), box_rect.bottom() - 2.0), box_rect.max), Rounding { nw: 0.0, ne: 0.0, sw: RADIUS, se: RADIUS }, skin.accent);
    }
    let block = &mut tab.doc.blocks[i];
    if out.response.changed() {
        if let Some(old) = &old_text {
            block.reanchor_controls(old);
        }
    }
    if out.response.has_focus() {
        if let Some(range) = out.cursor_range {
            tab.caret = Some((block_id, range.as_ccursor_range()));
            v.caret = Some(Caret { block: block_id, index: range.primary.ccursor.index, text: hash_text(&tab.doc.blocks[i].text) });
        }
        paint_ghost(ui, &skin, v, block_id, &galley, origin, box_rect.right() - 8.0);
    }
    // The live controls, over their hidden literals.
    let mut changes: Vec<(usize, String)> = Vec::new();
    let mut run = false;
    let block = &tab.doc.blocks[i];
    for (k, c) in block.controls.iter().enumerate() {
        if out.response.changed() {
            break;
        }
        let idx = char_index(&block.text, c.line, c.start);
        let at = galley.pos_from_ccursor(CCursor::new(idx)).translate(origin.to_vec2());
        let w = widths.get(k).copied().unwrap_or(0.0) - 8.0;
        let (x, cy) = if c.end > c.start {
            (at.left() - control_lead(widths[k], c.end - c.start, char_w) + 4.0, text_y(at))
        } else {
            (at.left() + 4.0, at.center().y)
        };
        let cid = Id::new(("control", block_id, k));
        let literal = block.control_text(k).to_string();
        let tip;
        match &c.kind {
            ControlKind::Slider { min, max, step } => {
                tip = format!("Slider · {} to {}, step {}", fmt_num(*min), fmt_num(*max), fmt_num(*step));
                let mut value: f64 = literal.trim().parse().unwrap_or(*min);
                let mut child = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(x, cy - 12.0), vec2(w, 24.0))));
                if widgets::track_bar(&mut child, &skin, cid, &mut value, *min, *max, *step, 150.0) {
                    changes.push((k, fmt_num(value)));
                }
            }
            ControlKind::DropDown { items, labels } => {
                tip = "Drop-down".into();
                let sel = items.iter().position(|it| it.trim() == literal.trim()).unwrap_or(0);
                let mut child = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(x, cy - 14.0), vec2(110.0, 28.0))));
                if let Some(j) = widgets::combo_box(&mut child, &skin, cid, labels, sel, vec2(110.0, 28.0)) {
                    changes.push((k, items[j].clone()));
                }
            }
            ControlKind::CheckBox => {
                tip = "Check box".into();
                let mut on = literal.trim() == "true";
                let mut child = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(x, cy - 10.0), vec2(20.0, 20.0))));
                if widgets::check_box(&mut child, &skin, cid, &mut on) {
                    changes.push((k, if on { "true" } else { "false" }.into()));
                }
            }
            ControlKind::EditField => {
                tip = "Edit field".into();
                let buf_id = cid.with("buffer");
                let editing = ui.memory(|m| m.has_focus(cid));
                let mut buf: String = if editing { ui.data_mut(|d| d.get_temp(buf_id)).unwrap_or_else(|| literal.clone()) } else { literal.clone() };
                let mut child = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(x, cy - 14.0), vec2(w, 28.0))));
                let r = widgets::text_box(&mut child, &skin, &mut buf, cid, "", true, true, vec2(w, 28.0));
                if r.lost_focus() {
                    let b = buf.trim().to_string();
                    if !b.is_empty() && b != literal {
                        changes.push((k, b));
                    }
                    ui.data_mut(|d| d.remove::<String>(buf_id));
                } else if r.has_focus() {
                    ui.data_mut(|d| d.insert_temp(buf_id, buf));
                }
            }
            ControlKind::Button { label } => {
                tip = "Button · runs the section".into();
                let mut child = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(x, cy - 14.0), vec2(w, 28.0))));
                if widgets::standard_button(&mut child, &skin, cid, label).clicked() {
                    run = true;
                }
            }
        }
        let area = Rect::from_min_size(pos2(x - 4.0, cy - 16.0), vec2(w + 8.0, 32.0));
        if let Some(e) = v.control_edit.as_mut().filter(|e| e.block == block_id && e.index == k && e.at.is_none()) {
            e.at = Some(area.left_bottom() + vec2(0.0, 4.0));
        }
        if ui.rect_contains_pointer(area) {
            if ui.input(|i| i.pointer.secondary_clicked()) {
                v.control_edit = Some(ControlEdit { block: block_id, index: k, at: Some(area.left_bottom() + vec2(0.0, 4.0)), fields: control_fields(&c.kind) });
            } else {
                egui::show_tooltip_at_pointer(ui.ctx(), ui.layer_id(), cid.with("tip"), |ui| {
                    ui.label(format!("{tip}  ·  right-click to edit"));
                });
            }
        }
    }
    let block = &mut tab.doc.blocks[i];
    for (k, lit) in changes.iter() {
        block.set_control_value(*k, lit);
    }
    if !changes.is_empty() {
        tab.dirty = true;
    }
    if !changes.is_empty() || run {
        v.control_runs.push(section);
    }
    control_editor(ui, tab, i, v);
    out.response
}

/// The settings flyout's fields for a control.
fn control_fields(kind: &ControlKind) -> [String; 3] {
    match kind {
        ControlKind::Slider { min, max, step } => [fmt_num(*min), fmt_num(*max), fmt_num(*step)],
        ControlKind::DropDown { items, .. } => [items.join(", "), String::new(), String::new()],
        ControlKind::Button { label } => [label.clone(), String::new(), String::new()],
        _ => Default::default(),
    }
}

fn fmt_num(v: f64) -> String {
    let v = (v * 1e9).round() / 1e9;
    format!("{v}")
}

/// The settings flyout of a live control (right-click): range and step of a slider, the
/// items of a drop-down, the label of a button; or removing the control.
fn control_editor(ui: &mut Ui, tab: &mut Tab, i: usize, v: &mut View) {
    let block_id = tab.doc.blocks[i].id;
    let Some(edit) = &mut v.control_edit else { return };
    if edit.block != block_id {
        return;
    }
    let skin = v.skin.clone();
    let block = &mut tab.doc.blocks[i];
    let Some(control) = block.controls.get(edit.index).cloned() else {
        v.control_edit = None;
        return;
    };
    let mut close = false;
    let mut remove = false;
    let mut apply = false;
    let Some(at) = edit.at else { return };
    let area = egui::Area::new(Id::new("control-edit")).order(egui::Order::Foreground).fixed_pos(at).show(ui.ctx(), |ui| {
        egui::Frame::none()
            .fill(skin.menu_bg)
            .stroke(Stroke::new(1.0_f32, skin.menu_border))
            .rounding(Rounding::same(RADIUS_LARGE))
            .shadow(ui.style().visuals.popup_shadow)
            .inner_margin(Margin::same(12.0))
            .show(ui, |ui| {
                ui.set_width(260.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                let labels: &[&str] = match control.kind {
                    ControlKind::Slider { .. } => &["Minimum", "Maximum", "Step"],
                    ControlKind::DropDown { .. } => &["Items (code literals, comma-separated)"],
                    ControlKind::Button { .. } => &["Label"],
                    _ => &[],
                };
                ui.label(RichText::new("Control settings").font(semibold(14.0)).color(skin.text));
                for (k, label) in labels.iter().enumerate() {
                    ui.label(RichText::new(*label).size(12.0).color(skin.text_secondary));
                    widgets::text_box(ui, &skin, &mut edit.fields[k], Id::new(("control-field", k)), "", true, true, vec2(260.0, 32.0));
                }
                ui.horizontal(|ui| {
                    if !labels.is_empty() && widgets::standard_button(ui, &skin, Id::new("ce-apply"), "Apply").clicked() {
                        apply = true;
                    }
                    if widgets::standard_button(ui, &skin, Id::new("ce-remove"), "Remove control").clicked() {
                        remove = true;
                    }
                    if widgets::standard_button(ui, &skin, Id::new("ce-close"), "Close").clicked() {
                        close = true;
                    }
                });
            });
    });
    let outside = ui.input(|i| i.pointer.primary_pressed()) && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
    if apply {
        let num = |s: &str| s.trim().parse::<f64>().ok();
        let kind = match &control.kind {
            ControlKind::Slider { min, max, step } => {
                let (a, b, s) = (num(&edit.fields[0]).unwrap_or(*min), num(&edit.fields[1]).unwrap_or(*max), num(&edit.fields[2]).unwrap_or(*step));
                (a < b && s > 0.0).then_some(ControlKind::Slider { min: a, max: b, step: s })
            }
            ControlKind::DropDown { .. } => {
                let items: Vec<String> = edit.fields[0].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                (!items.is_empty()).then(|| ControlKind::DropDown { labels: items.clone(), items })
            }
            ControlKind::Button { .. } => Some(ControlKind::Button { label: edit.fields[0].trim().to_string() }),
            k => Some(k.clone()),
        };
        if let Some(kind) = kind {
            block.controls[edit.index].kind = kind;
            tab.dirty = true;
            close = true;
        }
    }
    if remove {
        block.controls.remove(edit.index);
        tab.dirty = true;
        close = true;
    }
    if close || outside {
        v.control_edit = None;
    }
}

fn outputs_ui(ui: &mut Ui, skin: &Skin, outs: &[UiOutput], block: u64, stale: bool, v: &mut View) {
    ui.spacing_mut().item_spacing.y = 8.0;
    let mono = |t: &str, color: Color32| {
        let mut job = LayoutJob::default();
        job.append(t, 0.0, TextFormat { font_id: FontId::monospace(13.0), color, line_height: Some(18.0), ..Default::default() });
        egui::Label::new(job).selectable(true).extend()
    };
    for o in outs {
        match o {
            UiOutput::Text(t) => {
                let t = t.trim_end_matches('\n');
                if !t.is_empty() {
                    ui.add(mono(t, skin.text));
                }
            }
            UiOutput::Value(t) => {
                ui.add(mono(t.trim_end_matches('\n'), skin.text));
            }
            UiOutput::Warning(w) => {
                egui::Frame::none().fill(skin.warning_bg).rounding(Rounding::same(RADIUS)).inner_margin(Margin::symmetric(8.0, 4.0)).show(ui, |ui| {
                    ui.add(mono(&format!("Warning: {w}"), skin.warning));
                });
            }
            UiOutput::Error { message, line } => {
                widgets::error_bar(ui, skin, &format!("Error in line {line}."), message);
            }
            UiOutput::Figure(f) => {
                let w = ui.available_width();
                let resp = crate::plot::figure(ui, skin, f, w);
                let copied = v.copied.is_some_and(|(b, id, t)| b == block && id == f.id && v.now - t < 1.4);
                if v.copied.is_some_and(|(_, _, t)| v.now - t < 1.4) {
                    ui.ctx().request_repaint_after(Duration::from_millis(100));
                } else {
                    v.copied = None;
                }
                if !stale && (ui.rect_contains_pointer(resp.rect) || copied) {
                    let width = 2.0 + 24.0 * 2.0 + 2.0 + 2.0 + if copied { 52.0 } else { 0.0 };
                    let r = Rect::from_min_size(pos2(resp.rect.right() - 8.0 - width, resp.rect.top() + 8.0), vec2(width, 30.0));
                    let mut child = ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::left_to_right(Align::Center)));
                    toolbar_frame(skin).show(&mut child, |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if copied {
                            ui.add_space(4.0);
                            ui.label(RichText::new("Copied").size(12.0).color(skin.text_secondary));
                            ui.add_space(2.0);
                        }
                        if widgets::tool_button(ui, skin, if copied { Ic::Check } else { Ic::CopyImg }, 16.0, "Copy figure to clipboard").clicked() {
                            v.figure_action = Some((f.clone(), FigAction::Copy, block));
                        }
                        if widgets::tool_button(ui, skin, Ic::SaveImg, 16.0, "Save figure as image (PNG, SVG)").clicked() {
                            v.figure_action = Some((f.clone(), FigAction::Save, block));
                        }
                    });
                }
            }
        }
    }
}

// ---- plain script view -------------------------------------------------------------------

/// The `.m` editor: run column, section bar, line numbers, code. Returns a section to run.
fn script_view(ui: &mut Ui, tab: &mut Tab, v: &mut View) -> Option<usize> {
    let skin = v.skin.clone();
    let font = FontId::monospace(13.0);
    let mut run = None;
    let block_id = tab.doc.blocks[0].id;
    let edit_id = Id::new(("block", block_id));
    let focus = tab.focus_block.take().is_some();
    if take_ghost(ui, edit_id, block_id, &mut tab.doc.blocks[0].text, v).is_some() {
        tab.dirty = true;
    }
    ui.scope(|ui| {
        let content_style = ui.style().clone();
        style_scroll_bar(ui, &skin);
        egui::ScrollArea::both().id_salt("script").auto_shrink([false, false]).show(ui, |ui| {
            ui.set_style(content_style);
            let x0 = ui.max_rect().left();
            let code_x = x0 + 32.0 + 3.0 + 48.0;
            ui.add_space(12.0);
            let top = ui.cursor().top();
            let skin2 = skin.clone();
            let f2 = font.clone();
            let mut layouter = move |ui: &Ui, s: &str, _wrap: f32| {
                let job = crate::highlight::highlight(s, &skin2, f2.clone(), Some(20.0), true);
                ui.fonts(|f| f.layout_job(job))
            };
            let width = (ui.available_width() - (code_x - x0) - 16.0).max(200.0);
            let mut child = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(code_x, top), vec2(width, f32::INFINITY))).layout(Layout::top_down(Align::Min)));
            let out = egui::TextEdit::multiline(&mut tab.doc.blocks[0].text)
                .id(edit_id)
                .code_editor()
                .frame(false)
                .margin(Margin::ZERO)
                .lock_focus(true)
                .desired_rows(20)
                .desired_width(width)
                .layouter(&mut layouter)
                .show(&mut child);
            if focus {
                out.response.request_focus();
            }
            if out.response.changed() {
                tab.dirty = true;
            }
            if let Some(range) = out.cursor_range {
                if out.response.has_focus() {
                    tab.current = line_col(&tab.doc.blocks[0].text, range.primary.ccursor.index).0;
                    v.caret = Some(Caret { block: block_id, index: range.primary.ccursor.index, text: hash_text(&tab.doc.blocks[0].text) });
                    paint_ghost(&child, &skin, v, block_id, &out.galley, out.galley_pos, code_x + width);
                }
            }
            let text = tab.doc.blocks[0].text.clone();
            let lines: Vec<&str> = text.split('\n').collect();
            let sections = script_sections(&text);
            let current = script_section_of(&text, tab.current);
            let origin = out.galley_pos;
            let fh = ui.fonts(|f| f.row_height(&font));
            let rows: Vec<Rect> = out.galley.rows.iter().map(|r| r.rect.translate(origin.to_vec2())).collect();
            let row_of = |line: usize| rows.get(line).copied();
            let pointer = ui.ctx().pointer_hover_pos();
            let p = ui.painter().clone();
            for (si, range) in sections.iter().enumerate() {
                let (Some(first), Some(last)) = (row_of(range.start), row_of(range.end.saturating_sub(1).max(range.start))) else { continue };
                let band = Rect::from_min_max(pos2(x0, first.top()), pos2(code_x + width, last.bottom()));
                if pointer.is_some_and(|p| band.contains(p)) {
                    v.next_hover_section = Some(si);
                }
                let bar = Rect::from_min_max(pos2(x0 + 32.0, first.top()), pos2(x0 + 35.0, last.bottom()));
                if si == current {
                    p.rect_filled(bar, Rounding::same(1.5), if v.running_section.is_some() { skin.text_tertiary } else { skin.accent });
                } else if v.hover_section == Some(si) {
                    p.rect_filled(bar, Rounding::same(1.5), skin.border);
                }
                for line in range.clone() {
                    let Some(r) = row_of(line) else { continue };
                    let color = if si == current { skin.text_secondary } else { skin.text_tertiary };
                    p.text(pos2(code_x - 16.0, r.top() + fh / 2.0), Align2::RIGHT_CENTER, (line + 1).to_string(), font.clone(), color);
                    if lines.get(line).is_some_and(|l| is_section_header(l)) {
                        if line > 0 {
                            p.line_segment([pos2(code_x, r.top() + 0.5), pos2(code_x + width, r.top() + 0.5)], Stroke::new(1.0_f32, skin.separator));
                        }
                        if !v.running && (v.hover_section == Some(si) || si == current) {
                            let br = Rect::from_min_size(pos2(x0 + 6.0, r.top() - 2.0), vec2(24.0, 24.0));
                            let mut c = ui.new_child(UiBuilder::new().max_rect(br));
                            if widgets::tool_button(&mut c, &skin, Ic::RunSmall, 14.0, &crate::os::keys(&format!("Run section {} (Ctrl+Enter)", si + 1))).clicked() {
                                run = Some(si);
                                tab.current = line;
                            }
                        }
                    }
                }
            }
            let bottom = child.min_rect().bottom();
            ui.advance_cursor_after_rect(Rect::from_min_max(pos2(x0, top), pos2(code_x + width, bottom)));
            ui.add_space(160.0);
        });
    });
    run
}

/// Whether a pause in typing should bring a suggestion: at the end of a line that has code,
/// or on a new line below one.
fn wants_completion(before: &str, after: &str) -> bool {
    let rest = after.split('\n').next().unwrap_or("");
    if !rest.trim().is_empty() {
        return false;
    }
    let mut lines = before.rsplit('\n');
    let line = lines.next().unwrap_or("");
    let prev = lines.next().unwrap_or("");
    let code = |l: &str| !l.trim().is_empty() && !l.trim_start().starts_with('%');
    code(line) || (line.trim().is_empty() && code(prev))
}

/// Tab in the focused editor inserts the suggestion at its caret. Returns the text before.
fn take_ghost(ui: &Ui, edit_id: Id, block: u64, text: &mut String, v: &mut View) -> Option<String> {
    let g = v.ghost.as_ref().filter(|g| g.block == block)?;
    if !ui.memory(|m| m.has_focus(edit_id)) || !ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
        return None;
    }
    let old = text.clone();
    let at = text.char_indices().nth(g.at).map_or(text.len(), |(b, _)| b);
    text.insert_str(at, &g.text);
    let end = g.at + g.text.chars().count();
    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), edit_id) {
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(CCursor::new(end))));
        state.store(ui.ctx(), edit_id);
    }
    v.ghost = None;
    v.ghost_taken = true;
    Some(old)
}

/// The suggestion in grey after the caret; lines past the first cover the code below them.
fn paint_ghost(ui: &Ui, skin: &Skin, v: &View, block: u64, galley: &egui::Galley, origin: Pos2, right: f32) {
    let Some(g) = v.ghost.as_ref().filter(|g| g.block == block) else { return };
    let at = galley.pos_from_ccursor(CCursor::new(g.at)).translate(origin.to_vec2());
    let font = FontId::monospace(13.0);
    let p = ui.painter();
    let mut lines = g.text.split('\n');
    p.text(at.left_top(), Align2::LEFT_TOP, lines.next().unwrap_or(""), font.clone(), skin.text_tertiary);
    let rest: Vec<&str> = lines.collect();
    if rest.is_empty() {
        return;
    }
    let top = at.top() + 20.0;
    let cover = Rect::from_min_max(pos2(origin.x - 2.0, top), pos2(right, top + 20.0 * rest.len() as f32));
    p.rect_filled(cover, Rounding::same(2.0), skin.code_bg);
    p.rect_filled(Rect::from_min_size(pos2(origin.x - 6.0, top + 2.0), vec2(2.0, cover.height() - 4.0)), Rounding::same(1.0), skin.accent);
    for (k, l) in rest.iter().enumerate() {
        p.text(pos2(origin.x, top + 20.0 * k as f32), Align2::LEFT_TOP, *l, font.clone(), skin.text_tertiary);
    }
}

/// The bar a tab shows when its file changed on disk while it had unsaved changes.
/// `Some(true)` reloads the file, `Some(false)` keeps the editor's version.
fn conflict_bar(ui: &mut Ui, skin: &Skin, name: &str) -> Option<bool> {
    let mut pick = None;
    egui::Frame::none()
        .fill(skin.info_bg)
        .rounding(Rounding::same(RADIUS))
        .stroke(Stroke::new(1.0_f32, skin.separator))
        .inner_margin(Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 8.0, skin.accent);
                ui.painter().text(r.center(), Align2::CENTER_CENTER, "!", FontId::proportional(11.0), if skin.dark { Color32::BLACK } else { Color32::WHITE });
                ui.add_space(4.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if widgets::dialog_button(ui, skin, Id::new("conflict-keep"), "Keep mine", 96.0, false).on_hover_text("Keep the editor's text; saving replaces the file").clicked() {
                        pick = Some(false);
                    }
                    if widgets::dialog_button(ui, skin, Id::new("conflict-reload"), "Reload", 80.0, true).on_hover_text("Load the file from disk; your unsaved changes are lost").clicked() {
                        pick = Some(true);
                    }
                    ui.add(egui::Label::new(RichText::new(format!("{name} was changed by another program while you had unsaved changes."))).wrap());
                });
            });
        });
    pick
}

/// ModernInfoBar (informational / error) for notices. Returns true when dismissed.
fn info_bar(ui: &mut Ui, skin: &Skin, msg: &str, error: bool) -> bool {
    let mut closed = false;
    let (bg, icon) = if error { (skin.error_bg, skin.error) } else { (skin.info_bg, skin.accent) };
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
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("✕").clicked() {
                        closed = true;
                    }
                });
            });
        });
    closed
}

#[cfg(test)]
mod tests {
    use super::{disk_change, wants_completion, Disk, DiskChange, SavedLayout};

    #[test]
    fn disk_changes_are_told_apart() {
        let t0 = Some(std::time::SystemTime::UNIX_EPOCH);
        let t1 = t0.map(|t| t + std::time::Duration::from_secs(5));
        let known = Disk { modified: t0, len: 10, hash: 1 };
        assert_eq!(disk_change(&known, &known.clone()), DiskChange::None);
        assert_eq!(disk_change(&known, &Disk { modified: t1, len: 10, hash: 1 }), DiskChange::Touched);
        assert_eq!(disk_change(&known, &Disk { modified: t1, len: 12, hash: 2 }), DiskChange::Changed);
        assert_eq!(disk_change(&known, &Disk { modified: t0, len: 10, hash: 3 }), DiskChange::Changed);
    }

    #[test]
    fn completion_comes_at_the_end_of_a_line() {
        assert!(wants_completion("x = linspace(0, 1, 10);\nplot(x, ", ""));
        assert!(wants_completion("for i = 1:3\n", "\nend"));
        assert!(!wants_completion("plot(x", ", y)"));
        assert!(!wants_completion("", ""));
        assert!(!wants_completion("% a comment", ""));
    }

    /// A first launch starts with the Files drawer closed; a saved layout comes back as saved.
    #[test]
    fn layout_defaults_and_round_trip() {
        let dir = std::env::temp_dir().join(format!("matlivre-layout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("APPDATA", &dir);
        let first = SavedLayout::load();
        assert!(!first.files_open && first.cmd_open && first.ws_open && !first.output_right && !first.dark && !first.chat_open && first.completions);
        let mine = SavedLayout {
            files_open: true,
            cmd_open: false,
            ws_open: true,
            files_w: 300.0,
            ws_w: 280.0,
            cmd_h: 210.0,
            output_right: true,
            dark: true,
            chat_open: true,
            chat_w: 420.0,
            completions: false,
            folder: Some(dir.clone()),
        };
        mine.save();
        assert!(SavedLayout::load() == mine);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
