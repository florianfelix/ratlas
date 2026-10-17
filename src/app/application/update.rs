use std::sync::Arc;
use winit::window::Window;

use crate::app::{
    app_state::{AppState, canvas_renderer::CanvasRenderer},
    application::types::Channels,
    ui::{IcedUi, Message},
};

/// Updates the Iced UI overlay and dispatches any generated UI messages to AppState or playback channels.
/// Returns the computed UI viewport for subsequent rendering.
pub fn update_ui(
    ui: &mut IcedUi,
    state: &mut AppState,
    canvas_renderer: &CanvasRenderer,
    channels: &Channels,
    window: &Arc<Window>,
) -> iced_wgpu::graphics::Viewport {
    let ui_result = ui.update(window);
    for message in &ui_result.messages {
        match message {
            Message::TogglePlayback => {
                let _ = channels
                    .tx_playback
                    .send(crate::playback::PlaybackMessage::TogglePlayPause);
            }
            Message::StopPlayback => {
                state.set_playing_station(None);
                let _ = channels
                    .tx_playback
                    .send(crate::playback::PlaybackMessage::Stop);
            }
            Message::PlayStation { url, name } => {
                if let Some(station) = state
                    .all_radio_stations
                    .iter()
                    .chain(state.radio_stations.iter())
                    .find(|s| &s.url == url || &s.url_resolved == url)
                {
                    state.set_playing_station(Some(station.stationuuid.clone()));
                }
                let _ = channels
                    .tx_playback
                    .send(crate::playback::PlaybackMessage::Play {
                        url: url.clone(),
                        name: name.clone(),
                    });
            }
            Message::CycleEarthBaseTexture => {
                state.cycle_earth_base_texture(canvas_renderer);
            }
            Message::SetEarthBaseTexture(tex) => {
                state.set_earth_base_texture(canvas_renderer, *tex);
            }
            _ => {
                state.handle_message(message);
            }
        }
        window.request_redraw();
    }

    ui_result.viewport
}
