#[allow(unused)]
use iced::widget::{column, container, row, space, text};

use crate::app::ui::{
    Element, UiState,
    components::tags::{wdgt_tag_selected, wdgt_tag_selector},
};

pub fn viewport_hud<'a>(state: &'a UiState) -> Element<'a> {
    let mut parent = column![];

    let selector = container(wdgt_tag_selector(state))
        .padding(6)
        .style(container::bordered_box);
    parent = parent.push(selector);
    parent = parent.push(space().height(8));

    let boxed_selected = if !state.station_filter.tags.is_empty() {
        container(wdgt_tag_selected(state))
            .padding(6)
            .style(container::bordered_box)
            .max_width(260.0)
    } else {
        container(row![])
    };
    parent = parent.push(boxed_selected);

    container(parent).into()
}
