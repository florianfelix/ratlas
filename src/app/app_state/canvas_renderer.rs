use iced::wgpu;

use crate::{
    app::app_state::{canvas_renderer::pipeline_manager::PipelineManager, gpu::Gpu},
    data::RenderObject,
};

mod pipeline_manager;

pub struct CanvasRenderer {
    pub gpu: Gpu,
    pub pipelines: PipelineManager,
    pub camera_pos: glam::Vec3,
    pub camera_target: glam::Vec3,
}

impl CanvasRenderer {
    pub fn new(gpu: Gpu) -> Self {
        let pipelines = PipelineManager::new(&gpu.device, gpu.surface_config.format);

        Self {
            gpu,
            pipelines,
            camera_pos: glam::Vec3::new(0.0, 0.2, 2.8),
            camera_target: glam::Vec3::ZERO,
        }
    }

    /// Updates transform uniform buffers for all objects in the world using camera view/projection
    pub fn update(&self, objects: &[RenderObject], aspect_ratio: f32) {
        let view = glam::camera::rh::view::look_at_mat4(
            self.camera_pos,
            self.camera_target,
            glam::Vec3::Y,
        );

        let proj = glam::camera::rh::proj::directx::perspective(
            45.0_f32.to_radians(),
            aspect_ratio.max(0.1),
            0.1,
            100.0,
        );

        let vp = proj * view;

        for object in objects {
            let mvp = vp * object.local_matrix();
            object.update_transform(&self.gpu.queue, mvp);
        }
    }

    /// Renders all objects passed into the render pass
    pub fn render<'a>(
        &'a self,
        objects: &'a [RenderObject],
        render_pass: &mut wgpu::RenderPass<'a>,
    ) {
        for object in objects {
            object.draw(render_pass);
        }
    }
}
