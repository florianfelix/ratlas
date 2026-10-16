use std::collections::HashMap;

use crate::{
    app::ui::Message,
    data::{Effector, RenderObject, effector::EffectorKind},
};

pub mod camera;
pub mod canvas_renderer;
pub mod gpu;

pub use camera::{Camera, Ray};

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
    pub objects: HashMap<String, RenderObject>,
    pub camera: Camera,
    pub all_radio_stations: Vec<radiobrowser::ApiStation>,
    pub radio_stations: Vec<radiobrowser::ApiStation>,
    pub api_tags: Vec<radiobrowser::ApiTag>,
    pub station_limit: StationLimit,
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
            api_tags: Vec::new(),
            station_limit: StationLimit::default(),
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

    pub fn update_station_markers(&mut self, stations: Vec<radiobrowser::ApiStation>) {
        self.all_radio_stations = stations;
        self.rebuild_station_markers();
    }

    pub fn rebuild_station_markers(&mut self) {
        let limit = self.station_limit.to_count();
        self.radio_stations = match limit {
            Some(max) => self.all_radio_stations.iter().take(max).cloned().collect(),
            None => self.all_radio_stations.clone(),
        };

        // 1. Remove old station markers from world
        self.world.remove_by_tag("station_marker");

        // 2. Look up preloaded marker template
        let Some(marker_template) = self
            .objects
            .get("marker")
            .or_else(|| self.objects.get("marker.obj"))
        else {
            tracing::warn!("Marker template not found in AppState.objects");
            return;
        };

        // 3. Create new station markers from stations and position on sphere
        // Slightly elevated (1.002) to prevent polygon z-fighting with the globe surface
        let sphere_radius = 1.002_f32;
        let marker_scale = glam::Vec3::splat(0.06);

        let mut new_markers = Vec::with_capacity(self.radio_stations.len());
        for station in &self.radio_stations {
            let (Some(lat), Some(lon)) = (station.geo_lat, station.geo_long) else {
                continue;
            };

            let (position, normal) =
                RenderObject::sphere_coords(lat as f32, lon as f32, sphere_radius);

            // Orient marker so local +Y points away from sphere center (along normal)
            let rotation = glam::Quat::from_rotation_arc(glam::Vec3::Y, normal);

            let marker = marker_template
                .create_instance()
                .with_name(&station.name)
                .with_tag("station_marker")
                .with_id(&station.stationuuid)
                .with_transform(position, rotation, marker_scale);

            new_markers.push(marker);
        }

        // 4. Add new markers to world
        self.world.add_objects(new_markers);

        // 5. Apply effectors (camera proximity, mouse lens, hover/selection)
        self.apply_effectors();
    }

    /// Evaluates all active effectors and updates marker scales and color tints
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

        for obj in &mut self.world.objects {
            if obj.tag == "station_marker" {
                let mut current_scale = obj.base_scale;
                let mut current_tint = glam::Vec4::ONE;

                // 1. Evaluate effectors
                for effector in &self.effectors {
                    let (scale_mult, tint_opt) = effector.evaluate(obj.position);
                    current_scale *= scale_mult;
                    if let Some(t) = tint_opt {
                        current_tint += t;
                    }
                }

                // 2. Apply hover and selection highlights on top
                if obj.id.as_ref() == self.hovered_station.as_ref() {
                    current_scale *= 1.25;
                    current_tint *= glam::Vec4::new(0.7, 1.3, 1.5, 1.0);
                } else if obj.id.as_ref() == self.selected_station.as_ref() {
                    current_scale *= 1.15;
                    current_tint *= glam::Vec4::new(1.3, 1.1, 0.4, 1.0);
                }

                obj.scale = current_scale;
                obj.color_tint = current_tint;
            }
        }
    }

    /// Picks a station marker under the cursor using the picking effector.
    /// Finds the closest marker to the cursor hit point on the globe within the picking effector radius,
    /// ensuring deterministic, stable hover/selection that visually matches the lens magnification.
    pub fn pick_station(&self, ray: &Ray) -> Option<&radiobrowser::ApiStation> {
        // Distance to the front surface of the Earth
        let earth_dist = ray.intersect_sphere(glam::Vec3::ZERO, Camera::SPHERE_RADIUS)?;
        let globe_hit = ray.origin + ray.direction * earth_dist;

        // Query the dedicated picking effector, or fall back to any active spherical lens
        let picker = self.effectors.iter().find(|e| e.is_picker()).or_else(|| {
            self.effectors
                .iter()
                .find(|e| matches!(e.kind, EffectorKind::SphericalLens { .. }))
        })?;

        let mut active_picker = picker.clone();
        active_picker.position = globe_hit;
        active_picker.enabled = true;

        let candidates = self
            .world
            .get_by_tag("station_marker")
            .filter_map(|obj| obj.id.as_deref().map(|id| (id, obj.position)));

        let (picked_id, _) = active_picker.pick_closest(candidates)?;
        self.find_station(picked_id)
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

    pub fn add_objects(&mut self, objects: impl IntoIterator<Item = RenderObject>) {
        self.objects.extend(objects);
    }

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

        // Change limit to 500
        state.set_station_limit(StationLimit::Limit500);
        assert_eq!(state.radio_stations.len(), 500);

        // Change limit to 2500
        state.set_station_limit(StationLimit::Limit2500);
        assert_eq!(state.radio_stations.len(), 2500);

        // Change limit to All
        state.set_station_limit(StationLimit::All);
        assert_eq!(state.radio_stations.len(), 3000);

        // Find station should still find stations outside the current active slice
        state.set_station_limit(StationLimit::Limit500);
        assert!(state.find_station("uuid-2999").is_some());
    }
}
