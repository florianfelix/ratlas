use iced::Task;
use tracing::info;

use crate::app::app_state::{EarthBaseTexture, StationFilter, StationLimit, TagMatchMode};

use super::UiState;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Message {
    ButtonClicked,
    ResetCamera,
    RefreshStations,
    SetStationLimit(StationLimit),
    StationFilterChanged(StationFilter),
    TagSearchInputChanged(String),
    AddTagFilter(String),
    RemoveTagFilter(String),
    SetTagMatchMode(TagMatchMode),
    ToggleTagMatchMode,
    ToggleCountryOutlines,
    SetShowCountryOutlines(bool),
    SetOutlineColor([f32; 4]),
    CycleEarthBaseTexture,
    SetEarthBaseTexture(EarthBaseTexture),
    TogglePlayback,
    StopPlayback,
    PlayStation { url: String, name: String },
}

/// Handles UI state transitions driven by Iced messages
#[tracing::instrument(skip_all, name = "handle_message")]
pub fn handle_message(state: &mut UiState, message: Message) -> Task<Message> {
    info!("Handling UI Message: {:?}", message);

    match message {
        Message::ButtonClicked => {
            state.counter += 1;
        }
        Message::ResetCamera => {
            // Can trigger camera reset or set dirty flags
        }
        Message::RefreshStations => {
            // Handled at application-level to dispatch to radio worker channel
        }
        Message::SetStationLimit(limit) => {
            state.station_limit = limit;
        }
        Message::StationFilterChanged(filter) => state.station_filter = filter,
        Message::TagSearchInputChanged(query) => {
            state.tag_search_query = query;
        }
        Message::AddTagFilter(tag) => {
            if !state
                .station_filter
                .tags
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&tag))
            {
                state.station_filter.tags.push(tag);
            }
            state.tag_search_query.clear();
        }
        Message::RemoveTagFilter(tag) => {
            state
                .station_filter
                .tags
                .retain(|t| !t.eq_ignore_ascii_case(&tag));
        }
        Message::SetTagMatchMode(mode) => {
            state.station_filter.tag_match_mode = mode;
        }
        Message::ToggleTagMatchMode => {
            state.station_filter.tag_match_mode = match state.station_filter.tag_match_mode {
                TagMatchMode::And => TagMatchMode::Or,
                TagMatchMode::Or => TagMatchMode::And,
            };
        }
        Message::ToggleCountryOutlines => {
            state.earth_config.show_country_outlines = !state.earth_config.show_country_outlines;
        }
        Message::SetShowCountryOutlines(show) => {
            state.earth_config.show_country_outlines = show;
        }
        Message::SetOutlineColor(color) => {
            state.earth_config.outline_color = glam::Vec4::from_array(color);
        }
        Message::CycleEarthBaseTexture => {
            let next_tex = match state.earth_config.base_texture {
                EarthBaseTexture::Day => EarthBaseTexture::Night,
                EarthBaseTexture::Night => EarthBaseTexture::Specular,
                EarthBaseTexture::Specular => EarthBaseTexture::Day,
            };
            state.earth_config.base_texture = next_tex;
        }
        Message::SetEarthBaseTexture(tex) => {
            state.earth_config.base_texture = tex;
        }
        Message::TogglePlayback | Message::StopPlayback | Message::PlayStation { .. } => {
            // Application-level worker handles playback messages
        }
    }

    Task::none()
}
