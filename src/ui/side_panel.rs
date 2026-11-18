use crate::app::{PointCloudInfo, Settings, SampleInfo};
use eframe::egui;
use thousands::Separable;

// side panel showing settings and details
// writes: settings. read-only: loaded point cloud details
pub fn side_panel(ui: &mut egui::Ui, settings: &mut Settings, cloud: Option<&PointCloudInfo>, sample_cloud : Option<&SampleInfo>) -> Option<SidePanelAction>
{
    let mut action = None;

    egui::Panel::left("settings").resizable(true).default_size(260.0).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Point Cloud");
            if let Some(cloud) = cloud {
                let filename = cloud.path.file_name().and_then(|name| name.to_str()).unwrap_or("None");
                let [x, y, z] = cloud.origin;
                ui.label(format!("Current File : {}", filename));
                ui.label(format!("Points : {}", cloud.point_count.separate_with_commas()));
                ui.label(format!("Local Origin: \nX : {:.2} \nY : {:.2} \nZ : {:.2}", x, y, z));


                if let Some(sample_cloud) = sample_cloud {
                    ui.add_space(1.0);
                    ui.label(format!("Sampling Used : {}", sample_cloud.method));
                    ui.label(format!("Sample Size : {}", sample_cloud.point_count.separate_with_commas()));
                }

            } else {
                ui.label("No File Selected");
            }

            ui.separator();

            // Rendering Options
            egui::CollapsingHeader::new("Rendering")
                .default_open(false)
                .show(ui, |ui| { 
                    ui.add(egui::Slider::new(&mut settings.point_size, 1.0..=8.0).text("Point Size"));
                    ui.horizontal(|ui| {
                        ui.label("Background");
                        ui.color_edit_button_srgb(&mut settings.background)
                    });
                }
            );

            // Sampling / data thinning methods
            egui::CollapsingHeader::new("Sampling")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("Poission Sampling");
                    ui.horizontal(|ui| {
                        ui.label("Radius");
                        ui.add(
                            egui::DragValue::new(&mut settings.poission_radius)
                                .speed(0.05)
                                .range(0.0..=f32::MAX)
                                .max_decimals(3),
                        );
                    });
                    if ui.button("Apply").clicked() {
                            action = Some(SidePanelAction::SamplePoission);
                    }

                    ui.add_space(1.5);
                    ui.separator();
                    ui.add_space(1.5);

                    ui.label("Voxel Grid Sampling");
                    ui.horizontal(|ui| {
                        ui.label("Resolution");
                        ui.add(
                            egui::DragValue::new(&mut settings.voxel_resolution)
                                .speed(0.05)
                                .range(0.0..=f32::MAX)
                                .max_decimals(3),
                        );
                    });
                    if ui.button("Apply").clicked() {
                            action = Some(SidePanelAction::SampleVoxelGrid);
                    }

                    ui.add_space(1.5);
                    ui.separator();
                    ui.add_space(1.5);

                    ui.label("Voxel Feature Aware Sampling");
                    ui.horizontal(|ui| {
                        ui.label("Resolution");
                        ui.add(
                            egui::DragValue::new(&mut settings.voxel_fa_resolution)
                                .speed(0.05)
                                .range(0.0..=f32::MAX)
                                .max_decimals(3),
                        );
                    });
                    if ui.button("Apply").clicked() {
                            action = Some(SidePanelAction::SampleVoxelFeatureAware);
                    }

                    ui.add_space(1.5);
                    ui.separator();
                    ui.add_space(1.5);

                    ui.label("Normal Space Sampling");
                    ui.horizontal(|ui| {
                        ui.label("Cell Size");
                        ui.add(
                            egui::DragValue::new(&mut settings.normal_cell_size)
                                .speed(0.05)
                                .range(0.0..=f32::MAX)
                                .max_decimals(3),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Target   ");
                        ui.add(
                            egui::DragValue::new(&mut settings.normal_pnt_target)
                                .speed(0.40)
                                .range(0..=usize::MAX)
                                .max_decimals(0),
                        );
                    });
                    if ui.button("Apply").clicked() {
                            action = Some(SidePanelAction::SampleNormalSpace);
                    }

                    ui.add_space(1.5);
                    ui.separator();
                    ui.add_space(1.5);

                    ui.label("Farthest Point Sampling");
                    ui.horizontal(|ui| {
                        ui.label("Target   ");
                        ui.add(
                            egui::DragValue::new(&mut settings.farthest_pnt_target)
                                .speed(0.40)
                                .range(0..=usize::MAX)
                                .max_decimals(0),
                        );
                    });
                    if ui.button("Apply").clicked() {
                            action = Some(SidePanelAction::SampleFarthestPoint);
                    }
                }
            );
           
            // Colouring Methods
            egui::CollapsingHeader::new("Colouring")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("Estimated Normals");
                    ui.horizontal(|ui| {
                        ui.label("Cell Size");
                        ui.add(
                            egui::DragValue::new(&mut settings.colour_normal_cell_size)
                                .speed(0.05)
                                .range(0.0..=f32::MAX)
                                .max_decimals(3),
                        );
                        if ui.button("Apply").clicked() {
                            action = Some(SidePanelAction::ColourNormals);
                        }
                    });
                }
            );



            

        });
    });

    action
}

pub enum SidePanelAction {
    SamplePoission,
    SampleVoxelGrid,
    SampleVoxelFeatureAware,
    SampleNormalSpace,
    SampleFarthestPoint,

    ColourNormals,
}