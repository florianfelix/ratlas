use iced::{
    Font,
    font::Weight,
    widget::{column, container, float, row, stack, text},
};

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
    if let (Some(station), Some(pos)) = (&state.hovered_station_full, state.mouse_pos) {
        let tooltip_offset_x = 12.0; // small offset so the tooltip doesn't sit directly under cursor
        let tooltip_offset_y = 12.0;

        //         let mut content = column![
        //             text(&station.name)
        //                 .size(12)
        //                 .wrapping(text::Wrapping::WordOrGlyph),
        //         ];
        //
        //         let mut tags = row![].spacing(6).padding(4);
        //         for t in station.tags.split(",").filter(|s| !s.trim().is_empty()) {
        //             tags = tags.push(
        //                 container(text(t.trim()).size(10).font(Font {
        //                     weight: Weight::Bold,
        //                     ..Font::default()
        //                 }))
        //                 .style(container::secondary)
        //                 .padding(2),
        //             );
        //         }
        //         content = content.push(tags.wrap());

        let content = station_display(station);
        let content = container(content)
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

pub fn station_display<'a>(station: &'a radiobrowser::ApiStation) -> Element<'a> {
    let mut content = column![
        text(&station.name)
            .size(12)
            .wrapping(text::Wrapping::WordOrGlyph),
        text(&station.country)
            .size(12)
            .wrapping(text::Wrapping::WordOrGlyph),
    ];

    let mut tags = row![].spacing(6).padding(4);
    for t in station.tags.split(",").filter(|s| !s.trim().is_empty()) {
        tags = tags.push(
            container(text(t.trim()).size(10).font(Font {
                weight: Weight::Bold,
                ..Font::default()
            }))
            .style(container::secondary)
            .padding(2),
        );
    }
    content = content.push(tags.wrap());

    content.into()
}
