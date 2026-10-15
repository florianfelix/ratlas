use iced_wgpu::wgpu;
use std::sync::Arc;
use winit::window::Window;

use crate::app::{
    app_state::{AppState, canvas_renderer::CanvasRenderer, gpu::Gpu},
    ui::IcedUi,
};

/// Renders the 3D scene and Iced UI overlay onto the GPU surface.
/// This function is strictly for drawing and performs no state mutations.
pub fn render_frame(
    state: &AppState,
    gpu: &mut Gpu,
    canvas_renderer: &CanvasRenderer,
    ui: &mut IcedUi,
    viewport: &iced_wgpu::graphics::Viewport,
    window: &Arc<Window>,
) {
    let physical_size = window.inner_size();
    if physical_size.width == 0 || physical_size.height == 0 {
        return;
    }

    // 1. Update 3D viewport canvas with camera and world objects
    let aspect_ratio = physical_size.width as f32 / physical_size.height as f32;
    canvas_renderer.update(&state.camera, &state.world.objects, aspect_ratio);

    // 2. Render scene from AppState world + UI overlay
    let surface_format = gpu.surface_config.format;
    match gpu.render_frame(canvas_renderer, &state.world.objects, |view| {
        ui.draw(surface_format, view, viewport);
    }) {
        Ok(_) => {}
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            let size = window.inner_size();
            gpu.handle_resize(size);
            window.request_redraw();
        }
        Err(wgpu::SurfaceError::OutOfMemory) => {
            std::process::exit(1);
        }
        Err(e) => {
            tracing::error!("Surface error: {:?}", e);
        }
    }
}
