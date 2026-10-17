use iced::widget::{button, column, container, row, text};

use crate::app::ui::{Element, Message, UiState};
use crate::playback::PlaybackStatus;
// use radiobrowser::ApiStation;

pub fn inspector<'a>(state: &'a UiState) -> Element<'a> {
    let mut content = column![text("Station Details").size(18)].spacing(6);

    if let Some(ref station) = state.playing_station {
        content = content.push(column![
            text(&station.name).size(14),
            text(&station.country).size(10),
        ]);
    }

    match &state.playback_status {
        Some(PlaybackStatus::Playing { .. }) => {
            content = content
                // .push(text(format!("▶ Playing: {name}")).size(12))
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
        Some(PlaybackStatus::Paused { .. }) => {
            content = content
                // .push(text(format!("⏸ Paused: {name}")).size(12))
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
        Some(PlaybackStatus::Loading { .. }) => {
            content = content.push(text("⏳ Connecting ...").size(12)).push(
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

    content = content.push(wdgt_hovered_station(state));

    container(content)
        .style(container::bordered_box)
        .width(210)
        .padding(12)
        .into()
}

fn wdgt_hovered_station<'a>(state: &'a UiState) -> Element<'a> {
    let mut wdgt = row![];
    if let Some(hovered) = &state.hovered_station_name {
        wdgt = wdgt.push(container(row![])).push(
            container(
                text(format!("{:}", hovered))
                    .size(13)
                    .align_y(iced::Alignment::Start),
            )
            .padding(2),
        );
    }

    wdgt.into()
}
