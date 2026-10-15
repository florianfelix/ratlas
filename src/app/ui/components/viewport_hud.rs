use iced::widget::{column, container, text};

use crate::app::ui::{Element, UiState};
// use crate::playback::PlaybackStatus;

pub fn viewport_hud<'a>(_state: &'a UiState) -> Element<'a> {
    let content = column![text("LMB: Orbit | Click: Play / Select | Scroll: Zoom").size(11),]
        .spacing(4)
        .max_width(260.0);

    container(content).padding(6).into()
}
