
use eframe::egui::{self, Color32,Sense};
use crate::Settings;

pub fn viewport(_ui : &mut egui::Ui, settings : &Settings, viewport_size : &mut [f32;2]) {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(_ui, |ui| {
            // retreive response and painter for specific part of the screen
            let size = ui.available_size();
            let (response, painter) = ui.allocate_painter(size, Sense::click_and_drag());

            // set viewport size to painter area of screen
            let rect = response.rect;
            *viewport_size = [rect.width(), rect.height()];

            let [r,g,b] = settings.background;
            painter.rect_filled(rect, 0.0, Color32::from_rgb(r, g, b));
        });
    }