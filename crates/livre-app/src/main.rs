#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod engine;
mod highlight;
mod os;
mod plot;
mod theme;
mod transfer;
mod widgets;

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
        .with_title("matlivre")
        .with_inner_size([1360.0, 860.0])
        .with_min_inner_size([760.0, 480.0])
        .with_decorations(false)
        .with_resizable(true);
    if let Some(p) = at {
        viewport = viewport.with_position(p);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("matlivre", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, launch)))))
}
