use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    app::{app_state::canvas_renderer::CanvasRenderer, ui::Message},
    data::{Effector, MarkerInstance, RenderObject, material::Material},
};

pub mod camera;
pub mod canvas_renderer;
pub mod gpu;
pub mod spatial_grid;

pub use camera::{Camera, Ray};
use radiobrowser::ApiStation;
pub use spatial_grid::{GlobeSpatialGrid, SpatialStation};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EarthBaseTexture {
    #[default]
    Day,
    Night,
    Specular,
}

impl EarthBaseTexture {
    #[allow(dead_code)]
    pub const ALL: [EarthBaseTexture; 3] = [
        EarthBaseTexture::Day,
        EarthBaseTexture::Night,
        EarthBaseTexture::Specular,
    ];

    pub fn asset_path(&self) -> &'static str {
        match self {
            EarthBaseTexture::Day => "assets/sphere_diffuse.jpg",
            EarthBaseTexture::Night => "assets/sphere_diffuse_night.jpg",
            EarthBaseTexture::Specular => "assets/8k_earth_specular_map.jpg",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            EarthBaseTexture::Day => "Day",
            EarthBaseTexture::Night => "Night",
            EarthBaseTexture::Specular => "Specular",
        }
    }
}

impl std::fmt::Display for EarthBaseTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EarthConfig {
    pub show_country_outlines: bool,
    pub outline_color: glam::Vec4,
    pub base_texture: EarthBaseTexture,
}

impl Default for EarthConfig {
    fn default() -> Self {
        Self {
            show_country_outlines: false,
            outline_color: glam::Vec4::new(0.2, 0.8, 1.0, 0.85),
            base_texture: EarthBaseTexture::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagMatchMode {
    #[default]
    And,
    Or,
}

impl std::fmt::Display for TagMatchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagMatchMode::And => write!(f, "AND"),
            TagMatchMode::Or => write!(f, "OR"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationFilter {
    pub country_code: Option<isocountry::CountryCode>,
    pub tags: Vec<String>,
    pub tag_match_mode: TagMatchMode,
}

impl Default for StationFilter {
    fn default() -> Self {
        Self {
            country_code: None,
            tags: vec![],
            tag_match_mode: TagMatchMode::default(),
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
        if !self.tags.is_empty() {
            let mut tag_matches = self.tags.iter().map(|tag| {
                station
                    .tags
                    .split(',')
                    .any(|t| t.trim().eq_ignore_ascii_case(tag))
            });
            let matched = match self.tag_match_mode {
                TagMatchMode::And => tag_matches.all(|m| m),
                TagMatchMode::Or => tag_matches.any(|m| m),
            };
            if !matched {
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
            Message::TogglePlayback | Message::StopPlayback | Message::PlayStation { .. } | Message::VolumeChanged(_) => {
                // Handled in application run loop / channels
            }
        }
    }

    pub fn set_show_country_outlines(&mut self, show: bool) {
        self.earth_config.show_country_outlines = show;
        for obj in self.world.get_by_tag_mut("country_outlines") {
            obj.visible = show;
        }
    }

    pub fn toggle_country_outlines(&mut self) {
        let new_show = !self.earth_config.show_country_outlines;
        self.set_show_country_outlines(new_show);
    }

    pub fn set_country_outline_color(&mut self, color: glam::Vec4) {
        self.earth_config.outline_color = color;
        for obj in self.world.get_by_tag_mut("country_outlines") {
            obj.color_tint = color;
        }
    }

    pub fn set_earth_base_texture(
        &mut self,
        canvas_renderer: &CanvasRenderer,
        texture: EarthBaseTexture,
    ) {
        if self.earth_config.base_texture == texture {
            return;
        }
        self.earth_config.base_texture = texture;
        let resolved_path = crate::data::resolve_asset_path(texture.asset_path());
        let new_material = Arc::new(Material::from_image_path(
            &canvas_renderer.gpu.device,
            &canvas_renderer.gpu.queue,
            canvas_renderer.pipelines.default_pipeline.clone(),
            &canvas_renderer.pipelines.texture_layout,
            &resolved_path,
        ));
        for obj in self.world.get_by_tag_mut("earth") {
            obj.set_material(new_material.clone());
        }
    }

    pub fn cycle_earth_base_texture(&mut self, canvas_renderer: &CanvasRenderer) {
        let next_tex = match self.earth_config.base_texture {
            EarthBaseTexture::Day => EarthBaseTexture::Night,
            EarthBaseTexture::Night => EarthBaseTexture::Specular,
            EarthBaseTexture::Specular => EarthBaseTexture::Day,
        };
        self.set_earth_base_texture(canvas_renderer, next_tex);
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

    pub fn get_by_tag_mut(&mut self, tag: &str) -> impl Iterator<Item = &mut RenderObject> {
        self.objects.iter_mut().filter(move |obj| obj.tag == tag)
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
}
