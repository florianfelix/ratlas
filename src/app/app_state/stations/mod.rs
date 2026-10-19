pub mod filter;
pub mod limit;
pub mod tags;

pub use filter::StationFilter;
pub use limit::StationLimit;
pub use tags::TagMatchMode;

use crate::{
    app::app_state::{AppState, Camera, Ray, SpatialStation},
    data::{MarkerInstance, RenderObject},
};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stations {
    pub all: Vec<radiobrowser::ApiStation>,
    pub filtered: Vec<radiobrowser::ApiStation>,
    pub filter: StationFilter,
    pub limit: StationLimit,
    pub tags: Vec<radiobrowser::ApiTag>,
    pub hovered: Option<String>,  // stationuuid
    pub selected: Option<String>, // stationuuid
    pub playing: Option<String>,  // stationuuid
}

impl Stations {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks up a station by UUID across all stations, falling back to filtered stations.
    pub fn find_station(&self, uuid: &str) -> Option<&radiobrowser::ApiStation> {
        self.all
            .iter()
            .find(|s| s.stationuuid == uuid)
            .or_else(|| self.filtered.iter().find(|s| s.stationuuid == uuid))
    }

    /// Finds a station by URL or resolved stream URL.
    pub fn find_by_url(&self, url: &str) -> Option<&radiobrowser::ApiStation> {
        self.all
            .iter()
            .chain(self.filtered.iter())
            .find(|s| s.url == url || s.url_resolved == url)
    }

    /// Returns a reference to the currently playing station, if any.
    pub fn playing_station(&self) -> Option<&radiobrowser::ApiStation> {
        self.playing.as_ref().and_then(|id| self.find_station(id))
    }

    /// Sets or clears the currently playing station UUID.
    pub fn set_playing_station(&mut self, uuid: Option<String>) {
        self.playing = uuid;
    }

    /// Sets the hovered station UUID without triggering effector updates.
    /// Returns true if the hover state changed.
    pub fn set_hovered(&mut self, uuid: Option<String>) -> bool {
        if self.hovered == uuid {
            return false;
        }
        self.hovered = uuid;
        true
    }

    /// Sets the selected station UUID without triggering effector updates.
    /// Returns true if the selected state changed.
    pub fn set_selected(&mut self, uuid: Option<String>) -> bool {
        if self.selected == uuid {
            return false;
        }
        self.selected = uuid;
        true
    }

    /// Updates station limit. Returns true if the limit changed.
    pub fn set_limit(&mut self, limit: StationLimit) -> bool {
        if self.limit != limit {
            self.limit = limit;
            true
        } else {
            false
        }
    }

    /// Updates station filter. Returns (filter_changed, country_changed).
    pub fn set_filter(&mut self, filter: StationFilter) -> (bool, bool) {
        if self.filter != filter {
            let country_changed =
                self.filter.country_code != filter.country_code && filter.country_code.is_some();
            self.filter = filter;
            (true, country_changed)
        } else {
            (false, false)
        }
    }

    /// Adds a tag to the station filter if not already present. Returns true if added.
    pub fn add_tag_filter(&mut self, tag: String) -> bool {
        if !self
            .filter
            .tags
            .iter()
            .any(|t| t.eq_ignore_ascii_case(&tag))
        {
            self.filter.tags.push(tag);
            true
        } else {
            false
        }
    }

    /// Removes a tag from the station filter. Returns true if removed.
    pub fn remove_tag_filter(&mut self, tag: &str) -> bool {
        let prev_len = self.filter.tags.len();
        self.filter.tags.retain(|t| !t.eq_ignore_ascii_case(tag));
        self.filter.tags.len() != prev_len
    }

    /// Sets the tag match mode. Returns true if changed.
    pub fn set_tag_match_mode(&mut self, mode: TagMatchMode) -> bool {
        if self.filter.tag_match_mode != mode {
            self.filter.tag_match_mode = mode;
            true
        } else {
            false
        }
    }

    /// Toggles the tag match mode between AND and OR.
    pub fn toggle_tag_match_mode(&mut self) {
        self.filter.tag_match_mode = match self.filter.tag_match_mode {
            TagMatchMode::And => TagMatchMode::Or,
            TagMatchMode::Or => TagMatchMode::And,
        };
    }
}

impl AppState {
    pub fn set_station_limit(&mut self, limit: StationLimit) {
        if self.stations.set_limit(limit) {
            self.rebuild_station_markers();
        }
    }

    pub fn set_station_filter(&mut self, filter: StationFilter) {
        let (filter_changed, country_changed) = self.stations.set_filter(filter);
        if filter_changed {
            self.rebuild_station_markers();
            if country_changed {
                self.focus_on_country();
            }
        }
    }

    pub fn update_station_markers(&mut self, stations: Vec<radiobrowser::ApiStation>) {
        self.stations.all = stations;
        self.rebuild_station_markers();
    }

    pub fn rebuild_station_markers(&mut self) {
        let limit = self.stations.limit.to_count();
        self.stations.filtered = match limit {
            Some(max) => self
                .stations
                .all
                .iter()
                .filter(|s| self.stations.filter.matches(s))
                .take(max)
                .cloned()
                .collect(),
            None => self
                .stations
                .all
                .iter()
                .filter(|s| self.stations.filter.matches(s))
                .cloned()
                .collect(),
        };

        // 1. Clear previous marker instances and spatial grid
        self.marker_instances.clear();
        self.spatial_grid.clear();

        // 2. Create new marker instances from stations and position on sphere
        // Slightly elevated (1.002) to prevent polygon z-fighting with the globe surface
        let sphere_radius = 1.002_f32;
        let marker_scale = glam::Vec3::splat(0.06);

        self.marker_instances.reserve(self.stations.filtered.len());

        for station in &self.stations.filtered {
            let (Some(lat), Some(lon)) = (station.geo_lat, station.geo_long) else {
                continue;
            };

            let (position, normal) =
                RenderObject::sphere_coords(lat as f32, lon as f32, sphere_radius);

            // Orient marker so local +Y points away from sphere center (along normal)
            let rotation = glam::Quat::from_rotation_arc(glam::Vec3::Y, normal);

            let instance_index = self.marker_instances.len();
            let marker = MarkerInstance::new(
                &station.stationuuid,
                &station.name,
                position,
                rotation,
                marker_scale,
            );
            self.marker_instances.push(marker);

            self.spatial_grid.insert(SpatialStation {
                uuid: station.stationuuid.clone(),
                position,
                lat: lat as f32,
                lon: lon as f32,
                instance_index,
            });
        }

        // 3. Apply effectors (camera proximity, mouse lens, hover/selection)
        self.apply_effectors();
    }

    /// Picks a station marker under the cursor using the fast spatial grid and picking radius.
    /// Finds the closest marker to the cursor hit point on the globe within the picking effector radius,
    /// in O(1) time without scanning all objects in the world.
    pub fn pick_station(&self, ray: &Ray) -> Option<&radiobrowser::ApiStation> {
        // Distance to the front surface of the Earth
        let earth_dist = ray.intersect_sphere(glam::Vec3::ZERO, Camera::SPHERE_RADIUS)?;
        let globe_hit = ray.origin + ray.direction * earth_dist;

        let picker_radius = self
            .effectors
            .iter()
            .find(|e| e.is_picker())
            .and_then(|e| e.radius())
            .unwrap_or(0.02);

        let (station, _) = self.spatial_grid.query_closest(globe_hit, picker_radius)?;
        self.stations.find_station(&station.uuid)
    }

    /// Updates hover state and triggers effector updates.
    /// Returns true if the hover state changed.
    pub fn set_hovered_station(&mut self, uuid: Option<String>) -> bool {
        if self.stations.set_hovered(uuid) {
            self.apply_effectors();
            true
        } else {
            false
        }
    }

    /// Sets the currently selected station and triggers effector updates.
    /// Returns true if the selected state changed.
    pub fn set_selected_station(&mut self, uuid: Option<String>) -> bool {
        if self.stations.set_selected(uuid) {
            self.apply_effectors();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{app_state::dummy_station, ui::Message};

    #[test]
    fn test_rebuild_station_markers_with_filter() {
        let mut state = AppState::default();
        let stations = vec![
            dummy_station("1", "Station One"),
            dummy_station("2", "Station Two"),
        ];
        state.update_station_markers(stations);

        assert_eq!(state.stations.filtered.len(), 2);
        assert_eq!(state.marker_instances.len(), 2);

        // Filter by Germany (none of dummy match)
        let mut filter = StationFilter::default();
        filter.country_code = Some(isocountry::CountryCode::DEU);
        state.set_station_filter(filter);

        assert_eq!(state.stations.filtered.len(), 0);
        assert_eq!(state.marker_instances.len(), 0);
    }

    #[test]
    fn test_stations_struct_methods() {
        let mut stations = Stations::new();
        let s1 = dummy_station("1", "Station One");
        let s2 = dummy_station("2", "Station Two");
        stations.all = vec![s1.clone()];
        stations.filtered = vec![s2.clone()];

        assert_eq!(stations.find_station("1").unwrap().name, "Station One");
        assert_eq!(stations.find_station("2").unwrap().name, "Station Two");
        assert!(stations.find_station("3").is_none());

        assert_eq!(
            stations.find_by_url("http://stream.org").unwrap().name,
            "Station One"
        );

        assert!(stations.playing_station().is_none());
        stations.set_playing_station(Some("1".to_string()));
        assert_eq!(stations.playing_station().unwrap().name, "Station One");
        assert_eq!(stations.playing.as_deref(), Some("1"));

        assert!(stations.set_hovered(Some("1".to_string())));
        assert!(!stations.set_hovered(Some("1".to_string())));
        assert_eq!(stations.hovered.as_deref(), Some("1"));

        assert!(stations.set_selected(Some("2".to_string())));
        assert!(!stations.set_selected(Some("2".to_string())));
        assert_eq!(stations.selected.as_deref(), Some("2"));

        assert!(stations.set_limit(StationLimit::Limit5000));
        assert!(!stations.set_limit(StationLimit::Limit5000));
        assert_eq!(stations.limit, StationLimit::Limit5000);

        let mut f = StationFilter::default();
        f.country_code = Some(isocountry::CountryCode::FRA);
        let (filter_changed, country_changed) = stations.set_filter(f);
        assert!(filter_changed);
        assert!(country_changed);

        assert!(stations.add_tag_filter("jazz".to_string()));
        assert!(!stations.add_tag_filter("jazz".to_string()));
        assert_eq!(stations.filter.tags, vec!["jazz".to_string()]);

        assert!(stations.set_tag_match_mode(TagMatchMode::Or));
        assert_eq!(stations.filter.tag_match_mode, TagMatchMode::Or);
        stations.toggle_tag_match_mode();
        assert_eq!(stations.filter.tag_match_mode, TagMatchMode::And);

        assert!(stations.remove_tag_filter("jazz"));
        assert!(!stations.remove_tag_filter("jazz"));
        assert!(stations.filter.tags.is_empty());
    }

    #[test]
    fn test_tag_match_mode_toggle_message() {
        let mut state = AppState::default();
        assert_eq!(state.stations.filter.tag_match_mode, TagMatchMode::And);

        state.handle_message(&Message::ToggleTagMatchMode);
        assert_eq!(state.stations.filter.tag_match_mode, TagMatchMode::Or);

        state.handle_message(&Message::ToggleTagMatchMode);
        assert_eq!(state.stations.filter.tag_match_mode, TagMatchMode::And);
    }
}
