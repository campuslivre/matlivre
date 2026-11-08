#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod chat;
mod claude;
mod complete;
mod engine;
mod files;
mod highlight;
mod icons;
mod mcp;
#[cfg(target_os = "macos")]
mod macos;
mod os;
mod plot;
mod theme;
mod transfer;
mod widgets;
mod updater;

use std::path::PathBuf;

use eframe::egui;

/// What the window starts with.
pub enum Launch {
    Welcome,
    Open(PathBuf),
    /// A tab dragged out of another window (a manifest from `transfer::write`).
    Tab(PathBuf),
}

fn main() -> eframe::Result<()> {
    // Velopack's install, update and uninstall hooks: answered and quit before anything else.
    velopack::VelopackApp::build().run();
    // An MCP server over stdio for Claude Code (`claude mcp add matlivre -- matlivre --mcp`).
    if std::env::args().nth(1).as_deref() == Some("--mcp") {
        mcp::stdio();
        return Ok(());
    }
    updater::check_in_background();
    let mut launch = Launch::Welcome;
    let mut at = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--tab" => launch = args.next().map(|p| Launch::Tab(p.into())).unwrap_or(Launch::Welcome),
            "--at" => {
                at = args.next().and_then(|s| {
                    let (x, y) = s.split_once(',')?;
                    Some(egui::pos2(x.parse().ok()?, y.parse().ok()?))
                })
            }
            _ => launch = Launch::Open(a.into()),
        }
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("MatLivre")
        .with_inner_size(app::DEFAULT_SIZE)
        .with_min_inner_size(app::MIN_SIZE)
        .with_icon(std::sync::Arc::new(icons::window_icon()))
        .with_decorations(cfg!(target_os = "macos"))
        .with_resizable(true);
    #[cfg(target_os = "macos")]
    {
        // Native macOS window: traffic lights drawn over our tab bar, no title text.
        viewport = viewport.with_fullsize_content_view(true).with_title_shown(false).with_titlebar_shown(false);
    }
    if let Some(p) = at {
        viewport = viewport.with_position(p);
    }
    // A tab torn off to a new window opens where it was dropped; otherwise the window is
    // centred on the screen the pointer is on.
    let place = at.is_none();
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("matlivre", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, launch, place)))))
}
