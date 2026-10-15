use iced::widget::{column, container, text};

use crate::app::ui::{Element, UiState};
// use crate::playback::PlaybackStatus;

pub fn viewport_hud<'a>(_state: &'a UiState) -> Element<'a> {
    let content = column![
        text("Viewport").size(12),
        text("LMB: Orbit | Click: Play / Select | Scroll: Zoom").size(11),
    ]
    .spacing(4)
    .max_width(260.0);

    // if let Some(status) = &state.playback_status {
    //     match status {
    //         PlaybackStatus::Playing { name, .. } => {
    //             content = content
    //                 .push(container(text(format!("▶ Now Playing: {name}")).size(11)).padding(2));
    //         }
    //         PlaybackStatus::Loading { name, .. } => {
    //             content = content
    //                 .push(container(text(format!("⏳ Buffering: {name}")).size(11)).padding(2));
    //         }
    //         PlaybackStatus::Paused { name, .. } => {
    //             content =
    //                 content.push(container(text(format!("⏸ Paused: {name}")).size(11)).padding(2));
    //         }
    //         _ => {}
    //     }
    // }

    container(content).padding(6).into()
}
