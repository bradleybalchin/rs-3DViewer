use crate::render::vertex::GpuPoint;
use crate::ui;
use crate::filehandling;
use crate::sampling::{poisson,voxel};

use rand::seq::IndexedRandom;
use ui::menu_bar::{MenuAction, menu_bar};
use ui::side_panel::{SidePanelAction, side_panel};
use ui::viewport::Viewport;
use eframe::egui;
use std::path::PathBuf;

pub struct Settings {
    pub point_size: f32,
    pub background: [u8; 3],

    pub poission_radius: f32,
    pub voxel_resolution: f32,
    pub voxel_fa_resolution : f32
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            point_size: 1.0,
            background: [0,0,0],

            poission_radius:0.0,
            voxel_resolution:0.0,
            voxel_fa_resolution:0.0,
        }
    }
}

/// Details of the loaded point cloud, shown in the side panel
pub struct PointCloudInfo {
    pub path : PathBuf,         // file the cloud was loaded from
    pub points : Vec<GpuPoint>,
    pub point_count : usize,    // number of points
    pub origin : [f64; 3],      // local origin, add back to get real-world coordinates
}

// Details about sampling
pub struct SampleInfo {
    pub method : String,          // Sampling method
    pub point_count : usize     // sample point count
}

pub struct ViewerApp {
    settings : Settings,                  // user settings
    cloud : Option<PointCloudInfo>,       // loaded point cloud (None until a file is opened)
    sample_cloud : Option<SampleInfo>,// currently shown sampled cloud
    viewport : Viewport,                  // rendered scene view
}

impl ViewerApp {

    pub fn new() -> Self {
        Self {
            settings : Settings::default(),
            cloud : None,
            sample_cloud : None,
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
                    let count = msg.points.len();
                    self.cloud = Some(PointCloudInfo {
                        path,
                        points : msg.points,
                        point_count : count,
                        origin : msg.origin,
                    });
                    self.sample_cloud = None;
                }
                Err(e) => eprintln!("Failed to load file {}: {e}", path.display())
            }
        }
    }

    // Perform poission sampling and render the reduced pointcloud sample
    fn sample_poisson(&mut self){
        if let Some(cloud) = &self.cloud{
            match poisson::sample(&cloud.points, self.settings.poission_radius) {
                Ok(pnts) => {
                    if let Err(e) = self.viewport.set_points(&pnts) {
                        eprintln!("Failed to upload points: {e}");
                    }    
                    self.sample_cloud = Some(SampleInfo { method: "Poission".into(), point_count: pnts.len() });
                }
                Err(e) => eprint!("{e}")
            }
        }
    }

    // Perform voxel grid sampling and render the reduced pointcloud sample
    fn sample_voxel_grid(&mut self){
        if let Some(cloud) = &self.cloud{
            match voxel::sample_grid(&cloud.points, self.settings.voxel_resolution) {
                Ok(pnts) => {
                    if let Err(e) = self.viewport.set_points(&pnts) {
                        eprintln!("Failed to upload points: {e}");
                    }    
                    self.sample_cloud = Some(SampleInfo { method: "Voxel Grid".into(), point_count: pnts.len() });
                }
                Err(e) => eprint!("{e}")
            }
        }
    }

    // Perform feature aware voxel sampling and render the reduced pointcloud sample
    fn sample_voxel_fa(&mut self){
        if let Some(cloud) = &self.cloud{
            match voxel::sample_feature_aware(&cloud.points, self.settings.voxel_fa_resolution) {
                Ok(pnts) => {
                    if let Err(e) = self.viewport.set_points(&pnts) {
                        eprintln!("Failed to upload points: {e}");
                    }    
                    self.sample_cloud = Some(SampleInfo { method: "Voxel Feature Aware".into(), point_count: pnts.len() });
                }
                Err(e) => eprint!("{e}")
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
        match side_panel(ui, &mut self.settings, self.cloud.as_ref(), self.sample_cloud.as_ref()) {
            Some(SidePanelAction::SamplePoission) => self.sample_poisson(),
            Some(SidePanelAction::SampleVoxelGrid) => self.sample_voxel_grid(),
            Some(SidePanelAction::SampleVoxelFeatureAware) => self.sample_voxel_fa(),
            None => ()
        }
        self.viewport.ui(ui, &self.settings);
    }
}
