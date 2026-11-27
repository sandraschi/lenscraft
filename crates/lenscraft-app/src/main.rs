//! lenscraft-app: L3 pure-Rust egui desktop workstation over lenscraft-core.
mod app;
mod help;
mod theme;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1240.0, 820.0])
            .with_min_inner_size([860.0, 600.0])
            .with_title("LensCraft"),
        ..Default::default()
    };
    eframe::run_native(
        "LensCraft",
        options,
        Box::new(|cc| Ok(Box::new(app::LensApp::new(cc)))),
    )
}
