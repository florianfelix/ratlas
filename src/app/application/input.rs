use std::sync::Arc;
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, MouseScrollDelta},
    window::Window,
};

use crate::app::{
    app_state::{AppState, Camera},
    application::types::Channels,
};

/// Tracks mouse cursor and drag state within the 3D viewport canvas.
#[derive(Debug, Default, Clone, Copy)]
pub struct MouseTracker {
    pub is_pressed: bool,
    pub press_pos: Option<(f64, f64)>,
    pub last_pos: Option<(f64, f64)>,
}

impl MouseTracker {
    pub fn reset(&mut self) {
        self.is_pressed = false;
        self.press_pos = None;
        self.last_pos = None;
    }
}

pub fn handle_mouse_input(
    button: MouseButton,
    button_state: ElementState,
    state: &mut AppState,
    channels: &Channels,
    window: &Arc<Window>,
    mouse: &mut MouseTracker,
) {
    if button != MouseButton::Left {
        return;
    }

    let pressed = button_state == ElementState::Pressed;
    mouse.is_pressed = pressed;
    if pressed {
        mouse.press_pos = mouse.last_pos;
    } else {
        state.camera.stop_drag();

        // If released with minimal mouse travel, treat as a click / selection
        if let (Some(press_pos), Some(release_pos)) = (mouse.press_pos, mouse.last_pos) {
            let dx = (release_pos.0 - press_pos.0) as f32;
            let dy = (release_pos.1 - press_pos.1) as f32;
            let dist_sq = dx * dx + dy * dy;

            if dist_sq < 25.0 {
                // Less than 5px movement: process click
                let physical_size = window.inner_size();
                let ray = state
                    .camera
                    .screen_to_ray(release_pos, (physical_size.width, physical_size.height));
                let clicked_station = state.pick_station(&ray).cloned();
                if let Some(ref station) = clicked_station {
                    state.stations.set_playing_station(Some(station.stationuuid.clone()));
                    let stream_url = if !station.url_resolved.is_empty() {
                        station.url_resolved.clone()
                    } else {
                        station.url.clone()
                    };
                    let _ = channels
                        .tx_playback
                        .send(crate::playback::PlaybackMessage::Play {
                            url: stream_url,
                            name: station.name.clone(),
                        });
                }
                state.set_selected_station(clicked_station.as_ref().map(|s| s.stationuuid.clone()));
                window.request_redraw();
            }
        }

        mouse.press_pos = None;
        mouse.last_pos = None;
        window.request_redraw();
    }
}

pub fn handle_cursor_moved(
    position: PhysicalPosition<f64>,
    state: &mut AppState,
    window: &Arc<Window>,
    mouse: &mut MouseTracker,
) {
    let current_pos = (position.x, position.y);
    // Set mouse position on AppState
    state.mouse_pos = Some(glam::Vec2 {
        x: position.x as f32,
        y: position.y as f32,
    });
    let physical_size = window.inner_size();
    let ray = state
        .camera
        .screen_to_ray(current_pos, (physical_size.width, physical_size.height));

    // Update 3D point on globe under cursor for mouse lens effector
    state.mouse_globe_pos = ray
        .intersect_sphere(glam::Vec3::ZERO, Camera::SPHERE_RADIUS)
        .map(|t| ray.origin + ray.direction * t);

    if mouse.is_pressed {
        if let Some((prev_x, prev_y)) = mouse.last_pos {
            let delta_x = (current_pos.0 - prev_x) as f32;
            let delta_y = (current_pos.1 - prev_y) as f32;

            // Rotate camera around target with distance-scaled sensitivity and momentum
            state.camera.rotate_around_target(delta_x, delta_y);
            window.request_redraw();
        }
    } else {
        // Hover detection when mouse is moving without dragging
        let hovered_uuid = state.pick_station(&ray).map(|s| s.stationuuid.clone());
        let hover_changed = state.set_hovered_station(hovered_uuid);
        // Only run apply_effectors if set_hovered_station didn't already trigger it
        if !hover_changed {
            state.apply_effectors();
        }
        window.request_redraw();
    }

    mouse.last_pos = Some(current_pos);
}

pub fn handle_mouse_wheel(delta: MouseScrollDelta, state: &mut AppState, window: &Arc<Window>) {
    let zoom_amount = match delta {
        MouseScrollDelta::LineDelta(_x, y) => y * 0.25,
        MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) * 0.005,
    };
    state.camera.zoom(zoom_amount);
    window.request_redraw();
}

pub fn handle_cursor_left(state: &mut AppState, window: &Arc<Window>, mouse: &mut MouseTracker) {
    mouse.reset();
    state.mouse_globe_pos = None;
    state.camera.stop_drag();
    state.set_hovered_station(None);
    state.apply_effectors();
    window.request_redraw();
}
