
use eframe::egui::{self, Color32, Rect, Sense, TextureHandle, TextureOptions, pos2};
use crate::app::Settings;
use crate::render::vulkan::VulkanRenderer;

// Main viewport where rendered scene is shown
pub fn viewport(
    _ui : &mut egui::Ui,
    settings : &Settings,
    viewport_size : &mut [f32;2],
    renderer : Option<&mut VulkanRenderer>,
    texture : &mut Option<TextureHandle>,
) {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(_ui, |ui| {
            // retreive response and painter for specific part of the screen
            let size = ui.available_size();
            let (response, painter) = ui.allocate_painter(size, Sense::click_and_drag());

            // set viewport size to painter area of screen
            let rect = response.rect;
            *viewport_size = [rect.width(), rect.height()];

            let [r,g,b] = settings.background;

            // no renderer (vulkan failed to initialise), just fill with background colour
            let Some(renderer) = renderer else {
                painter.rect_filled(rect, 0.0, Color32::from_rgb(r, g, b));
                return;
            };

            // render at physical pixel resolution
            let ppp = ui.pixels_per_point();
            let width = (rect.width() * ppp).round().max(1.0) as u32;
            let height = (rect.height() * ppp).round().max(1.0) as u32;
            let clear = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0];

            match renderer.render(width, height, clear) {
                Ok(pixels) => {
                    // upload the frame into an egui texture
                    let image = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], pixels);
                    match texture {
                        Some(tex) => tex.set(image, TextureOptions::NEAREST),
                        None => *texture = Some(ui.ctx().load_texture("vulkan_viewport", image, TextureOptions::NEAREST)),
                    }
                }
                Err(e) => eprintln!("Vulkan render failed: {e}"),
            }

            if let Some(tex) = texture {
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                painter.image(tex.id(), rect, uv, Color32::WHITE);
            }
        });
    }
