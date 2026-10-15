use std::collections::HashMap;

use crate::{app::ui::Message, data::RenderObject};

pub mod camera;
pub mod canvas_renderer;
pub mod gpu;

pub use camera::{Camera, Ray};

pub struct AppState {
    pub world: World,
    pub objects: HashMap<String, RenderObject>,
    pub camera: Camera,
    pub radio_stations: Vec<radiobrowser::ApiStation>,
    pub hovered_station: Option<String>,  // stationuuid
    pub selected_station: Option<String>, // stationuuid
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            world: World::new(),
            objects: HashMap::new(),
            camera: Camera::default(),
            radio_stations: Vec::new(),
            hovered_station: None,
            selected_station: None,
        }
    }
}

impl AppState {
    /// Handles application-level actions and mutations triggered by UI messages
    pub fn handle_message(&mut self, message: &Message) {
        match message {
            Message::ResetCamera => {
                self.camera.reset();
            }
            Message::ButtonClicked => {
                // Application-level logic when button is clicked
            }
        }
    }

    pub fn update_station_markers(&mut self, stations: Vec<radiobrowser::ApiStation>) {
        self.radio_stations = stations;

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

        // 5. Re-apply any active hover or selection highlights
        self.update_marker_scales();
    }

    /// Casts a ray into the scene to pick a station marker, taking Earth occlusion into account.
    pub fn pick_station(&self, ray: &Ray) -> Option<&radiobrowser::ApiStation> {
        // Distance to the front surface of the Earth
        let earth_dist = ray.intersect_sphere(glam::Vec3::ZERO, Camera::SPHERE_RADIUS);

        let mut closest_hit: Option<(f32, &String)> = None;

        for obj in self.world.get_by_tag("station_marker") {
            // The marker pin head is located along the object's local +Y axis
            let pin_head_center = obj.position + obj.rotation * (glam::Vec3::Y * 0.035);
            let hit_radius = 0.045_f32; // Generous hit radius for smooth hover/selection

            if let Some(t) = ray.intersect_sphere(pin_head_center, hit_radius) {
                // If Earth is closer than the marker, the marker is occluded on the far side
                if let Some(t_earth) = earth_dist {
                    if t > t_earth {
                        continue;
                    }
                }

                if closest_hit.as_ref().map_or(true, |(min_t, _)| t < *min_t) {
                    if let Some(id) = &obj.id {
                        closest_hit = Some((t, id));
                    }
                }
            }
        }

        closest_hit.and_then(|(_, id)| self.find_station(id))
    }

    /// Looks up a station by UUID
    pub fn find_station(&self, uuid: &str) -> Option<&radiobrowser::ApiStation> {
        self.radio_stations.iter().find(|s| s.stationuuid == uuid)
    }

    /// Updates hover state and dynamically scales the hovered/selected markers for visual pop.
    /// Returns true if the hover state changed.
    pub fn set_hovered_station(&mut self, uuid: Option<String>) -> bool {
        if self.hovered_station == uuid {
            return false;
        }
        self.hovered_station = uuid;
        self.update_marker_scales();
        true
    }

    /// Sets the currently selected station and updates marker visual scales.
    pub fn set_selected_station(&mut self, uuid: Option<String>) -> bool {
        if self.selected_station == uuid {
            return false;
        }
        self.selected_station = uuid;
        self.update_marker_scales();
        true
    }

    fn update_marker_scales(&mut self) {
        for obj in &mut self.world.objects {
            if obj.tag == "station_marker" {
                if obj.id.as_ref() == self.hovered_station.as_ref() {
                    obj.scale = glam::Vec3::splat(0.085); // Enlarged on hover
                } else if obj.id.as_ref() == self.selected_station.as_ref() {
                    obj.scale = glam::Vec3::splat(0.075); // Highlighted on select
                } else {
                    obj.scale = glam::Vec3::splat(0.060); // Default scale
                }
            }
        }
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
}
