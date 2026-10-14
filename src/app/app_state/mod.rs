use crate::{app::ui::Message, data::RenderObject};

pub mod camera;
pub mod canvas_renderer;
pub mod gpu;

pub use camera::Camera;

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

pub struct AppState {
    pub world: World,
    pub camera: Camera,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            world: World::new(),
            camera: Camera::default(),
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
}
