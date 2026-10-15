#![allow(unused)]
use crate::app::ui::{Element, Message, UiState};
use iced::{
    Length, alignment,
    widget::{button, container, row, text},
};

pub fn toolbar<'a>(state: &'a UiState) -> Element<'a> {
    let mut content = row![]
        .spacing(12)
        .height(12.0)
        .align_y(iced::Alignment::Center);

    if let Some(hovered) = &state.hovered_station_name {
        content = content
            .push(container(text(hovered).size(14).align_y(iced::Alignment::Center)).padding(2));
    }

    container(content)
        .style(container::primary)
        .width(Length::Fill)
        .padding(8)
        .into()
}
