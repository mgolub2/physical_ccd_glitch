mod app;
mod ccd;
mod color;
mod glitch;
mod image_io;
mod pipeline;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("Physical CCD Glitch"),
        ..Default::default()
    };

    eframe::run_native(
        "Physical CCD Glitch",
        options,
        Box::new(|cc| Ok(Box::new(app::CcdGlitchApp::new(cc)))),
    )
}
