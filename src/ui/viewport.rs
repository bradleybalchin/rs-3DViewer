
use eframe::egui::{self, Color32, Rect, Sense, TextureHandle, TextureOptions, pos2};
use crate::app::Settings;
use crate::render::renderer::VulkanRenderer;
use crate::render::vertex::GpuPoint;

/// Main viewport where the rendered scene is shown.
///
/// Owns everything needed to draw the scene: the Vulkan renderer and the egui texture each
/// frame is uploaded into. Camera state will also live here, since it is driven by mouse
/// input on the viewport.
pub struct Viewport {
    renderer : Option<VulkanRenderer>,          // vulkan renderer (None if vulkan failed to initialise)
    texture : Option<TextureHandle>,            // last rendered frame, shown in the viewport
}

impl Viewport {

    pub fn new() -> Self {
        let renderer = VulkanRenderer::new(cfg!(debug_assertions))
            .map_err(|e| eprintln!("Vulkan unavailable: {e}"))
            .ok();
        Self { renderer, texture : None }
    }

    /// Replace the points being drawn
    pub fn set_points(&mut self, points : &[GpuPoint]) -> Result<(), String> {
        match &mut self.renderer {
            Some(renderer) => renderer.upload_vertices(points),
            None => Ok(()),     // nothing to upload to without a renderer
        }
    }

    /// Render the scene and draw it into the remaining space of the window
    pub fn ui(&mut self, ui : &mut egui::Ui, settings : &Settings) {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            // retrieve response and painter for the remaining part of the screen
            // response will be used for camera input (drag to orbit, scroll to zoom)
            let size = ui.available_size();
            let (response, painter) = ui.allocate_painter(size, Sense::click_and_drag());
            let rect = response.rect;

            let [r,g,b] = settings.background;

            // no renderer (vulkan failed to initialise), just fill with background colour
            let Some(renderer) = &mut self.renderer else {
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
                    match &mut self.texture {
                        Some(tex) => tex.set(image, TextureOptions::NEAREST),
                        None => self.texture = Some(ui.ctx().load_texture("vulkan_viewport", image, TextureOptions::NEAREST)),
                    }
                }
                Err(e) => eprintln!("Vulkan render failed: {e}"),
            }

            if let Some(tex) = &self.texture {
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                painter.image(tex.id(), rect, uv, Color32::WHITE);
            }
        });
    }
}
