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
        // remove old station markers from world
        // create new station markers from stations
        // set transform for each to sit on sphere with radius 1.0 with y pointing away from center
        // add new markers to world
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
}
