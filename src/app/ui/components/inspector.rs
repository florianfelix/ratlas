use iced::widget::{button, column, container, row, text};

use crate::app::ui::{Element, Message, UiState};
use crate::playback::PlaybackStatus;

pub fn inspector<'a>(state: &'a UiState) -> Element<'a> {
    let mut content = column![text("Station Details").size(16)].spacing(6);

    if let Some(station) = &state.selected_station {
        content = content
            .push(text(&station.name).size(14))
            .push(
                text(format!(
                    "Country: {}",
                    if station.country.is_empty() {
                        "Unknown"
                    } else {
                        &station.country
                    }
                ))
                .size(12),
            )
            .push(text(format!("Clicks: {}", station.clickcount)).size(12))
            .push(
                text(format!(
                    "Codec: {} ({} kbps)",
                    station.codec, station.bitrate
                ))
                .size(12),
            );

        if let (Some(lat), Some(lon)) = (station.geo_lat, station.geo_long) {
            content = content.push(text(format!("Pos: {:.2}°, {:.2}°", lat, lon)).size(11));
        }

        // Direct Play button for selected station
        let stream_url = if !station.url_resolved.is_empty() {
            station.url_resolved.clone()
        } else {
            station.url.clone()
        };

        content = content.push(
            button("▶ Play Stream")
                .on_press(Message::PlayStation {
                    url: stream_url,
                    name: station.name.clone(),
                })
                .padding([4, 8]),
        );
    } else {
        content = content
            .push(text("Click a station pin to view details and start playback.").size(12))
            .push(text(format!("Button Clicks: {}", state.counter)).size(12));
    }

    // Playback status and control section
    content = content.push(text("Audio Playback").size(14));

    match &state.playback_status {
        Some(PlaybackStatus::Playing { name, .. }) => {
            content = content
                .push(text(format!("▶ Playing: {name}")).size(12))
                .push(
                    row![
                        button("⏸ Pause")
                            .on_press(Message::TogglePlayback)
                            .padding([3, 6]),
                        button("⏹ Stop")
                            .on_press(Message::StopPlayback)
                            .padding([3, 6]),
                    ]
                    .spacing(6),
                );
        }
        Some(PlaybackStatus::Paused { name, .. }) => {
            content = content
                .push(text(format!("⏸ Paused: {name}")).size(12))
                .push(
                    row![
                        button("▶ Resume")
                            .on_press(Message::TogglePlayback)
                            .padding([3, 6]),
                        button("⏹ Stop")
                            .on_press(Message::StopPlayback)
                            .padding([3, 6]),
                    ]
                    .spacing(6),
                );
        }
        Some(PlaybackStatus::Loading { name, .. }) => {
            content = content
                .push(text(format!("⏳ Connecting to {name}...")).size(12))
                .push(
                    button("⏹ Cancel")
                        .on_press(Message::StopPlayback)
                        .padding([3, 6]),
                );
        }
        Some(PlaybackStatus::Error { message }) => {
            content = content
                .push(text(format!("⚠ Error: {message}")).size(11))
                .push(
                    button("Clear")
                        .on_press(Message::StopPlayback)
                        .padding([3, 6]),
                );
        }
        Some(PlaybackStatus::Stopped) | None => {
            content = content.push(text("Status: Stopped").size(12));
        }
    }

    container(content).width(210).padding(12).into()
}
