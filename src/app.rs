use crate::ui;
use crate::filehandling;
use ui::menu_bar::{MenuAction, menu_bar};
use ui::side_panel::side_panel;
use ui::viewport::Viewport;
use eframe::egui;
use std::path::PathBuf;

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

/// Details of the loaded point cloud, shown in the side panel
pub struct PointCloudInfo {
    pub path : PathBuf,         // file the cloud was loaded from
    pub point_count : usize,    // number of points
    pub origin : [f64; 3],      // local origin, add back to get real-world coordinates
}

pub struct ViewerApp {
    settings : Settings,                // user settings
    cloud : Option<PointCloudInfo>,     // loaded point cloud (None until a file is opened)
    viewport : Viewport,                // rendered scene view
}

impl ViewerApp {

    pub fn new() -> Self {
        Self {
            settings : Settings::default(),
            cloud : None,
            viewport : Viewport::new(),
        }
    }

    // file dialog for opening a file
    fn open_file_dialog(&mut self){
        // open dialog
        let picked = rfd::FileDialog::new()
        .add_filter("Point clouds", &["las", "laz", "ply", "xyz", "bin"])
        .pick_file();

        // load the picked file, send its points to the viewport and keep its details
        if let Some(path) = picked {
            println!("picked: {:?}", path);
            match filehandling::laz::load(&path) {
                Ok(msg) => {
                    if let Err(e) = self.viewport.set_points(&msg.points) {
                        eprintln!("Failed to upload points: {e}");
                    }
                    self.cloud = Some(PointCloudInfo {
                        path,
                        point_count : msg.points.len(),
                        origin : msg.origin,
                    });
                }
                Err(e) => eprintln!("Failed to load file {}: {e}", path.display())
            }
        }
    }

}

// renders the viewer app UI
impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(MenuAction::OpenFile) = menu_bar(ui) {
            self.open_file_dialog();
        }
        side_panel(ui, &mut self.settings, self.cloud.as_ref());
        self.viewport.ui(ui, &self.settings);
    }
}
