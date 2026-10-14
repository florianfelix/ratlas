use iced::widget::{column, container, row};

use crate::app::ui::{
    Element, Ui, UiState,
    components::{inspector, toolbar, viewport_hud},
};

impl<'a> Ui {
    pub fn root_view(&self, state: &'a UiState) -> Element<'a> {
        // Top section: Toolbar + Viewport HUD on left, Inspector on right
        let top_bar = toolbar(state);
        let hud = viewport_hud(state);
        let right_panel = inspector(state);

        // Layout: Top bar on top, body row with HUD and right-aligned Inspector
        let body = row![
            hud,
            container(row![]).width(iced::Length::Fill), // Transparent viewport space
            right_panel,
        ]
        .width(iced::Length::Fill)
        .height(iced::Length::Fill);

        let content = column![top_bar, body]
            .width(iced::Length::Fill)
            .height(iced::Length::Fill);

        container(content)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .into()
    }
}
