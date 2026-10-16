use iced::wgpu;

use crate::{
    app::app_state::{
        camera::Camera, canvas_renderer::pipeline_manager::PipelineManager, gpu::Gpu,
    },
    data::RenderObject,
};

mod pipeline_manager;

pub struct CanvasRenderer {
    pub gpu: Gpu,
    pub pipelines: PipelineManager,
}

impl CanvasRenderer {
    pub fn new(gpu: Gpu) -> Self {
        let pipelines = PipelineManager::new(&gpu.device, gpu.surface_config.format);

        Self { gpu, pipelines }
    }

    /// Updates transform uniform buffers for all visible objects in the world using camera view/projection
    pub fn update(&self, camera: &Camera, objects: &[RenderObject], aspect_ratio: f32) {
        let view = camera.view_matrix();
        let proj = camera.projection_matrix(aspect_ratio);
        let vp = proj * view;

        for object in objects {
            if !object.visible {
                continue;
            }
            let mvp = vp * object.local_matrix();
            object.update_transform(&self.gpu.queue, mvp);
        }
    }

    /// Renders all visible objects passed into the render pass
    pub fn render<'a>(
        &'a self,
        objects: &'a [RenderObject],
        render_pass: &mut wgpu::RenderPass<'a>,
    ) {
        for object in objects {
            if !object.visible {
                continue;
            }
            object.draw(render_pass);
        }
    }
}
