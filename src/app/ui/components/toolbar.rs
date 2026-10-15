#![allow(unused)]
use crate::app::ui::{Element, Message, UiState};
use iced::widget::{button, container, row, text};

pub fn toolbar<'a>(state: &'a UiState) -> Element<'a> {
    let mut content = row![
        // text("Ratlas").size(16),
        // button("Reset Camera").on_press(Message::ResetCamera),
    ]
    .spacing(12)
    .height(12.0)
    .align_y(iced::Alignment::Center);

    if let Some(hovered) = &state.hovered_station_name {
        content = content.push(container(text(hovered).size(12)).padding(2));
    }

    container(content).padding(8).into()
}
