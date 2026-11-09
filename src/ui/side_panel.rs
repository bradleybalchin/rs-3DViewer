use crate::app::{PointCloudInfo, Settings};
use eframe::egui;

// side panel showing settings and details
// writes: settings. read-only: loaded point cloud details
pub fn side_panel(ui: &mut egui::Ui, settings: &mut Settings, cloud: Option<&PointCloudInfo>)
{
    egui::Panel::left("settings").resizable(true).default_size(260.0).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Point Cloud");
            if let Some(cloud) = cloud {
                let filename = cloud.path.file_name().and_then(|name| name.to_str()).unwrap_or("None");
                let [x, y, z] = cloud.origin;
                ui.label(format!("Current File : {}", filename));
                ui.label(format!("Points : {}", cloud.point_count));
                ui.label(format!("Local Origin: \nX : {:.2} \nY : {:.2} \nZ : {:.2}", x, y, z));
            } else {
                ui.label("No File Selected");
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