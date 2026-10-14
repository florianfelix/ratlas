use iced::widget::{column, container, text};

use crate::app::ui::{Element, UiState};

pub fn inspector<'a>(state: &'a UiState) -> Element<'a> {
    let content = column![
        text("Properties").size(16),
        text(format!("Clicks: {}", state.counter)),
    ]
    .spacing(8);

    container(content).width(180).padding(12).into()
}
