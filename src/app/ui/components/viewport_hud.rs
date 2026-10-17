#[allow(unused)]
use iced::widget::{column, container, row, space, text};

use crate::app::ui::{
    Element, UiState,
    components::tags::{wdgt_tag_selected, wdgt_tag_selector},
};

pub fn viewport_hud<'a>(state: &'a UiState) -> Element<'a> {
    let mut parent = column![
        container(wdgt_tag_selector(state),)
            .padding(6)
            .style(container::rounded_box),
    ];

    let selected_tags = column![
        wdgt_tag_selected(state),
        // wdgt_hovered_station(state),
    ]
    .spacing(4)
    .width(260.0);

    let selected_tags = container(selected_tags)
        .style(container::rounded_box)
        .padding(6);

    parent = parent.push(selected_tags);
    container(parent).into()
}

fn wdgt_hovered_station<'a>(state: &'a UiState) -> Element<'a> {
    let mut wdgt = row![];
    if let Some(hovered) = &state.hovered_station_name {
        wdgt = wdgt.push(container(row![])).push(
            container(
                text(format!("{:}", hovered))
                    .size(13)
                    .align_y(iced::Alignment::Start),
            )
            .padding(2),
        );
    }

    wdgt.into()
}
