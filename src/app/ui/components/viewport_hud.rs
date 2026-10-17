#[allow(unused)]
use iced::widget::{column, container, row, space, text};

use crate::app::ui::{
    Element, UiState,
    components::tags::{wdgt_tag_selected, wdgt_tag_selector},
};

pub fn viewport_hud<'a>(state: &'a UiState) -> Element<'a> {
    let mut parent = column![];

    let boxed_selected = container(wdgt_tag_selected(state)).max_width(260.0);

    parent = parent.push(wdgt_tag_selector(state));
    parent = parent.push(boxed_selected);

    container(parent)
        .padding(6)
        .style(container::rounded_box)
        .into()
}
