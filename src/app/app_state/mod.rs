use crate::{
    app::{
        app_state::{earth::EarthConfig, world::World},
        ui::Message,
    },
    data::{Effector, MarkerInstance},
};

pub mod camera;
pub mod canvas_renderer;
pub mod earth;
pub mod effectors;
pub mod gpu;
pub mod spatial_grid;
pub mod stations;
pub mod world;

pub use camera::{Camera, Ray};
pub use spatial_grid::{GlobeSpatialGrid, SpatialStation};
#[allow(unused_imports)]
pub use stations::{StationFilter, StationLimit, Stations, TagMatchMode};

pub struct AppState {
    pub world: World,
    pub camera: Camera,
    pub earth_config: EarthConfig,
    pub stations: Stations,
    pub marker_instances: Vec<MarkerInstance>,
    pub spatial_grid: GlobeSpatialGrid,
    pub mouse_globe_pos: Option<glam::Vec3>,
    pub mouse_pos: Option<glam::Vec2>,
    pub effectors: Vec<Effector>,
    pub playback_status: crate::playback::PlaybackStatus,
    pub active_theme: iced::Theme,
}

impl Default for AppState {
    fn default() -> Self {
        let effectors = vec![
            // 1. Camera distance proximity effector: scales markers down smoothly when zoomed in close
            // so dense clusters separate and don't overlap, while remaining visible when zoomed out.
            Effector::new_camera_proximity(0.25, 2.2, 0.15, 1.0),
            // 2. Mouse globe lens effector: magnifies markers under/around the cursor with an S-curve
            // and gives a subtle glowing tint.
            Effector::new_mouse_lens(0.02, 1.05, Some(glam::Vec4::new(0.2, 0.35, 0.5, 0.0))),
            // 3. Standalone picking effector: spatial selection sphere for hover and click picking
            Effector::new_picker(0.02),
        ];

        Self {
            world: World::new(),
            camera: Camera::default(),
            earth_config: EarthConfig::default(),
            stations: Stations::default(),
            marker_instances: Vec::new(),
            spatial_grid: GlobeSpatialGrid::default(),
            mouse_globe_pos: None,
            mouse_pos: None,
            effectors,
            playback_status: crate::playback::PlaybackStatus::Stopped,
            active_theme: iced::Theme::Moonfly,
        }
    }
}

impl AppState {
    /// Updates camera damping and applies dynamic effectors.
    pub fn update(&mut self, window: &winit::window::Window) {
        if self.camera.update() {
            window.request_redraw();
        }
        self.apply_effectors();
    }

    /// Handles application-level actions and mutations triggered by UI messages
    pub fn handle_message(&mut self, message: &Message) {
        match message {
            Message::ResetCamera => {
                self.camera.reset();
            }
            Message::ButtonClicked => {
                // Application-level logic when button is clicked
            }
            Message::RefreshStations => {
                // Handled in update_ui / channels
            }
            Message::SetStationLimit(limit) => {
                self.set_station_limit(*limit);
            }
            Message::StationFilterChanged(filter) => {
                self.set_station_filter(filter.clone());
            }
            Message::TagSearchInputChanged(_) => {
                // Handled in UI state
            }
            Message::AddTagFilter(tag) => {
                if self.stations.add_tag_filter(tag.clone()) {
                    self.rebuild_station_markers();
                }
            }
            Message::RemoveTagFilter(tag) => {
                if self.stations.remove_tag_filter(tag) {
                    self.rebuild_station_markers();
                }
            }
            Message::SetTagMatchMode(mode) => {
                if self.stations.set_tag_match_mode(*mode) {
                    self.rebuild_station_markers();
                }
            }
            Message::ToggleTagMatchMode => {
                self.stations.toggle_tag_match_mode();
                self.rebuild_station_markers();
            }
            Message::ToggleCountryOutlines => {
                self.toggle_country_outlines();
            }
            Message::SetShowCountryOutlines(show) => {
                self.set_show_country_outlines(*show);
            }
            Message::SetOutlineColor(color) => {
                self.set_country_outline_color(glam::Vec4::from_array(*color));
            }
            Message::CycleEarthBaseTexture | Message::SetEarthBaseTexture(_) => {
                // Handled in application update loop with CanvasRenderer access
            }
            Message::ThemeSelected(theme) => {
                self.active_theme = theme.clone();
            }
            Message::TogglePlayback
            | Message::StopPlayback
            | Message::PlayStation { .. }
            | Message::VolumeChanged(_) => {
                // Handled in application run loop / channels
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn dummy_station(uuid: &str, name: &str) -> radiobrowser::ApiStation {
    radiobrowser::ApiStation {
        changeuuid: "".to_string(),
        stationuuid: uuid.to_string(),
        serveruuid: None,
        name: name.to_string(),
        url: "http://stream.org".to_string(),
        url_resolved: "http://stream.org".to_string(),
        homepage: "".to_string(),
        favicon: "".to_string(),
        tags: "".to_string(),
        country: "".to_string(),
        countrycode: "".to_string(),
        iso_3166_2: None,
        state: "".to_string(),
        language: "".to_string(),
        languagecodes: None,
        votes: 0,
        lastchangetime_iso8601: None,
        codec: "MP3".to_string(),
        bitrate: 128,
        hls: 0,
        lastcheckok: 1,
        lastchecktime_iso8601: None,
        lastcheckoktime_iso8601: None,
        lastlocalchecktime_iso8601: None,
        clicktimestamp_iso8601: None,
        clickcount: 10,
        clicktrend: 0,
        ssl_error: Some(0),
        geo_lat: Some(10.0),
        geo_long: Some(20.0),
        has_extended_info: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_selected_message() {
        let mut state = AppState::default();
        assert_eq!(state.active_theme, iced::Theme::Moonfly);

        state.handle_message(&Message::ThemeSelected(iced::Theme::Dracula));
        assert_eq!(state.active_theme, iced::Theme::Dracula);

        state.handle_message(&Message::ThemeSelected(iced::Theme::Nord));
        assert_eq!(state.active_theme, iced::Theme::Nord);
    }
}
