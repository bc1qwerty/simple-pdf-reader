pub mod cache;
pub mod pipeline;

use crate::backend::RenderedPage;
use egui::{Context, TextureHandle, TextureOptions};

/// Convert rendered RGBA pixels to an egui GPU texture
pub fn pixels_to_texture(
    ctx: &Context,
    rendered: &RenderedPage,
    name: &str,
) -> TextureHandle {
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [rendered.width as usize, rendered.height as usize],
        &rendered.pixels,
    );
    ctx.load_texture(name, image, TextureOptions::LINEAR)
}
