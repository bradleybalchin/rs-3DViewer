use crate::ui;
use crate::filehandling;
use ui::menu_bar::{MenuAction, menu_bar};
use ui::side_panel::{side_panel};
use ui::viewport::viewport;
use eframe::egui::{self};
use std::{path::PathBuf};

pub struct Settings {
    pub point_size: f32,
    pub background: [u8; 3],
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
pub struct ViewerApp {
    loaded_file : Option<PathBuf>,   // current loaded file
    point_count : i32,               // number of points
    local_origin : Option<[f64;3]>,  // pointcloud local origin
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
        // if path is picked, select the file as the loaded file
        if let Some(path) = picked {
            println!("picked: {:?}", path);
            match filehandling::laz::load(&path) {
                Ok(msg) => {
                    // add more to here (storing pointcloud object, local origin etc.)
                    self.point_count = msg.points.len() as i32;
                    self.loaded_file = Some(path);
                    self.local_origin = Some(msg.origin);
                }
                Err(e) => eprintln!("Failed to load file {}: {e}", path.display())
            }

        }
        

    }

}

// renders the viewer app UI
impl  eframe::App for ViewerApp {
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(MenuAction::OpenFile) = menu_bar(_ui) {
            self.open_file_dialog();
        }
        side_panel(_ui, &mut self.settings, self.loaded_file.as_deref(), self.point_count,self.local_origin);
        viewport(_ui, &self.settings, &mut self.viewport_size);


    }
}