#[allow(unused)]
use iced::widget::{button, column, container, row, space, text};

use crate::app::ui::root_view::station_display;
use crate::app::ui::{Element, UiState};

pub fn inspector<'a>(state: &'a UiState) -> Element<'a> {
    let mut content = column![].spacing(6);

    if let Some(ref station) = state.playing_station {
        content = content.push(station_display(station));
        container(content)
            .style(container::bordered_box)
            .width(210)
            .padding(12)
            .into()
    } else {
        container(row![]).into()
    }
}
