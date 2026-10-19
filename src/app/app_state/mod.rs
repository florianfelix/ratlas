use std::collections::HashMap;

use crate::{
    app::{
        app_state::{
            earth::EarthConfig,
            filters::{StationFilter, StationLimit, TagMatchMode},
            world::World,
        },
        ui::Message,
    },
    data::{Effector, MarkerInstance, RenderObject},
};

pub mod camera;
pub mod canvas_renderer;
pub mod earth;
pub mod effectors;
pub mod filters;
pub mod gpu;
pub mod spatial_grid;
pub mod stations;
pub mod world;

pub use camera::{Camera, Ray};
pub use spatial_grid::{GlobeSpatialGrid, SpatialStation};

pub struct AppState {
    pub world: World,
    #[allow(dead_code)]
    pub objects: HashMap<String, RenderObject>,
    pub camera: Camera,
    pub earth_config: EarthConfig,
    pub all_radio_stations: Vec<radiobrowser::ApiStation>,
    pub radio_stations: Vec<radiobrowser::ApiStation>,
    pub station_filter: StationFilter,
    pub api_tags: Vec<radiobrowser::ApiTag>,
    pub station_limit: StationLimit,
    pub marker_instances: Vec<MarkerInstance>,
    pub spatial_grid: GlobeSpatialGrid,
    pub hovered_station: Option<String>,  // stationuuid
    pub selected_station: Option<String>, // stationuuid
    pub playing_station: Option<String>,  // stationuuid
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
            objects: HashMap::new(),
            camera: Camera::default(),
            earth_config: EarthConfig::default(),
            all_radio_stations: Vec::new(),
            radio_stations: Vec::new(),
            station_filter: StationFilter::default(),
            api_tags: Vec::new(),
            station_limit: StationLimit::default(),
            marker_instances: Vec::new(),
            spatial_grid: GlobeSpatialGrid::default(),
            hovered_station: None,
            selected_station: None,
            playing_station: None,
            mouse_globe_pos: None,
            mouse_pos: None,
            effectors,
            playback_status: crate::playback::PlaybackStatus::Stopped,
            active_theme: iced::Theme::Dark,
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
                if !self
                    .station_filter
                    .tags
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(tag))
                {
                    self.station_filter.tags.push(tag.clone());
                    self.rebuild_station_markers();
                }
            }
            Message::RemoveTagFilter(tag) => {
                let prev_len = self.station_filter.tags.len();
                self.station_filter
                    .tags
                    .retain(|t| !t.eq_ignore_ascii_case(tag));
                if self.station_filter.tags.len() != prev_len {
                    self.rebuild_station_markers();
                }
            }
            Message::SetTagMatchMode(mode) => {
                if self.station_filter.tag_match_mode != *mode {
                    self.station_filter.tag_match_mode = *mode;
                    self.rebuild_station_markers();
                }
            }
            Message::ToggleTagMatchMode => {
                self.station_filter.tag_match_mode = match self.station_filter.tag_match_mode {
                    TagMatchMode::And => TagMatchMode::Or,
                    TagMatchMode::Or => TagMatchMode::And,
                };
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
mod tests {
    use super::*;

    fn dummy_station(uuid: &str, name: &str) -> radiobrowser::ApiStation {
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

    #[test]
    fn test_earth_config_and_outlines_toggle() {
        let mut state = AppState::default();
        assert!(!state.earth_config.show_country_outlines);

        state.handle_message(&Message::ToggleCountryOutlines);
        assert!(state.earth_config.show_country_outlines);

        state.handle_message(&Message::SetShowCountryOutlines(false));
        assert!(!state.earth_config.show_country_outlines);

        state.handle_message(&Message::SetOutlineColor([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(
            state.earth_config.outline_color,
            glam::Vec4::new(1.0, 0.0, 0.0, 1.0)
        );
    }

    #[test]
    fn test_rebuild_station_markers_with_filter() {
        let mut state = AppState::default();
        let stations = vec![
            dummy_station("1", "Station One"),
            dummy_station("2", "Station Two"),
        ];
        state.update_station_markers(stations);

        assert_eq!(state.radio_stations.len(), 2);
        assert_eq!(state.marker_instances.len(), 2);

        // Filter by Germany (none of dummy match)
        let mut filter = StationFilter::default();
        filter.country_code = Some(isocountry::CountryCode::DEU);
        state.set_station_filter(filter);

        assert_eq!(state.radio_stations.len(), 0);
        assert_eq!(state.marker_instances.len(), 0);
    }

    #[test]
    fn test_spatial_grid_proximity_query() {
        let mut state = AppState::default();
        let stations = vec![
            dummy_station("1", "Station Near"),
            dummy_station("2", "Station Far"),
        ];
        state.update_station_markers(stations);

        // Grid contains stations inserted at sphere coords
        assert_eq!(state.spatial_grid.cell_count(), 1);
        let pos = state.marker_instances[0].position;
        let nearby = state.spatial_grid.query_radius(pos, 0.05);
        assert!(!nearby.is_empty());
        assert_eq!(nearby[0].uuid, "1");
    }

    #[test]
    fn test_picking_effector_and_hover() {
        let mut state = AppState::default();
        let stations = vec![dummy_station("1", "Pick Me")];
        state.update_station_markers(stations);

        let target_pos = state.marker_instances[0].position;
        let ray = Ray {
            origin: target_pos.normalize() * 3.0,
            direction: -target_pos.normalize(),
        };

        let picked = state.pick_station(&ray);
        assert!(picked.is_some());
        assert_eq!(picked.unwrap().stationuuid, "1");

        // Set hovered station
        let changed = state.set_hovered_station(Some("1".to_string()));
        assert!(changed);
        assert_eq!(state.hovered_station, Some("1".to_string()));

        // Effector scaled the hovered marker up by 1.25
        assert!((state.marker_instances[0].scale.x - 0.06 * 1.25).abs() < 1e-4);
    }

    #[test]
    fn test_tag_search_and_matching() {
        let mut station = dummy_station("1", "Rock Station");
        station.tags = "classic rock, 80s, rock".to_string();

        let mut filter = StationFilter::default();
        filter.tags = vec!["rock".to_string()];
        assert!(filter.matches(&station));

        filter.tags = vec!["jazz".to_string()];
        assert!(!filter.matches(&station));

        // Multi-tag matching AND vs OR
        filter.tags = vec!["rock".to_string(), "jazz".to_string()];
        filter.tag_match_mode = TagMatchMode::And;
        assert!(!filter.matches(&station));

        filter.tag_match_mode = TagMatchMode::Or;
        assert!(filter.matches(&station));
    }

    #[test]
    fn test_tag_match_mode_toggle_message() {
        let mut state = AppState::default();
        assert_eq!(state.station_filter.tag_match_mode, TagMatchMode::And);

        state.handle_message(&Message::ToggleTagMatchMode);
        assert_eq!(state.station_filter.tag_match_mode, TagMatchMode::Or);

        state.handle_message(&Message::ToggleTagMatchMode);
        assert_eq!(state.station_filter.tag_match_mode, TagMatchMode::And);
    }

    #[test]
    fn test_theme_selected_message() {
        let mut state = AppState::default();
        assert_eq!(state.active_theme, iced::Theme::Dark);

        state.handle_message(&Message::ThemeSelected(iced::Theme::Dracula));
        assert_eq!(state.active_theme, iced::Theme::Dracula);

        state.handle_message(&Message::ThemeSelected(iced::Theme::Nord));
        assert_eq!(state.active_theme, iced::Theme::Nord);
    }
}
