mod ui;
mod app;

fn main() -> eframe::Result {
    // window options
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("3DViewer")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    // run the window
    eframe::run_native(
        "3DViewer",
        options,
        Box::new(|_cc| Ok(Box::new(app::ViewerApp::default()))),
    )
}