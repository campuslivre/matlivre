//! The engine thread owns the interpreter (and so the base workspace). The UI sends run
//! requests and receives outputs as they are produced (spec section 3).

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use livre_lang::{Interpreter, Output, OutputKind, VarInfo};

pub enum Command {
    /// Run `blocks` in order. `all_code` holds every code block of the document so local
    /// functions defined anywhere are callable.
    Run { blocks: Vec<(u64, String)>, all_code: Vec<String>, folder: Option<std::path::PathBuf> },
    /// A command-window line, run in the base workspace without resetting figures or
    /// functions. Its outputs come back under [`COMMAND_ID`].
    Eval(String),
    ClearWorkspace,
}

/// The block id the command window's outputs are reported under.
pub const COMMAND_ID: u64 = u64::MAX;

pub enum Event {
    BlockStarted(u64),
    Output(u64, Output),
    Finished { workspace: Vec<VarInfo>, ok: bool, elapsed: Duration },
}

pub struct Engine {
    tx: Sender<Command>,
    pub rx: Receiver<Event>,
    cancel: Arc<AtomicBool>,
}

impl Engine {
    pub fn start(ctx: egui::Context) -> Self {
        let (tx, cmd_rx) = channel::<Command>();
        let (ev_tx, rx) = channel::<Event>();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        std::thread::Builder::new()
            .name("livre-engine".into())
            .stack_size(64 * 1024 * 1024)
            .spawn(move || engine_main(cmd_rx, ev_tx, flag, ctx))
            .expect("failed to start engine thread");
        Self { tx, rx, cancel }
    }

    pub fn send(&self, cmd: Command) {
        if matches!(cmd, Command::Run { .. }) {
            self.cancel.store(false, Ordering::Relaxed);
        }
        let _ = self.tx.send(cmd);
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn engine_main(rx: Receiver<Command>, tx: Sender<Event>, cancel: Arc<AtomicBool>, ctx: egui::Context) {
    let mut it = Interpreter::new();
    it.set_cancel_flag(cancel);
    let current = Rc::new(Cell::new(0u64));
    {
        let tx = tx.clone();
        let ctx = ctx.clone();
        let current = current.clone();
        it.set_output_sink(move |outs| {
            for o in outs {
                let _ = tx.send(Event::Output(current.get(), o));
            }
            ctx.request_repaint();
        });
    }
    {
        // saveas/print/exportgraphics render with the light skin at MATLAB's figure size.
        let ctx = ctx.clone();
        it.set_figure_exporter(move |fig, format, scale| {
            let skin = crate::theme::Skin::light();
            let size = egui::vec2(560.0, 420.0);
            match format {
                livre_lang::ExportFormat::Svg => Ok(crate::plot::to_svg(&ctx, &skin, fig, size).into_bytes()),
                livre_lang::ExportFormat::Png => crate::plot::to_png(&ctx, &skin, fig, size, scale),
            }
        });
    }
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Command::ClearWorkspace => {
                it.clear_workspace();
                it.close_all_figures();
                let _ = tx.send(Event::Finished { workspace: it.workspace(), ok: true, elapsed: Duration::ZERO });
                ctx.request_repaint();
            }
            Command::Eval(src) => {
                current.set(COMMAND_ID);
                let started = Instant::now();
                let ok = match livre_lang::parser::parse(&src) {
                    Ok(prog) => {
                        it.define_functions(&prog);
                        it.run(&prog.body).is_ok()
                    }
                    Err(e) => {
                        let _ = tx.send(Event::Output(COMMAND_ID, Output { line: 1, kind: OutputKind::Error(e.message) }));
                        false
                    }
                };
                let _ = tx.send(Event::Finished { workspace: it.workspace(), ok, elapsed: started.elapsed() });
                ctx.request_repaint();
            }
            Command::Run { blocks, all_code, folder } => {
                // The script's folder is on the search path (function and classdef files);
                // resetting it each run picks up edited files.
                it.set_search_path(folder.into_iter().collect());
                let started = Instant::now();
                // Each run draws into fresh figures, like re-running a live script.
                it.close_all_figures();
                it.clear_functions();
                for src in &all_code {
                    if let Ok(p) = livre_lang::parser::parse(src) {
                        it.define_functions(&p);
                    }
                }
                let mut ok = true;
                for (id, src) in blocks {
                    current.set(id);
                    let _ = tx.send(Event::BlockStarted(id));
                    ctx.request_repaint();
                    match livre_lang::parser::parse(&src) {
                        Ok(prog) => {
                            if it.run(&prog.body).is_err() {
                                ok = false;
                                break;
                            }
                        }
                        Err(e) => {
                            let out = Output { line: e.line.max(1), kind: OutputKind::Error(e.message) };
                            let _ = tx.send(Event::Output(id, out));
                            ok = false;
                            break;
                        }
                    }
                }
                let _ = tx.send(Event::Finished { workspace: it.workspace(), ok, elapsed: started.elapsed() });
                ctx.request_repaint();
            }
        }
    }
}
