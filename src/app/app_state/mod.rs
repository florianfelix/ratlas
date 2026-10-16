use std::collections::HashMap;

use crate::{
    app::ui::Message,
    data::{Effector, MarkerInstance, RenderObject},
};

pub mod camera;
pub mod canvas_renderer;
pub mod gpu;
pub mod spatial_grid;

pub use camera::{Camera, Ray};
use radiobrowser::ApiStation;
pub use spatial_grid::{GlobeSpatialGrid, SpatialStation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationFilter {
    pub country_code: Option<isocountry::CountryCode>,
    pub tags: Vec<String>,
}

impl Default for StationFilter {
    fn default() -> Self {
        Self {
            country_code: None,
            tags: vec![],
        }
    }
}

impl StationFilter {
    pub fn matches(&self, station: &ApiStation) -> bool {
        if let Some(ref cc) = self.country_code {
            if !station.countrycode.eq_ignore_ascii_case(cc.alpha2()) {
                return false;
            }
        }
        for tag in &self.tags {
            if !station.tags.contains(tag) {
                return false;
            }
        }

        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StationLimit {
    Limit500,
    #[default]
    Limit1000,
    Limit2500,
    Limit5000,
    All,
}

impl StationLimit {
    pub const ALL: [StationLimit; 5] = [
        StationLimit::Limit500,
        StationLimit::Limit1000,
        StationLimit::Limit2500,
        StationLimit::Limit5000,
        StationLimit::All,
    ];

    pub fn to_count(self) -> Option<usize> {
        match self {
            StationLimit::Limit500 => Some(500),
            StationLimit::Limit1000 => Some(1000),
            StationLimit::Limit2500 => Some(2500),
            StationLimit::Limit5000 => Some(5000),
            StationLimit::All => None,
        }
    }
}

impl std::fmt::Display for StationLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StationLimit::Limit500 => write!(f, "500"),
            StationLimit::Limit1000 => write!(f, "1,000"),
            StationLimit::Limit2500 => write!(f, "2,500"),
            StationLimit::Limit5000 => write!(f, "5,000"),
            StationLimit::All => write!(f, "All"),
        }
    }
}

pub struct AppState {
    pub world: World,
    #[allow(dead_code)]
    pub objects: HashMap<String, RenderObject>,
    pub camera: Camera,
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
    pub effectors: Vec<Effector>,
    pub playback_status: crate::playback::PlaybackStatus,
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
            effectors,
            playback_status: crate::playback::PlaybackStatus::Stopped,
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
            Message::SetStationLimit(limit) => {
                self.set_station_limit(*limit);
            }
            Message::StationFilterChanged(filter) => {
                self.set_station_filter(filter.clone());
            }
            Message::TogglePlayback | Message::StopPlayback | Message::PlayStation { .. } => {
                // Handled in application run loop / channels
            }
        }
    }

    pub fn set_station_limit(&mut self, limit: StationLimit) {
        if self.station_limit != limit {
            self.station_limit = limit;
            self.rebuild_station_markers();
        }
    }

    pub fn set_station_filter(&mut self, filter: StationFilter) {
        if self.station_filter != filter {
            let country_changed = self.station_filter.country_code != filter.country_code;
            let has_new_country = filter.country_code.is_some();
            self.station_filter = filter;
            self.rebuild_station_markers();

            if country_changed && has_new_country {
                self.focus_on_country();
            }
        }
    }

    /// Smoothly animates camera rotation around the globe until it is centered
    /// over the active cluster of station markers.
    pub fn focus_on_country(&mut self) {
        let mut sum = glam::Vec3::ZERO;
        let mut count = 0;
        for marker in &self.marker_instances {
            sum += marker.position;
            count += 1;
        }

        if count > 0 {
            let center_normal = (sum / count as f32).normalize();
            // Smoothly fly camera to face the centroid of the country's station cluster
            // Gently bring the camera into a comfortable viewing altitude (~2.1 units)
            let target_dist = self.camera.radius.clamp(1.8, 2.3);
            self.camera
                .fly_to_direction(center_normal, Some(target_dist));
        }
    }

    pub fn update_station_markers(&mut self, stations: Vec<radiobrowser::ApiStation>) {
        self.all_radio_stations = stations;
        self.rebuild_station_markers();
    }

    pub fn rebuild_station_markers(&mut self) {
        let limit = self.station_limit.to_count();
        self.radio_stations = match limit {
            Some(max) => self
                .all_radio_stations
                .iter()
                .filter(|s| self.station_filter.matches(s))
                .take(max)
                .cloned()
                .collect(),
            None => self
                .all_radio_stations
                .iter()
                .filter(|s| self.station_filter.matches(s))
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

        self.marker_instances.reserve(self.radio_stations.len());

        for station in &self.radio_stations {
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

    /// Evaluates all active effectors and updates marker scales and color tints.
    /// Also performs horizon/backface culling on spherical station markers to skip
    /// occluded markers on the back side of the Earth.
    pub fn apply_effectors(&mut self) {
        // Sync camera effector position with camera
        if let Some(cam_eff) = self
            .effectors
            .iter_mut()
            .find(|e| e.name == "CameraProximityEffector")
        {
            cam_eff.position = self.camera.position;
        }

        // Sync mouse lens and picking effector positions with mouse globe hit point
        for effector in &mut self.effectors {
            if effector.name == "MouseGlobeLens" || effector.is_picker() {
                if let Some(globe_pos) = self.mouse_globe_pos {
                    effector.enabled = true;
                    effector.position = globe_pos;
                } else {
                    effector.enabled = false;
                }
            }
        }

        let cam_pos = self.camera.position;
        // Small margin (-0.05) ensures markers right at the horizon silhouette
        // don't pop out while their 3D geometry is still partially visible.
        const HORIZON_CULL_MARGIN: f32 = -0.05;

        let cam_eff_opt = self
            .effectors
            .iter()
            .find(|e| e.name == "CameraProximityEffector");

        for marker in &mut self.marker_instances {
            // Horizon / Backface Culling:
            // Test if the marker's outward normal is facing the camera.
            // For a sphere centered at origin, marker.position points along outward normal.
            // to_cam = cam_pos - marker.position. If marker.position.dot(to_cam) <= margin,
            // the marker is behind the horizon of the Earth sphere.
            let to_cam = cam_pos - marker.position;
            let is_visible = marker.position.dot(to_cam) > HORIZON_CULL_MARGIN;
            marker.visible = is_visible;

            if !is_visible {
                continue;
            }

            let mut current_scale = marker.base_scale;
            if let Some(cam_eff) = cam_eff_opt {
                let (scale_mult, _) = cam_eff.evaluate(marker.position);
                current_scale *= scale_mult;
            }

            marker.scale = current_scale;
            marker.color_tint = glam::Vec4::ONE;
        }

        // Mouse Globe Lens: only evaluate the few stations within spatial grid radius
        if let Some(globe_pos) = self.mouse_globe_pos {
            if let Some(lens_eff) = self.effectors.iter().find(|e| e.name == "MouseGlobeLens") {
                if lens_eff.enabled {
                    if let Some(radius) = lens_eff.radius() {
                        let nearby = self.spatial_grid.query_radius(globe_pos, radius);
                        for station in nearby {
                            if station.instance_index < self.marker_instances.len() {
                                let marker = &mut self.marker_instances[station.instance_index];
                                if marker.visible {
                                    let (scale_mult, tint_opt) = lens_eff.evaluate(marker.position);
                                    marker.scale *= scale_mult;
                                    if let Some(t) = tint_opt {
                                        marker.color_tint += t;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Apply hover and selection highlights
        if let Some(ref hovered_id) = self.hovered_station {
            for marker in &mut self.marker_instances {
                if &marker.id == hovered_id {
                    marker.scale *= 1.25;
                    marker.color_tint *= glam::Vec4::new(0.7, 1.3, 1.5, 1.0);
                    break;
                }
            }
        }
        if let Some(ref selected_id) = self.selected_station {
            for marker in &mut self.marker_instances {
                if &marker.id == selected_id {
                    marker.scale *= 1.15;
                    marker.color_tint *= glam::Vec4::new(1.3, 1.1, 0.4, 1.0);
                    break;
                }
            }
        }
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
        self.find_station(&station.uuid)
    }

    /// Looks up a station by UUID
    pub fn find_station(&self, uuid: &str) -> Option<&radiobrowser::ApiStation> {
        self.all_radio_stations
            .iter()
            .find(|s| s.stationuuid == uuid)
            .or_else(|| self.radio_stations.iter().find(|s| s.stationuuid == uuid))
    }

    /// Updates hover state and triggers effector updates.
    /// Returns true if the hover state changed.
    pub fn set_hovered_station(&mut self, uuid: Option<String>) -> bool {
        if self.hovered_station == uuid {
            return false;
        }
        self.hovered_station = uuid;
        self.apply_effectors();
        true
    }

    /// Sets the currently selected station and triggers effector updates.
    /// Returns true if the selected state changed.
    pub fn set_selected_station(&mut self, uuid: Option<String>) -> bool {
        if self.selected_station == uuid {
            return false;
        }
        self.selected_station = uuid;
        self.apply_effectors();
        true
    }

    /// Returns a reference to the currently playing station, if any.
    pub fn playing_station(&self) -> Option<&radiobrowser::ApiStation> {
        self.playing_station
            .as_ref()
            .and_then(|id| self.find_station(id))
    }

    /// Sets or clears the currently playing station.
    pub fn set_playing_station(&mut self, uuid: Option<String>) {
        self.playing_station = uuid;
    }
}

pub struct World {
    pub objects: Vec<RenderObject>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            objects: Vec::new(),
        }
    }
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_object(&mut self, object: RenderObject) {
        self.objects.push(object);
    }

    #[allow(dead_code)]
    pub fn add_objects(&mut self, objects: impl IntoIterator<Item = RenderObject>) {
        self.objects.extend(objects);
    }

    #[allow(dead_code)]
    pub fn remove_by_tag(&mut self, tag: &str) {
        self.objects.retain(|obj| obj.tag != tag);
    }

    #[allow(dead_code)]
    pub fn remove_by_id(&mut self, id: &str) {
        self.objects.retain(|obj| obj.id.as_deref() != Some(id));
    }

    #[allow(dead_code)]
    pub fn get_by_id(&self, id: &str) -> Option<&RenderObject> {
        self.objects
            .iter()
            .find(|obj| obj.id.as_deref() == Some(id))
    }

    #[allow(dead_code)]
    pub fn get_by_tag(&self, tag: &str) -> impl Iterator<Item = &RenderObject> {
        self.objects.iter().filter(move |obj| obj.tag == tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::PickingEffector;

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
    fn test_sphere_coords_mapping() {
        let (pos_greenwich, normal_greenwich) = RenderObject::sphere_coords(0.0, 0.0, 1.0);
        assert!((pos_greenwich.x - 1.0).abs() < 1e-4);
        assert!(pos_greenwich.y.abs() < 1e-4);
        assert!(pos_greenwich.z.abs() < 1e-4);
        assert_eq!(pos_greenwich, normal_greenwich);

        let (pos_north, normal_north) = RenderObject::sphere_coords(90.0, 0.0, 1.0);
        assert!(pos_north.x.abs() < 1e-4);
        assert!((pos_north.y - 1.0).abs() < 1e-4);
        assert!(pos_north.z.abs() < 1e-4);
        assert_eq!(pos_north, normal_north);

        let (pos_lon90, normal_lon90) = RenderObject::sphere_coords(0.0, 90.0, 1.0);
        assert!(pos_lon90.x.abs() < 1e-4);
        assert!(pos_lon90.y.abs() < 1e-4);
        assert!((pos_lon90.z - (-1.0)).abs() < 1e-4);
        assert_eq!(pos_lon90, normal_lon90);
    }

    #[test]
    fn test_earth_occlusion_math() {
        let ray = Ray::new(
            glam::Vec3::new(0.0, 0.0, 3.0),
            glam::Vec3::new(0.0, 0.0, -1.0),
        );
        let earth_t = ray
            .intersect_sphere(glam::Vec3::ZERO, Camera::SPHERE_RADIUS)
            .unwrap();
        assert!((earth_t - 2.0).abs() < 1e-4);

        // Marker in front of Earth (t = 1.95 < 2.0): NOT occluded
        let front_marker_t = ray
            .intersect_sphere(glam::Vec3::new(0.0, 0.0, 1.05), 0.05)
            .unwrap();
        assert!(front_marker_t < earth_t);

        // Marker on the back side of Earth (t = 3.95 > 2.0): occluded by Earth
        let back_marker_t = ray
            .intersect_sphere(glam::Vec3::new(0.0, 0.0, -1.05), 0.05)
            .unwrap();
        assert!(back_marker_t > earth_t);
    }

    #[test]
    fn test_pick_station_closest_inside_mouse_lens_math() {
        // Ray cast from (0, 0, 3) pointing along -Z towards the Earth at (0, 0, 0)
        let ray = Ray::new(
            glam::Vec3::new(0.0, 0.0, 3.0),
            glam::Vec3::new(0.0, 0.0, -1.0),
        );
        let earth_t = ray
            .intersect_sphere(glam::Vec3::ZERO, Camera::SPHERE_RADIUS)
            .unwrap();
        let globe_hit = ray.origin + ray.direction * earth_t;

        // Picking effector with radius 0.02
        let mut picker = PickingEffector::new(0.02);
        picker.position = globe_hit;

        let marker_1_pos = glam::Vec3::new(0.005, 0.0, 1.002); // Distance ~ 0.0054 <= 0.02 (inside lens, very close)
        let marker_2_pos = glam::Vec3::new(0.015, 0.0, 1.002); // Distance ~ 0.0151 <= 0.02 (inside lens, further)
        let marker_3_pos = glam::Vec3::new(0.050, 0.0, 1.002); // Distance ~ 0.0500 > 0.02 (outside lens)

        let candidates = [
            ("uuid-1", marker_1_pos),
            ("uuid-2", marker_2_pos),
            ("uuid-3", marker_3_pos),
        ];

        let picked = picker.pick_closest(candidates.iter().map(|(id, pos)| (*id, *pos)));
        assert_eq!(picked.map(|(id, _)| id), Some("uuid-1"));
    }

    #[test]
    fn test_station_limit_counts() {
        assert_eq!(StationLimit::Limit500.to_count(), Some(500));
        assert_eq!(StationLimit::Limit1000.to_count(), Some(1000));
        assert_eq!(StationLimit::Limit2500.to_count(), Some(2500));
        assert_eq!(StationLimit::Limit5000.to_count(), Some(5000));
        assert_eq!(StationLimit::All.to_count(), None);
    }

    #[test]
    fn test_station_limit_filtering() {
        let mut state = AppState::default();
        let stations: Vec<_> = (0..3000)
            .map(|i| dummy_station(&format!("uuid-{}", i), &format!("Station {}", i)))
            .collect();

        // Default limit is 1,000
        state.update_station_markers(stations);
        assert_eq!(state.all_radio_stations.len(), 3000);
        assert_eq!(state.radio_stations.len(), 1000);
        assert_eq!(state.marker_instances.len(), 1000);

        // Change limit to 500
        state.set_station_limit(StationLimit::Limit500);
        assert_eq!(state.radio_stations.len(), 500);
        assert_eq!(state.marker_instances.len(), 500);

        // Change limit to 2500
        state.set_station_limit(StationLimit::Limit2500);
        assert_eq!(state.radio_stations.len(), 2500);
        assert_eq!(state.marker_instances.len(), 2500);

        // Change limit to All
        state.set_station_limit(StationLimit::All);
        assert_eq!(state.radio_stations.len(), 3000);
        assert_eq!(state.marker_instances.len(), 3000);

        // Find station should still find stations outside the current active slice
        state.set_station_limit(StationLimit::Limit500);
        assert!(state.find_station("uuid-2999").is_some());
    }

    #[test]
    fn test_country_filter_matching() {
        let mut state = AppState::default();
        let mut s_de = dummy_station("s-de", "German Station");
        s_de.countrycode = "DE".to_string();

        let mut s_us = dummy_station("s-us", "US Station");
        s_us.countrycode = "US".to_string();

        state.update_station_markers(vec![s_de, s_us]);
        assert_eq!(state.radio_stations.len(), 2);

        // Filter by Germany (DEU)
        state.set_station_filter(StationFilter {
            country_code: Some(isocountry::CountryCode::DEU),
            tags: vec![],
        });
        assert_eq!(state.radio_stations.len(), 1);
        assert_eq!(state.radio_stations[0].stationuuid, "s-de");

        // Clear filter
        state.set_station_filter(StationFilter::default());
        assert_eq!(state.radio_stations.len(), 2);
    }

    #[test]
    fn test_horizon_culling_math() {
        let cam_pos = glam::Vec3::new(0.0, 0.0, 3.0);
        const HORIZON_CULL_MARGIN: f32 = -0.05;

        // Front facing marker (directly facing camera along +Z)
        let front_pos = glam::Vec3::new(0.0, 0.0, 1.002);
        let to_cam_front = cam_pos - front_pos;
        let is_front_visible = front_pos.dot(to_cam_front) > HORIZON_CULL_MARGIN;
        assert!(is_front_visible, "Front-facing marker should be visible");

        // Backside marker (facing away from camera along -Z)
        let back_pos = glam::Vec3::new(0.0, 0.0, -1.002);
        let to_cam_back = cam_pos - back_pos;
        let is_back_visible = back_pos.dot(to_cam_back) > HORIZON_CULL_MARGIN;
        assert!(!is_back_visible, "Backside marker should be culled");

        // Equator marker (perpendicular to camera ray, at 90 deg)
        let side_pos = glam::Vec3::new(1.002, 0.0, 0.0);
        let to_cam_side = cam_pos - side_pos;
        let is_side_visible = side_pos.dot(to_cam_side) > HORIZON_CULL_MARGIN;
        assert!(
            !is_side_visible,
            "Side marker at 90 deg from 3.0 units away is behind horizon"
        );
    }

    #[test]
    fn test_marker_instances_effectors_and_picking() {
        let mut state = AppState::default();
        let mut s1 = dummy_station("s1", "Station One");
        s1.geo_lat = Some(0.0);
        s1.geo_long = Some(0.0); // Facing along +X (1.002, 0, 0)
        let mut s2 = dummy_station("s2", "Station Two");
        s2.geo_lat = Some(0.0);
        s2.geo_long = Some(180.0); // Backside along -X (-1.002, 0, 0)

        state.update_station_markers(vec![s1, s2]);
        assert_eq!(state.marker_instances.len(), 2);

        // Position camera at (3.0, 0, 0) facing origin
        state.camera.position = glam::Vec3::new(3.0, 0.0, 0.0);
        state.apply_effectors();

        // Front marker (s1) should be visible, s2 should be culled
        assert!(state.marker_instances[0].visible);
        assert!(!state.marker_instances[1].visible);

        // Hover over s1
        state.set_hovered_station(Some("s1".to_string()));
        assert!(state.marker_instances[0].color_tint.y > 1.0); // Highlighted tint
    }
}
