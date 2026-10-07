use eframe::egui::{self, Color32, PointerButton, Pos2, Rect, Sense, Stroke};
use egui::Ui;
use glam::{Mat4, Vec3, Vec4};
use std::path::PathBuf;
 
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("3DViewer")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "3DViewer",
        options,
        Box::new(|_cc| Ok(Box::new(ViewerApp::default()))),
    )
}

#[derive(Default)]
struct ViewerApp {
    
}

impl ViewerApp {

}

impl  eframe::App for ViewerApp {
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {



    }
}