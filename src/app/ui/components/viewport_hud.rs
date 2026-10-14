use iced::widget::{column, container, text};

use crate::app::ui::{Element, UiState};

pub fn viewport_hud<'a>(_state: &'a UiState) -> Element<'a> {
    let content = column![
        text("Viewport").size(12),
        text("LMB: Orbit | Scroll: Zoom").size(11),
    ]
    .spacing(4);

    container(content).padding(6).into()
}
