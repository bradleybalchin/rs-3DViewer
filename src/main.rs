use eframe::egui::{self};
use std::{path::PathBuf};
mod ui;
use ui::menu_bar::{MenuAction, menu_bar};
use ui::side_panel::{side_panel};
use ui::viewport::viewport;

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
        Box::new(|_cc| Ok(Box::new(ViewerApp::default()))),
    )
}


pub struct Settings {
    point_size: f32,
    background: [u8; 3],
}

impl Default for Settings {
    fn default() -> Self {
        Self { 
            point_size: 1.0, 
            background: [0,0,0] 
        }
    }
}

#[derive(Default)]
struct ViewerApp {
    loaded_file : Option<PathBuf>,   // current loaded file
    point_count : i32,               // number of points
    settings: Settings,              // setttings
    viewport_size: [f32; 2]          // size of viewport (rendered scene view)

}

impl ViewerApp {

    // file dialog for opening a file
    fn open_file_dialog(&mut self){
        // open dialog
        let picked = rfd::FileDialog::new()
        .add_filter("Point clouds", &["las", "laz", "ply", "xyz", "bin"])
        .pick_file();
        // if path is picked
        if let Some(path) = picked {
            println!("picked: {:?}", path);
            self.loaded_file = Some(path);
        }
        

    }

}

// renders the viewer app UI
impl  eframe::App for ViewerApp {
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(MenuAction::OpenFile) = menu_bar(_ui) {
            self.open_file_dialog();
        }
        side_panel(_ui, &mut self.settings, self.loaded_file.as_deref(), self.point_count);
        viewport(_ui, &self.settings, &mut self.viewport_size);


    }
}