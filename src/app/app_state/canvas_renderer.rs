use iced::wgpu::{self, util::DeviceExt};
use std::sync::Arc;

use crate::{
    app::app_state::gpu::Gpu,
    data::{Material, Mesh, RenderObject, Vertex},
};

pub struct CanvasRenderer {
    gpu: Gpu,
    pub objects: Vec<RenderObject>,
    #[allow(dead_code)]
    pub transform_layout: wgpu::BindGroupLayout,
    pub rotation: f32,
}

impl CanvasRenderer {
    pub fn new(gpu: Gpu) -> Self {
        // 1. Uniform Bind Group Layout (slot 0: Model Matrix)
        let transform_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                });

        // 2. Texture Bind Group Layout (slot 1: Texture + Sampler)
        let texture_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                });

        // 3. Default 1x1 White Texture & Sampler
        let texture_size = wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        };
        let white_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Default White Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &white_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            texture_size,
        );
        let texture_view = white_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Default Texture Bind Group"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // 4. Render Pipeline
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

        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                    cull_mode: None, // Render both sides so monkey is fully visible
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        let material = Arc::new(Material {
            pipeline: Arc::new(pipeline),
            texture_bind_group,
        });

        // 5. Load Mesh (assets/monkey.obj or fallback triangle)
        let mesh = crate::data::load_mesh_from_obj(&gpu.device, "assets/monkey.obj")
            .or_else(|_| crate::data::load_mesh_from_obj(&gpu.device, "./assets/monkey.obj"))
            .unwrap_or_else(|e| {
                tracing::warn!("Could not load assets/monkey.obj ({e}), using fallback mesh");
                create_fallback_mesh(&gpu.device)
            });

        let monkey_object =
            RenderObject::new(&gpu.device, Arc::new(mesh), material, &transform_layout);

        Self {
            gpu,
            objects: vec![monkey_object],
            transform_layout,
            rotation: 0.0,
        }
    }

    pub fn update(&mut self, aspect_ratio: f32) {
        self.rotation += 0.015;

        // View: camera placed at (0, 0.2, 2.8) looking at (0, 0, 0)
        let view = glam::camera::rh::view::look_at_mat4(
            glam::Vec3::new(0.0, 0.2, 2.8),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
        );

        // Perspective projection for WebGPU (Y-up, Z in [0, 1])
        let proj = glam::camera::rh::proj::directx::perspective(
            45.0_f32.to_radians(),
            aspect_ratio.max(0.1),
            0.1,
            100.0,
        );

        // Model: rotate around Y axis and slight X tilt
        let model = glam::Mat4::from_rotation_y(self.rotation) * glam::Mat4::from_rotation_x(0.2);

        let mvp = proj * view * model;

        for object in &self.objects {
            object.update_transform(&self.gpu.queue, mvp);
        }
    }

    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        for object in &self.objects {
            object.draw(render_pass);
        }
    }
}

fn create_fallback_mesh(device: &wgpu::Device) -> Mesh {
    let vertices = [
        Vertex {
            position: [0.0, 0.6, 0.0],
            tex_coords: [0.5, 1.0],
            normal: [0.0, 0.0, 1.0],
        },
        Vertex {
            position: [-0.6, -0.6, 0.0],
            tex_coords: [0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
        },
        Vertex {
            position: [0.6, -0.6, 0.0],
            tex_coords: [1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
        },
    ];
    let indices: [u16; 3] = [0, 1, 2];

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Fallback Vertex Buffer"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Fallback Index Buffer"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    Mesh {
        vertex_buffer,
        index_buffer,
        index_count: 3,
    }
}
