use iced::widget::{column, container, row, text};

use crate::app::ui::{Element, UiState, components::toolbar::wdgt_tag_selected};
// use crate::playback::PlaybackStatus;

pub fn viewport_hud<'a>(state: &'a UiState) -> Element<'a> {
    let mut content = column![].spacing(4).max_width(260.0);
    content = content.push(wdgt_tag_selected(state));
    content = content.push(wdgt_hovered_station(state));
    container(content).padding(6).into()
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
