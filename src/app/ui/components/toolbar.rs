use crate::app::{
    app_state::StationLimit,
    ui::{Element, Message, UiState},
};
use iced::{
    Length,
    widget::{button, container, pick_list, row, text},
};

pub fn toolbar<'a>(state: &'a UiState) -> Element<'a> {
    let mut left_controls = row![
        text("Stations:").size(12).align_y(iced::Alignment::Center),
        pick_list(
            &StationLimit::ALL[..],
            Some(state.station_limit),
            Message::SetStationLimit,
        )
        .text_size(12)
        .padding([3, 8]),
        text("Country:").size(12).align_y(iced::Alignment::Center),
        pick_list(
            isocountry::CountryCode::as_array().as_slice(),
            state.station_filter.country_code,
            {
                let filter = state.station_filter.clone();
                move |selected| {
                    let mut f = filter.clone();
                    f.country_code = Some(selected);
                    Message::StationFilterChanged(f)
                }
            },
        )
        .placeholder("All Countries")
        .text_size(12)
        .padding([3, 8]),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    if state.station_filter.country_code.is_some() {
        let mut clear_filter = state.station_filter.clone();
        clear_filter.country_code = None;
        left_controls = left_controls.push(
            button(text("✕").size(10).align_y(iced::Alignment::Center))
                .padding([3, 6])
                .on_press(Message::StationFilterChanged(clear_filter)),
        );
    }

    if state.total_stations_count > 0 {
        let count_str = format!(
            "({} / {})",
            state.displayed_stations_count, state.total_stations_count
        );
        left_controls =
            left_controls.push(text(count_str).size(11).align_y(iced::Alignment::Center));
    }

    let mut content = row![left_controls]
        .spacing(12)
        .align_y(iced::Alignment::Center)
        .width(Length::Fill);

    if let Some(hovered) = &state.hovered_station_name {
        content = content
            .push(container(row![]).width(Length::Fill))
            .push(container(text(hovered).size(13).align_y(iced::Alignment::Center)).padding(2));
    }

    container(content)
        .style(container::bordered_box)
        .width(Length::Fill)
        .padding(6)
        .into()
}
