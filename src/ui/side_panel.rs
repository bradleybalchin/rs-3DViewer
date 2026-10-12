use crate::app::Settings;
use eframe::egui;
use std::path::Path;

// side panel showing settings and details
// writes: settings. read-only: loaded file, point count
pub fn side_panel(ui: &mut egui::Ui, settings: &mut Settings, loaded_file: Option<&Path>, point_count: i32, origin:Option<[f64;3]>) 
{
    egui::Panel::left("settings").resizable(true).default_size(260.0).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Point Cloud");
            if let Some(path) = loaded_file {
                let filename = path.file_name().and_then(|name| name.to_str()).unwrap_or("None");
                ui.label(format!("Current File : {}", filename));
            } else {
                ui.label("No File Selected");
            }

            ui.label(format!("Points : {}", point_count));
            if let Some(orig) = origin {
                ui.label(format!("Local Origin: \nX : {:.2} \nY : {:.2} \nZ : {:.2}", orig[0], orig[1], orig[2] ));
            }

            ui.separator();

            ui.heading("Rendering");
            ui.add(egui::Slider::new(&mut settings.point_size, 1.0..=8.0).text("Point Size"));
            ui.horizontal(|ui| {
                ui.label("Background");
                ui.color_edit_button_srgb(&mut settings.background)
            });
        });
    });
}