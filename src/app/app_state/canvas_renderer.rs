use iced::wgpu::RenderPass;

use crate::{app::app_state::gpu::Gpu, data::RenderObject};

pub struct CanvasRenderer {
    gpu: Gpu,
    objects: Vec<RenderObject>,
}

impl CanvasRenderer {
    pub fn new(gpu: Gpu) -> Self {
        Self {
            gpu,
            objects: vec![],
        }
    }
    pub fn update(&mut self) {
        // update transforms,...
    }
    pub fn render<'a>(&self, render_pass: &mut RenderPass<'a>) {

        // render all objects
    }
}
