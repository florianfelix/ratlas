use iced_wgpu::wgpu;
use std::sync::Arc;
use winit::window::Window;

use crate::app::{
    app_state::{AppState, canvas_renderer::CanvasRenderer, gpu::Gpu},
    application::types::Channels,
    ui::IcedUi,
};

pub fn render_frame(
    state: &mut AppState,
    gpu: &mut Gpu,
    canvas_renderer: &mut CanvasRenderer,
    ui: &mut IcedUi,
    channels: &Channels,
    window: &Arc<Window>,
) {
    let physical_size = window.inner_size();
    if physical_size.width == 0 || physical_size.height == 0 {
        return;
    }

    // Smoothly update camera towards target angles/radius with damping
    if state.camera.update() {
        window.request_redraw();
    }

    // Apply dynamic effectors based on current camera and mouse positions
    state.apply_effectors();

    // Sync hovered / selected station details and playback status to UI overlay state
    ui.state.hovered_station_name = state
        .hovered_station
        .as_ref()
        .and_then(|id| state.find_station(id))
        .map(|s| s.name.clone());
    ui.state.selected_station = state
        .selected_station
        .as_ref()
        .and_then(|id| state.find_station(id))
        .cloned();
    ui.state.playback_status = Some(state.playback_status.clone());

    // 1. Update UI overlay and dispatch messages directly to AppState or playback channel
    let ui_result = ui.update(window);
    for message in &ui_result.messages {
        match message {
            crate::app::ui::Message::TogglePlayback => {
                let _ = channels
                    .tx_playback
                    .send(crate::playback::PlaybackMessage::TogglePlayPause);
            }
            crate::app::ui::Message::StopPlayback => {
                let _ = channels
                    .tx_playback
                    .send(crate::playback::PlaybackMessage::Stop);
            }
            crate::app::ui::Message::PlayStation { url, name } => {
                let _ = channels
                    .tx_playback
                    .send(crate::playback::PlaybackMessage::Play {
                        url: url.clone(),
                        name: name.clone(),
                    });
            }
            _ => {
                state.handle_message(message);
            }
        }
        window.request_redraw();
    }

    // 2. Update 3D viewport canvas with camera and world objects
    let aspect_ratio = physical_size.width as f32 / physical_size.height as f32;
    canvas_renderer.update(&state.camera, &state.world.objects, aspect_ratio);

    // 3. Render scene from AppState world + UI overlay
    let surface_format = gpu.surface_config.format;
    match gpu.render_frame(canvas_renderer, &state.world.objects, |view| {
        ui.draw(surface_format, view, &ui_result.viewport);
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
