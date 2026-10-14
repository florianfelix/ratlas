#![allow(unused)]

use crate::app::ui::{Element, Message, Ui, UiState};
use iced::widget::{button, column, container, row, text};

impl Ui {
    pub fn root_view<'a>(&self, state: &'a UiState) -> Element<'a> {
        row![
            sidebar(state),
            // other panels or overlays...
        ]
        .into()
    }
}

pub fn sidebar<'a>(state: &'a UiState) -> Element<'a> {
    let content = column![
        text("Scene Controls").size(18),
        // button("Reset Camera").on_press(Message::ResetCamera),
        // text(format!("Selected Object: {}", state.selected_object)),
    ]
    .spacing(10);

    container(content).width(240).padding(10).into()
}
