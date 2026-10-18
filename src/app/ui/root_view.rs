use iced::widget::{column, container, float, row, stack, text};

use crate::app::ui::{
    Element, Ui, UiState,
    components::{inspector, toolbar, viewport_hud},
};

impl<'a> Ui {
    pub fn root_view(&self, state: &'a UiState) -> Element<'a> {
        let top_bar = toolbar(state);
        let hud = viewport_hud(state);
        let right_panel = inspector(state);

        let body = row![
            hud,
            container(row![]).width(iced::Length::Fill),
            right_panel,
        ]
        .width(iced::Length::Fill)
        .height(iced::Length::Fill);

        let content = column![top_bar, body]
            .width(iced::Length::Fill)
            .height(iced::Length::Fill);

        // Stack the base UI and the floating tooltip over the entire window
        stack![
            container(content)
                .width(iced::Length::Fill)
                .height(iced::Length::Fill),
            tooltip(state),
        ]
        .into()
    }
}

fn tooltip<'a>(state: &'a UiState) -> Element<'a> {
    if let (Some(station_name), Some(pos)) = (&state.hovered_station_name, state.mouse_pos) {
        let tooltip_offset_x = 12.0; // small offset so the tooltip doesn't sit directly under cursor
        let tooltip_offset_y = 12.0;

        let content = container(
            text(station_name)
                .size(12)
                .wrapping(text::Wrapping::WordOrGlyph),
        )
        .max_width(260)
        .padding(6)
        .style(container::rounded_box);

        float(content)
            .translate(move |content_bounds, _viewport| {
                // Subtract content_bounds to cancel any parent position and snap to absolute pos
                iced::Vector::new(
                    pos.x - content_bounds.x + tooltip_offset_x,
                    pos.y - content_bounds.y + tooltip_offset_y,
                )
            })
            .into()
    } else {
        row![].into()
    }
}
