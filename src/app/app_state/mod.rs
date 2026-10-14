use crate::data::RenderObject;

pub mod canvas_renderer;
pub mod gpu;

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
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            world: World::new(),
        }
    }
}
