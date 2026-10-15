use iced::widget::{column, container, text};

use crate::app::ui::{Element, UiState};

pub fn inspector<'a>(state: &'a UiState) -> Element<'a> {
    let mut content = column![text("Station Details").size(16),].spacing(6);

    if let Some(station) = &state.selected_station {
        content = content
            .push(text(&station.name).size(14))
            .push(
                text(format!(
                    "Country: {}",
                    if station.country.is_empty() {
                        "Unknown"
                    } else {
                        &station.country
                    }
                ))
                .size(12),
            )
            .push(text(format!("Clicks: {}", station.clickcount)).size(12))
            .push(
                text(format!(
                    "Codec: {} ({} kbps)",
                    station.codec, station.bitrate
                ))
                .size(12),
            );

        if let (Some(lat), Some(lon)) = (station.geo_lat, station.geo_long) {
            content = content.push(text(format!("Pos: {:.2}°, {:.2}°", lat, lon)).size(11));
        }
    } else {
        content = content
            .push(text("Click a station pin to view details.").size(12))
            .push(text(format!("Button Clicks: {}", state.counter)).size(12));
    }

    container(content).width(200).padding(12).into()
}
