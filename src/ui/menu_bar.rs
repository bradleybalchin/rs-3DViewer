// what the menu can ask the app to do
pub enum MenuAction {
    OpenFile,
}

// top menu bar
pub fn menu_bar(ui: &mut egui::Ui) -> Option<MenuAction> {
    let mut action = None;

    egui::Panel::top("menu").show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open").clicked() {
                    ui.close();
                    action = Some(MenuAction::OpenFile);
                }
            });
        });
    });

    action
}