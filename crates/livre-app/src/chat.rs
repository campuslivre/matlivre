//! The Claude panel: a conversation with the Claude Code CLI, run headless in stream-json mode
//! with MatLivre's MCP server attached, so Claude can run code and edit the open documents.
//! Claude Code asks MatLivre before using a tool (`--permission-prompt-tool`), and the panel
//! shows the question with Allow and Deny.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin};
use std::sync::mpsc::{channel, Receiver};

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, Align, Color32, FontId, Id, Key, Layout, Margin, Modifiers, Rounding, Stroke, Ui};
use serde_json::{json, Value};

use crate::i18n::{tr, trf};
use crate::mcp::{self, ToolResult};
use crate::theme::{semibold, Skin, RADIUS};
use crate::widgets;

const SYSTEM: &str = "You are running inside MatLivre, a MATLAB-compatible desktop environment with live scripts. \
The user is talking to you from MatLivre's Claude panel. The matlivre MCP tools act on their running session: \
run_code runs MATLAB code in their base workspace (they see it in the command window), get_workspace lists the \
variables, and list_documents, read_document, edit_document, write_document, open_document and run_document work \
on the documents open in the editor. Prefer editing open documents with edit_document, so the user sees the change \
in the editor before saving. MatLivre follows MATLAB R2023b semantics. Keep answers short.";

/// MatLivre tools that only read, which Claude may use without asking.
const READ_ONLY: &[&str] = &["get_workspace", "list_documents", "read_document"];

pub enum Item {
    User(String),
    Assistant(String),
    Tool { id: String, name: String, summary: String, result: Option<(String, bool)> },
    Approval { key: u64, tool: String, summary: String, answer: Option<bool> },
    Error(String),
    Note(String),
}

enum Line {
    Json(Value),
    Exit(String),
}

struct Proc {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Line>,
}

pub struct Chat {
    pub items: Vec<Item>,
    pub input: String,
    pub busy: bool,
    exe: Option<Option<PathBuf>>,
    proc: Option<Proc>,
    session: Option<String>,
    approvals: Vec<(u64, mcp::Request)>,
    always: HashSet<String>,
    next_key: u64,
    /// The current message's text arrived in pieces, so its full copy is not added again.
    streamed: bool,
}

impl Drop for Chat {
    fn drop(&mut self) {
        self.kill();
    }
}

/// A tool's name as shown: MatLivre's own without their MCP prefix.
fn short_name(name: &str) -> &str {
    name.strip_prefix("mcp__matlivre__").unwrap_or(name)
}

/// What a tool call is about, in a line or a few.
pub fn summarize(name: &str, input: &Value) -> String {
    let s = |k: &str| input.get(k).and_then(Value::as_str).map(str::to_string);
    let text = match short_name(name) {
        "run_code" => s("code"),
        "Bash" | "PowerShell" => s("command"),
        "Read" | "Edit" | "Write" | "NotebookEdit" => s("file_path"),
        "Glob" | "Grep" => s("pattern"),
        "WebFetch" => s("url"),
        "WebSearch" => s("query"),
        "open_document" => s("path"),
        "read_document" | "edit_document" | "write_document" | "run_document" => {
            let doc = s("document").unwrap_or_else(|| "the active document".into());
            Some(match input.get("section").and_then(Value::as_u64) {
                Some(n) => format!("{doc}, section {n}"),
                None => doc,
            })
        }
        _ => None,
    };
    let text = text.unwrap_or_else(|| if input.as_object().is_some_and(|o| o.is_empty()) { String::new() } else { input.to_string() });
    let lines: Vec<&str> = text.lines().take(8).collect();
    let mut out = lines.join("\n");
    if out.chars().count() > 600 {
        out = out.chars().take(600).collect::<String>() + "…";
    } else if text.lines().count() > 8 {
        out.push_str("\n…");
    }
    out
}

/// The text of a tool result's content (a string, or a list of blocks).
fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .map(|b| match b.get("type").and_then(Value::as_str) {
                Some("text") => b.get("text").and_then(Value::as_str).unwrap_or("").to_string(),
                Some("image") => "[image]".into(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

impl Chat {
    pub fn new() -> Self {
        Chat {
            items: Vec::new(),
            input: String::new(),
            busy: false,
            exe: None,
            proc: None,
            session: None,
            approvals: Vec::new(),
            always: HashSet::new(),
            next_key: 1,
            streamed: false,
        }
    }

    pub fn exe(&mut self) -> Option<&Path> {
        self.exe.get_or_insert_with(crate::claude::find).as_deref()
    }

    fn spawn(&mut self, mcp_url: &str, cwd: &Path, ctx: &egui::Context) -> Result<(), String> {
        let exe = self.exe().ok_or(tr("Claude Code is not installed."))?.to_path_buf();
        let config = json!({ "mcpServers": { "matlivre": { "type": "http", "url": mcp_url } } }).to_string();
        let mut cmd = crate::claude::command(&exe);
        cmd.args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages"])
            .args(["--mcp-config", &config])
            .args(["--permission-prompt-tool", &format!("mcp__matlivre__{}", mcp::APPROVE)])
            .args(["--append-system-prompt", SYSTEM])
            .current_dir(cwd);
        if let Some(s) = &self.session {
            cmd.args(["--resume", s]);
        }
        let mut child = cmd.spawn().map_err(|e| trf("Could not start Claude Code: {}", &[&e]))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let mut stderr = child.stderr.take().ok_or("no stderr")?;
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let err = std::thread::spawn(move || {
                let mut s = String::new();
                let _ = stderr.read_to_string(&mut s);
                s
            });
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    if tx.send(Line::Json(v)).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            }
            let _ = tx.send(Line::Exit(err.join().unwrap_or_default()));
            ctx.request_repaint();
        });
        self.proc = Some(Proc { child, stdin, rx });
        Ok(())
    }

    /// Sends the user's message, with `note` (what MatLivre shows them) appended out of sight,
    /// starting Claude Code (or resuming the conversation) first.
    pub fn send(&mut self, text: String, note: &str, mcp_url: &str, cwd: &Path, ctx: &egui::Context) {
        self.items.push(Item::User(text.clone()));
        let text = if note.is_empty() { text } else { format!("{text}

<matlivre>{note}</matlivre>") };
        if self.proc.is_none() {
            if let Err(e) = self.spawn(mcp_url, cwd, ctx) {
                self.items.push(Item::Error(e));
                return;
            }
        }
        let p = self.proc.as_mut().unwrap();
        if p.stdin.write_all(crate::claude::user_message(&text).as_bytes()).and_then(|()| p.stdin.flush()).is_err() {
            self.items.push(Item::Error(tr("Claude Code stopped unexpectedly.").into()));
            self.kill();
            return;
        }
        self.busy = true;
    }

    fn kill(&mut self) {
        if let Some(mut p) = self.proc.take() {
            let _ = p.child.kill();
            let _ = p.child.wait();
        }
        for (_, r) in self.approvals.drain(..) {
            let _ = r.reply.send(deny("The conversation was stopped."));
        }
        for it in &mut self.items {
            if let Item::Approval { answer: a @ None, .. } = it {
                *a = Some(false);
            }
        }
    }

    /// Stops the current turn. The conversation goes on from where it was with the next message.
    pub fn stop(&mut self) {
        if self.busy {
            self.kill();
            self.busy = false;
            self.items.push(Item::Note(tr("Stopped.").into()));
        }
    }

    pub fn clear(&mut self) {
        self.kill();
        self.busy = false;
        self.items.clear();
        self.session = None;
        self.always.clear();
    }

    /// A permission question from Claude Code, through the MCP server.
    pub fn ask(&mut self, req: mcp::Request) {
        let tool = req.args.get("tool_name").and_then(Value::as_str).unwrap_or("").to_string();
        let input = req.args.get("input").cloned().unwrap_or_else(|| json!({}));
        if READ_ONLY.contains(&short_name(&tool)) || self.always.contains(&tool) {
            let _ = req.reply.send(allow(&input));
            return;
        }
        let key = self.next_key;
        self.next_key += 1;
        self.items.push(Item::Approval { key, tool: short_name(&tool).to_string(), summary: summarize(&tool, &input), answer: None });
        self.approvals.push((key, req));
    }

    pub fn answer(&mut self, key: u64, yes: bool, always: bool) {
        let Some(i) = self.approvals.iter().position(|(k, _)| *k == key) else { return };
        let (_, req) = self.approvals.remove(i);
        let tool = req.args.get("tool_name").and_then(Value::as_str).unwrap_or("").to_string();
        let input = req.args.get("input").cloned().unwrap_or_else(|| json!({}));
        if always {
            self.always.insert(tool);
        }
        let _ = req.reply.send(if yes { allow(&input) } else { deny("The user denied this. Ask them what to do instead.") });
        for it in &mut self.items {
            if let Item::Approval { key: k, answer, .. } = it {
                if *k == key {
                    *answer = Some(yes);
                }
            }
        }
    }

    pub fn waiting_for_answer(&self) -> bool {
        !self.approvals.is_empty()
    }

    pub fn poll(&mut self) {
        let mut lines = Vec::new();
        if let Some(p) = &self.proc {
            lines.extend(p.rx.try_iter());
        }
        for l in lines {
            match l {
                Line::Json(v) => self.event(&v),
                Line::Exit(stderr) => {
                    if let Some(mut p) = self.proc.take() {
                        let _ = p.child.wait();
                    }
                    if self.busy {
                        self.busy = false;
                        let tail: Vec<&str> = stderr.lines().rev().take(6).collect();
                        let msg = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
                        self.items.push(Item::Error(if msg.trim().is_empty() { tr("Claude Code stopped unexpectedly.").into() } else { msg }));
                    }
                }
            }
        }
    }

    /// One stream-json message from Claude Code.
    fn event(&mut self, v: &Value) {
        // Subagents' messages carry the tool use they work for; the panel shows the main thread.
        if v.get("parent_tool_use_id").is_some_and(|p| !p.is_null()) {
            return;
        }
        match v.get("type").and_then(Value::as_str) {
            Some("system") => {
                if let Some(s) = v.get("session_id").and_then(Value::as_str) {
                    self.session = Some(s.to_string());
                }
            }
            Some("stream_event") => {
                let e = &v["event"];
                match e.get("type").and_then(Value::as_str) {
                    Some("message_start") => self.streamed = false,
                    Some("content_block_start") if e["content_block"]["type"] == "text" => {
                        self.streamed = true;
                        self.items.push(Item::Assistant(String::new()));
                    }
                    Some("content_block_delta") if e["delta"]["type"] == "text_delta" => {
                        if let (Some(Item::Assistant(t)), Some(d)) = (self.items.last_mut(), e["delta"]["text"].as_str()) {
                            t.push_str(d);
                        }
                    }
                    _ => {}
                }
            }
            Some("assistant") => {
                for b in v["message"]["content"].as_array().into_iter().flatten() {
                    match b.get("type").and_then(Value::as_str) {
                        Some("text") if !self.streamed => {
                            if let Some(t) = b["text"].as_str() {
                                self.items.push(Item::Assistant(t.to_string()));
                            }
                        }
                        Some("tool_use") => {
                            let name = b["name"].as_str().unwrap_or("").to_string();
                            let id = b["id"].as_str().unwrap_or("").to_string();
                            if self.items.iter().any(|i| matches!(i, Item::Tool { id: x, .. } if *x == id)) {
                                continue;
                            }
                            let summary = summarize(&name, &b["input"]);
                            self.items.push(Item::Tool { id, name: short_name(&name).to_string(), summary, result: None });
                        }
                        _ => {}
                    }
                }
            }
            Some("user") => {
                for b in v["message"]["content"].as_array().into_iter().flatten() {
                    if b["type"] != "tool_result" {
                        continue;
                    }
                    let id = b["tool_use_id"].as_str().unwrap_or("");
                    let text = result_text(&b["content"]);
                    let err = b["is_error"].as_bool().unwrap_or(false);
                    for it in self.items.iter_mut().rev() {
                        if let Item::Tool { id: x, result, .. } = it {
                            if x == id {
                                *result = Some((text, err));
                                break;
                            }
                        }
                    }
                }
            }
            Some("result") => {
                self.busy = false;
                if v["is_error"].as_bool().unwrap_or(false) {
                    let msg = v["result"].as_str().map(str::to_string).unwrap_or_else(|| trf("Claude Code reported an error ({}).", &[&v["subtype"].as_str().unwrap_or("unknown")]));
                    self.items.push(Item::Error(msg));
                }
            }
            _ => {}
        }
    }
}

fn allow(input: &Value) -> ToolResult {
    ToolResult::text(json!({ "behavior": "allow", "updatedInput": input }).to_string())
}

fn deny(message: &str) -> ToolResult {
    ToolResult::text(json!({ "behavior": "deny", "message": message }).to_string())
}

// ---- view ------------------------------------------------------------------------------------

pub enum Action {
    Send(String),
    Run(String),
}

/// Markdown, lightly: `#` headings, **bold** and `code` spans, as one wrapped job.
fn prose(text: &str, skin: &Skin, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    let body = FontId::proportional(14.0);
    for (n, line) in text.split('\n').enumerate() {
        if n > 0 {
            job.append("\n", 0.0, TextFormat { font_id: body.clone(), color: skin.text, ..Default::default() });
        }
        let heading = line.trim_start().starts_with('#');
        let line = if heading { line.trim_start().trim_start_matches('#').trim_start() } else { line };
        let base = if heading { semibold(15.0) } else { body.clone() };
        let (mut bold, mut code) = (false, false);
        let mut rest = line;
        while !rest.is_empty() {
            let next = [rest.find("**").filter(|_| !code), rest.find('`')].into_iter().flatten().min();
            let (piece, marker) = match next {
                Some(k) => (&rest[..k], Some(&rest[k..])),
                None => (rest, None),
            };
            if !piece.is_empty() {
                let font = if code { FontId::monospace(13.0) } else if bold || heading { semibold(base.size) } else { base.clone() };
                let bg = if code { skin.code_bg } else { Color32::TRANSPARENT };
                job.append(piece, 0.0, TextFormat { font_id: font, color: skin.text, background: bg, ..Default::default() });
            }
            match marker {
                Some(m) if m.starts_with("**") && !code => {
                    bold = !bold;
                    rest = &m[2..];
                }
                Some(m) => {
                    code = !code;
                    rest = &m[1..];
                }
                None => rest = "",
            }
        }
    }
    job
}

/// Splits a reply into prose and fenced code: (is_code, language, text).
pub fn segments(text: &str) -> Vec<(bool, String, String)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut lang = String::new();
    let mut in_code = false;
    for line in text.split('\n') {
        if let Some(l) = line.trim_start().strip_prefix("```") {
            if in_code {
                out.push((true, std::mem::take(&mut lang), std::mem::take(&mut cur)));
            } else {
                if !cur.trim().is_empty() {
                    out.push((false, String::new(), cur.trim_end().to_string()));
                }
                cur.clear();
                lang = l.trim().to_lowercase();
            }
            in_code = !in_code;
            continue;
        }
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(line);
    }
    if !cur.trim().is_empty() || in_code {
        out.push((in_code, lang, if in_code { cur } else { cur.trim_end().to_string() }));
    }
    out
}

fn code_card(ui: &mut Ui, skin: &Skin, id: Id, lang: &str, code: &str, actions: &mut Vec<Action>) {
    let matlab = matches!(lang, "" | "matlab" | "m" | "octave");
    egui::Frame::none().fill(skin.code_bg).stroke(Stroke::new(1.0_f32, skin.code_border)).rounding(Rounding::same(RADIUS)).inner_margin(Margin::same(8.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let font = FontId::monospace(13.0);
        let job = if matlab {
            crate::highlight::highlight(code, skin, font, Some(18.0), false)
        } else {
            LayoutJob::simple(code.to_string(), font, skin.text, f32::INFINITY)
        };
        egui::ScrollArea::horizontal().id_salt(id.with("scroll")).show(ui, |ui| {
            ui.add(egui::Label::new(job).selectable(true).extend());
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if ui.small_button(tr("Copy")).clicked() {
                ui.ctx().copy_text(code.to_string());
            }
            if matlab && ui.small_button(tr("Run")).on_hover_text(tr("Run in the command window")).clicked() {
                actions.push(Action::Run(code.to_string()));
            }
        });
    });
}

/// The conversation, top to bottom.
pub fn transcript(ui: &mut Ui, skin: &Skin, chat: &mut Chat) -> Vec<Action> {
    let mut actions = Vec::new();
    let mut answers = Vec::new();
    let width = ui.available_width();
    ui.spacing_mut().item_spacing.y = 10.0;
    if chat.items.is_empty() {
        ui.add_space(8.0);
        ui.add(egui::Label::new(egui::RichText::new(tr("Ask Claude about your code, your data or MATLAB.")).color(skin.text_secondary)).wrap());
        ui.add(egui::Label::new(egui::RichText::new(tr("Claude can run code in your workspace and edit the open documents; it asks before it changes anything. It uses the Claude Code installed on this computer.")).size(12.0).color(skin.text_tertiary)).wrap());
        for example in [tr("Plot the variables in my workspace"), tr("Explain what this script does"), tr("Vectorise the loop in the current section")] {
            if ui.add(egui::Button::new(egui::RichText::new(example).size(12.0)).fill(skin.button_fill)).clicked() {
                actions.push(Action::Send(example.to_string()));
            }
        }
    }
    for (n, item) in chat.items.iter().enumerate() {
        let id = Id::new(("chat-item", n));
        match item {
            Item::User(t) => {
                egui::Frame::none().fill(skin.info_bg).rounding(Rounding::same(RADIUS)).inner_margin(Margin::symmetric(10.0, 6.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(egui::RichText::new(t).color(skin.text)).wrap().selectable(true));
                });
            }
            Item::Assistant(t) => {
                for (k, (is_code, lang, s)) in segments(t).iter().enumerate() {
                    if *is_code {
                        code_card(ui, skin, id.with(k), lang, s, &mut actions);
                    } else {
                        ui.add(egui::Label::new(prose(s, skin, width)).selectable(true));
                    }
                }
            }
            Item::Tool { name, summary, result, .. } => {
                egui::Frame::none().stroke(Stroke::new(1.0_f32, skin.separator)).rounding(Rounding::same(RADIUS)).inner_margin(Margin::symmetric(8.0, 6.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let state = match result {
                        None => "running…",
                        Some((_, true)) => "failed",
                        Some(_) => "done",
                    };
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(name).font(semibold(12.0)).color(skin.text_secondary));
                        ui.label(egui::RichText::new(state).size(12.0).color(if matches!(result, Some((_, true))) { skin.error } else { skin.text_tertiary }));
                    });
                    if !summary.is_empty() {
                        ui.add(egui::Label::new(egui::RichText::new(summary).font(FontId::monospace(12.0)).color(skin.text)).wrap());
                    }
                    if let Some((r, err)) = result {
                        let short: String = r.lines().take(4).collect::<Vec<_>>().join("\n");
                        let short = if short.chars().count() > 300 { short.chars().take(300).collect::<String>() + "…" } else { short };
                        if !short.trim().is_empty() {
                            let c = if *err { skin.error } else { skin.text_tertiary };
                            ui.add(egui::Label::new(egui::RichText::new(short).font(FontId::monospace(11.0)).color(c)).wrap()).on_hover_text(r.chars().take(4000).collect::<String>());
                        }
                    }
                });
            }
            Item::Approval { key, tool, summary, answer } => {
                egui::Frame::none().fill(skin.surface).stroke(Stroke::new(1.0_f32, skin.accent)).rounding(Rounding::same(RADIUS)).inner_margin(Margin::same(10.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.label(egui::RichText::new(trf("Claude wants to use {}", &[&tool])).font(semibold(13.0)).color(skin.text));
                    if !summary.is_empty() {
                        ui.add(egui::Label::new(egui::RichText::new(summary).font(FontId::monospace(12.0)).color(skin.text)).wrap());
                    }
                    match answer {
                        None => {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                if widgets::dialog_button(ui, skin, id.with("allow"), tr("Allow"), 72.0, true).clicked() {
                                    answers.push((*key, true, false));
                                }
                                if widgets::dialog_button(ui, skin, id.with("always"), tr("Always"), 72.0, false).on_hover_text(trf("Allow {} for the rest of this conversation", &[&tool])).clicked() {
                                    answers.push((*key, true, true));
                                }
                                if widgets::dialog_button(ui, skin, id.with("deny"), tr("Deny"), 72.0, false).clicked() {
                                    answers.push((*key, false, false));
                                }
                            });
                        }
                        Some(yes) => {
                            ui.label(egui::RichText::new(if *yes { tr("Allowed") } else { tr("Denied") }).size(12.0).color(skin.text_tertiary));
                        }
                    }
                });
            }
            Item::Error(t) => {
                ui.add(egui::Label::new(egui::RichText::new(t).color(skin.error)).wrap().selectable(true));
            }
            Item::Note(t) => {
                ui.label(egui::RichText::new(t).size(12.0).color(skin.text_tertiary));
            }
        }
    }
    if chat.busy && !chat.waiting_for_answer() {
        ui.horizontal(|ui| {
            widgets::progress_ring(ui, skin, 16.0);
            ui.label(egui::RichText::new(tr("Working…")).size(12.0).color(skin.text_secondary));
        });
    }
    for (key, yes, always) in answers {
        chat.answer(key, yes, always);
    }
    actions
}

/// The message box: Enter sends, Shift+Enter starts a new line.
pub fn composer(ui: &mut Ui, skin: &Skin, chat: &mut Chat, id: Id) -> Option<Action> {
    let mut send = false;
    let focused = ui.memory(|m| m.has_focus(id));
    if focused && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
        send = true;
    }
    let frame = egui::Frame::none()
        .fill(if focused { skin.field_focus } else { skin.field_fill })
        .stroke(Stroke::new(1.0_f32, skin.button_stroke))
        .rounding(Rounding::same(RADIUS))
        .inner_margin(Margin::symmetric(8.0, 6.0));
    let mut stop = false;
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.visuals_mut().extreme_bg_color = Color32::TRANSPARENT;
        egui::ScrollArea::vertical().id_salt(id.with("scroll")).max_height(140.0).show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut chat.input)
                    .id(id)
                    .frame(false)
                    .margin(Margin::ZERO)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text(egui::RichText::new(tr("Ask Claude…  (Enter to send, Shift+Enter for a new line)")).color(skin.text_tertiary)),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if chat.busy {
                if widgets::dialog_button(ui, skin, id.with("stop"), tr("Stop"), 64.0, false).clicked() {
                    stop = true;
                }
            } else if widgets::dialog_button(ui, skin, id.with("send"), tr("Send"), 64.0, true).clicked() {
                send = true;
            }
        });
    });
    if stop {
        chat.stop();
    }
    let text = chat.input.trim().to_string();
    if send && !text.is_empty() && !chat.busy {
        chat.input.clear();
        return Some(Action::Send(text));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(chat: &mut Chat, lines: &[&str]) {
        for l in lines {
            chat.event(&serde_json::from_str(l).unwrap());
        }
    }

    #[test]
    fn stream_json_becomes_items() {
        let mut c = Chat::new();
        c.busy = true;
        feed(
            &mut c,
            &[
                r#"{"type":"system","subtype":"init","session_id":"abc"}"#,
                r#"{"type":"stream_event","event":{"type":"message_start"}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"text"}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hel"}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"lo"}}}"#,
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello"}]}}"#,
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__matlivre__run_code","input":{"code":"x = 1"}}]}}"#,
                r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"x =\n     1"}]}]}}"#,
                r#"{"type":"assistant","parent_tool_use_id":"t9","message":{"content":[{"type":"text","text":"subagent"}]}}"#,
                r#"{"type":"result","subtype":"success","is_error":false,"result":"Hello"}"#,
            ],
        );
        assert!(!c.busy);
        assert_eq!(c.session.as_deref(), Some("abc"));
        assert_eq!(c.items.len(), 2);
        assert!(matches!(&c.items[0], Item::Assistant(t) if t == "Hello"));
        assert!(matches!(&c.items[1], Item::Tool { name, summary, result: Some((r, false)), .. } if name == "run_code" && summary == "x = 1" && r.starts_with("x =")));
    }

    #[test]
    fn approvals_reply_to_claude_code() {
        let mut c = Chat::new();
        let (tx, rx) = channel();
        // Reading tools pass without a question.
        c.ask(mcp::Request { tool: "approve".into(), args: json!({"tool_name":"mcp__matlivre__read_document","input":{}}), reply: tx.clone() });
        let r: Value = serde_json::from_str(rx.recv().unwrap().content[0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(r["behavior"], "allow");
        c.ask(mcp::Request { tool: "approve".into(), args: json!({"tool_name":"Bash","input":{"command":"dir"}}), reply: tx.clone() });
        assert!(rx.try_recv().is_err() && c.waiting_for_answer());
        let Item::Approval { key, summary, .. } = &c.items[0] else { panic!() };
        assert_eq!(summary, "dir");
        c.answer(*key, false, false);
        let r: Value = serde_json::from_str(rx.recv().unwrap().content[0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(r["behavior"], "deny");
        // "Always" covers the tool from then on.
        c.ask(mcp::Request { tool: "approve".into(), args: json!({"tool_name":"Bash","input":{"command":"ls"}}), reply: tx.clone() });
        let Item::Approval { key, .. } = &c.items[1] else { panic!() };
        c.answer(*key, true, true);
        let r: Value = serde_json::from_str(rx.recv().unwrap().content[0]["text"].as_str().unwrap()).unwrap();
        assert_eq!((r["behavior"].as_str(), r["updatedInput"]["command"].as_str()), (Some("allow"), Some("ls")));
        c.ask(mcp::Request { tool: "approve".into(), args: json!({"tool_name":"Bash","input":{"command":"ls"}}), reply: tx });
        assert!(rx.try_recv().is_ok());
    }

    /// The whole path with the installed Claude Code: the conversation, the MCP server, the
    /// permission prompt and a tool call. `cargo test -p livre-app real_claude -- --ignored`.
    #[test]
    #[ignore]
    fn real_claude_uses_matlivre_tools() {
        let ctx = egui::Context::default();
        let server = mcp::start(ctx.clone()).unwrap();
        let mut c = Chat::new();
        let cwd = std::env::temp_dir();
        c.send("Use the matlivre run_code tool to evaluate 6*7, then tell me the result.".into(), "", &server.dock_url, &cwd, &ctx);
        let start = std::time::Instant::now();
        let mut asked = false;
        while (c.busy || start.elapsed().as_secs() < 2) && start.elapsed().as_secs() < 180 {
            c.poll();
            for r in server.rx.try_iter() {
                match r.tool.as_str() {
                    mcp::APPROVE => {
                        asked = true;
                        c.ask(r);
                    }
                    "run_code" => {
                        let _ = r.reply.send(ToolResult::text("ans =\n\n    42\n"));
                    }
                    _ => {
                        let _ = r.reply.send(ToolResult::text(""));
                    }
                }
            }
            let pending: Vec<u64> = c.items.iter().filter_map(|i| if let Item::Approval { key, answer: None, .. } = i { Some(*key) } else { None }).collect();
            for k in pending {
                c.answer(k, true, false);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(asked, "Claude Code did not ask before running code");
        assert!(c.items.iter().any(|i| matches!(i, Item::Tool { name, result: Some((r, false)), .. } if name == "run_code" && r.contains("42"))));
        assert!(c.items.iter().any(|i| matches!(i, Item::Assistant(t) if t.contains("42"))));
    }

    #[test]
    fn fenced_code_is_split_out() {
        let s = segments("Try this:\n```matlab\nx = 1;\ny = 2;\n```\nThen run it.");
        assert_eq!(s, vec![(false, String::new(), "Try this:".into()), (true, "matlab".into(), "x = 1;\ny = 2;".into()), (false, String::new(), "Then run it.".into())]);
    }
}
