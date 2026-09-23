#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cards;
mod game;
mod hand;
mod icon;
mod sound;
mod strategy;
mod ui;

fn main() -> eframe::Result {
    let icon = eframe::egui::IconData {
        rgba: icon::rgba(256),
        width: 256,
        height: 256,
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Jacks or Better")
            .with_icon(icon)
            .with_inner_size([1000.0, 820.0])
            .with_min_inner_size([780.0, 705.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Jacks or Better",
        options,
        Box::new(|cc| Ok(Box::new(ui::App::new(cc)))),
    )
}
