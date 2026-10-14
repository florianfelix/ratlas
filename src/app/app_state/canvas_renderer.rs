use iced::wgpu;
use std::sync::Arc;

use crate::{
    app::app_state::gpu::Gpu,
    data::{RenderObject, mesh::Vertex},
};

pub struct CanvasRenderer {
    pub gpu: Gpu,
    pub transform_layout: Arc<wgpu::BindGroupLayout>,
    pub texture_layout: Arc<wgpu::BindGroupLayout>,
    pub pipeline: Arc<wgpu::RenderPipeline>,
    pub camera_pos: glam::Vec3,
    pub camera_target: glam::Vec3,
}

impl CanvasRenderer {
    pub fn new(gpu: Gpu) -> Self {
        // 1. Uniform Bind Group Layout (slot 0: Model Matrix)
        let transform_layout = Arc::new(gpu.device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Transform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            },
        ));

        // 2. Texture Bind Group Layout (slot 1: Texture + Sampler)
        let texture_layout = Arc::new(gpu.device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            },
        ));

        // 3. Render Pipeline
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Default Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/default.wgsl").into()),
            });

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Canvas Pipeline Layout"),
                bind_group_layouts: &[&transform_layout, &texture_layout],
                push_constant_ranges: &[],
            });

        let pipeline = Arc::new(gpu.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Canvas Render Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::desc()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: gpu.surface_config.format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));

        Self {
            gpu,
            transform_layout,
            texture_layout,
            pipeline,
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
