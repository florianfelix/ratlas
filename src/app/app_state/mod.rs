use std::collections::HashMap;

use crate::{app::ui::Message, data::RenderObject};

pub mod camera;
pub mod canvas_renderer;
pub mod gpu;

pub use camera::Camera;

pub struct AppState {
    pub world: World,
    pub objects: HashMap<String, RenderObject>,
    pub camera: Camera,
    pub radio_stations: Vec<radiobrowser::ApiStation>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            world: World::new(),
            objects: HashMap::new(),
            camera: Camera::default(),
            radio_stations: Vec::new(),
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

    #[allow(dead_code)]
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
}
