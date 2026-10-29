#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod engine;
mod highlight;
mod plot;
mod theme;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let open = std::env::args().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("matlivre")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([760.0, 480.0])
            .with_decorations(false)
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native("matlivre", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, open)))))
}
