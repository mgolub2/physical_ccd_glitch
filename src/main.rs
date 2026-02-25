mod app;
mod ccd;
mod circuit_display;
mod color;
mod glitch;
mod image_io;
mod pipeline;
mod spice;
mod waveform_display;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::init();

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

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;

    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("No canvas element")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("Not a canvas element");

        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(app::CcdGlitchApp::new(cc)))),
            )
            .await
            .expect("Failed to start eframe");

        // Remove loading indicator now that the app is running
        if let Some(loading) = document.get_element_by_id("loading_text") {
            loading.remove();
        }
    });
}
