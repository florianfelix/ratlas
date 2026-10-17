use crate::app::{
    app_state::StationLimit,
    ui::{Element, Message, UiState},
};
#[allow(unused)]
use iced::{
    Length,
    widget::{Space, button, column, container, pick_list, row, text, text_input},
};

fn wdgt_station_limit<'a>(state: &'a UiState) -> Element<'a> {
    pick_list(
        &StationLimit::ALL[..],
        Some(state.station_limit),
        Message::SetStationLimit,
    )
    .text_size(12)
    .padding([3, 8])
    .into()
}

fn wdgt_country_select<'a>(state: &'a UiState) -> Element<'a> {
    let mut wdgt = row![
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
        .padding([3, 8])
        .width(200.0),
    ];
    if state.station_filter.country_code.is_some() {
        let mut clear_filter = state.station_filter.clone();
        clear_filter.country_code = None;
        wdgt = wdgt.push(
            button(text("✕").size(10).align_y(iced::Alignment::Center))
                .padding([3, 6])
                .on_press(Message::StationFilterChanged(clear_filter)),
        );
    }

    wdgt.into()
}

fn wdgt_display_station_count<'a>(state: &'a UiState) -> Element<'a> {
    let count_str = format!(
        "({} / {})",
        state.displayed_stations_count, state.total_stations_count
    );
    text(count_str)
        .size(11)
        .align_y(iced::Alignment::Center)
        .into()
}

fn wdgt_earth_controls<'a>(state: &'a UiState) -> Element<'a> {
    let borders_label = if state.earth_config.show_country_outlines {
        "Borders: ON"
    } else {
        "Borders: OFF"
    };

    let borders_btn = button(
        text(borders_label)
            .size(11)
            .align_y(iced::Alignment::Center),
    )
    .padding([3, 8])
    .style(if state.earth_config.show_country_outlines {
        button::primary
    } else {
        button::secondary
    })
    .on_press(Message::ToggleCountryOutlines);

    let theme_label = format!("Earth: {}", state.earth_config.base_texture.label());
    let theme_btn = button(text(theme_label).size(11).align_y(iced::Alignment::Center))
        .padding([3, 8])
        .style(button::secondary)
        .on_press(Message::CycleEarthBaseTexture);

    row![borders_btn, theme_btn]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
}

pub fn toolbar<'a>(state: &'a UiState) -> Element<'a> {
    let left_controls = row![
        wdgt_station_limit(state),
        wdgt_display_station_count(state),
        wdgt_country_select(state),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let right_controls = wdgt_earth_controls(state);

    let content = row![
        left_controls,
        Space::new().width(Length::Fill),
        right_controls,
    ]
    .align_y(iced::Alignment::Center);

    container(content)
        .style(container::bordered_box)
        .width(Length::Fill)
        .padding(6)
        .into()
}
