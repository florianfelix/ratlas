use iced::widget::{button, container, row, text};

use crate::app::ui::{Element, Message, UiState};

pub fn toolbar<'a>(_state: &'a UiState) -> Element<'a> {
    let content = row![
        text("Ratlas").size(16),
        button("Reset Camera").on_press(Message::ResetCamera),
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center);

    container(content).padding(8).into()
}
