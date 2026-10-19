use crate::{
    app::app_state::{AppState, Camera, Ray, SpatialStation},
    data::{MarkerInstance, RenderObject},
};

impl AppState {
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
