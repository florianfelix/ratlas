use iced::widget::{column, container, text};

use crate::app::ui::{Element, UiState};

pub fn viewport_hud<'a>(state: &'a UiState) -> Element<'a> {
    let mut content = column![
        text("Viewport").size(12),
        text("LMB: Orbit | Click: Select | Scroll: Zoom").size(11),
    ]
    .spacing(4);

    if let Some(hovered) = &state.hovered_station_name {
        content = content.push(container(text(format!("Hovered: {hovered}")).size(12)).padding(4));
    }

    container(content).padding(6).into()
}
