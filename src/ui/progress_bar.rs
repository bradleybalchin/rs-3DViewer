use eframe::egui;

pub fn progress_bar(ui: &mut egui::Ui, name: &str, status: &str){

    egui::Modal::new(egui::Id::unique("progress_bar"))
    .show(ui, |ui| {
        ui.heading(name);
        ui.add_space(1.5);
        ui.label(status);
        ui.add_space(5.5);
        ui.add(egui::ProgressBar::new(0.1).show_percentage());
    });

}