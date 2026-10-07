use eframe::egui::{self, Color32, PointerButton, Pos2, Rect, Sense, Stroke};
use egui::{response};
use glam::{Mat4, Vec3, Vec4};
use std::{path::PathBuf, thread::sleep};
 
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


struct Settings {
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

    // defines the top menu bar
    fn menu_bar(&mut self, _ui: &mut egui::Ui){
        egui::Panel::top("menu").show(_ui, |ui| {
            // top menu bar
            egui::MenuBar::new().ui(ui, |ui| {
                // file menu section
                ui.menu_button("File", |ui| {
                    if ui.button("Open").clicked() {
                        ui.close();
                        self.open_file_dialog();
                    }
                });
            });
        });
    }

    // side panel showing settings and details
    fn side_panel(&mut self, _ui : &mut egui::Ui) {
        egui::Panel::left("settings").resizable(true).default_size(260.0).show(_ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Point Cloud");
                if let Some(path) = &self.loaded_file {
                    let filename = path.file_name().and_then(|name | name.to_str()).unwrap_or("None");
                    ui.label(format!("Current File : {:?}", filename)); 
                } else {
                    ui.label("No File Selected");
                }

                ui.label(format!("Points : {}", self.point_count));

                ui.separator();

                ui.heading("Rendering");
                ui.add(
                    egui::Slider::new(&mut self.settings.point_size,1.0..=8.0).text("Point Size")
                );
                ui.horizontal(|ui| {
                    ui.label("Background");
                    ui.color_edit_button_srgb(&mut self.settings.background)
                });


            });
            


        });
    }

    // Central viewport for showing rendered scene
    fn viewport(&mut self, _ui : &mut egui::Ui) {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(_ui, |ui| {
            // retreive response and painter for specific part of the screen
            let size = ui.available_size();
            let (response, painter) = ui.allocate_painter(size, Sense::click_and_drag());

            // set viewport size to painter area of screen
            let rect = response.rect;
            self.viewport_size = [rect.width(), rect.height()];

            painter.rect_filled(rect, 0.0, Color32::from_rgb(self.settings.background[0], self.settings.background[1], self.settings.background[2]))
        });
    }


}

// renders the viewer app UI
impl  eframe::App for ViewerApp {
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.menu_bar(_ui);
        self.side_panel(_ui);
        self.viewport(_ui);


    }
}