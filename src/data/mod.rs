use iced::wgpu;
use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;
use wgpu::util::DeviceExt;

mod load_obj;
pub use load_obj::load_mesh_from_obj;

use crate::app::app_state::canvas_renderer::CanvasRenderer;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
    pub normal: [f32; 3],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
        wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x3,
        },
        wgpu::VertexAttribute {
            offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
            shader_location: 1,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: (std::mem::size_of::<[f32; 3]>() + std::mem::size_of::<[f32; 2]>())
                as wgpu::BufferAddress,
            shader_location: 2,
            format: wgpu::VertexFormat::Float32x3,
        },
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

// 1. Holds the raw GPU geometry data (Vertices and Indices).
pub struct Mesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

impl Mesh {
    pub fn from_obj(
        device: &wgpu::Device,
        path: impl AsRef<Path>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        load_mesh_from_obj(device, path)
    }

    pub fn fallback_triangle(device: &wgpu::Device) -> Self {
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

        Self {
            vertex_buffer,
            index_buffer,
            index_count: 3,
        }
    }
}

/// 2. Holds the visual surface configurations (Textures, Samplers, Shaders, and Bind Groups).
pub struct Material {
    pub pipeline: Arc<wgpu::RenderPipeline>,
    pub texture_bind_group: wgpu::BindGroup,
}

impl Material {
    pub fn new(pipeline: Arc<wgpu::RenderPipeline>, texture_bind_group: wgpu::BindGroup) -> Self {
        Self {
            pipeline,
            texture_bind_group,
        }
    }

    pub fn from_image_path(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: Arc<wgpu::RenderPipeline>,
        layout: &wgpu::BindGroupLayout,
        path: impl AsRef<Path>,
    ) -> Self {
        let image_data = image::open(path.as_ref()).map(|img| img.to_rgba8()).ok();

        let (dimensions, rgba) = match &image_data {
            Some(img) => (img.dimensions(), img.as_raw().as_slice()),
            None => {
                tracing::warn!(
                    "Could not load texture from {:?}, using fallback white texture",
                    path.as_ref()
                );
                ((1, 1), [255u8, 255, 255, 255].as_slice())
            }
        };

        let texture_size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Material Diffuse Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            texture_size,
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material Texture Bind Group"),
            layout,
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

        Self {
            pipeline,
            texture_bind_group,
        }
    }
}

// 3. The Unified RenderObject.
// Combines geometry mesh, visual material, and a dedicated Uniform Buffer for spatial transform.
pub struct RenderObject {
    pub name: String,
    pub mesh: Arc<Mesh>,
    pub material: Arc<Material>,

    // Transform tracking
    pub position: glam::Vec3,
    pub rotation: glam::Quat,
    pub scale: glam::Vec3,

    pub transform_buffer: wgpu::Buffer,
    pub transform_bind_group: wgpu::BindGroup,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TransformUniform {
    pub model_matrix: [[f32; 4]; 4],
}

impl RenderObject {
    pub fn new(
        name: impl Into<String>,
        device: &wgpu::Device,
        mesh: Arc<Mesh>,
        material: Arc<Material>,
        matrix_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let transform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Object Transform Buffer"),
            contents: bytemuck::cast_slice(&[TransformUniform {
                model_matrix: glam::Mat4::IDENTITY.to_cols_array_2d(),
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let transform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Object Transform Bind Group"),
            layout: matrix_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: transform_buffer.as_entire_binding(),
            }],
        });

        Self {
            name: name.into(),
            mesh,
            material,
            position: glam::Vec3::ZERO,
            rotation: glam::Quat::IDENTITY,
            scale: glam::Vec3::ONE,
            transform_buffer,
            transform_bind_group,
        }
    }

    /// Convenience builder to create a complete RenderObject from asset files directly
    pub fn from_files(
        canvas_renderer: &CanvasRenderer,
        obj_path: impl AsRef<Path>,
        texture_path: impl AsRef<Path>,
    ) -> Self {
        let gpu = &canvas_renderer.gpu;
        let mesh = Mesh::from_obj(&gpu.device, obj_path.as_ref()).unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to load {:?} ({e}), using fallback mesh",
                obj_path.as_ref()
            );
            Mesh::fallback_triangle(&gpu.device)
        });
        let name = obj_path
            .as_ref()
            .file_name()
            .unwrap_or(OsStr::new("default_name"))
            .to_str()
            .unwrap_or("default_name");

        let material = Material::from_image_path(
            &gpu.device,
            &gpu.queue,
            canvas_renderer.pipeline.clone(),
            &canvas_renderer.texture_layout,
            texture_path,
        );

        Self::new(
            name,
            &gpu.device,
            Arc::new(mesh),
            Arc::new(material),
            &canvas_renderer.transform_layout,
        )
    }

    pub fn local_matrix(&self) -> glam::Mat4 {
        glam::Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }

    pub fn update_transform(&self, queue: &wgpu::Queue, mvp: glam::Mat4) {
        let uniform = TransformUniform {
            model_matrix: mvp.to_cols_array_2d(),
        };
        queue.write_buffer(&self.transform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    pub fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        render_pass.set_pipeline(&self.material.pipeline);
        render_pass.set_bind_group(0, &self.transform_bind_group, &[]);
        render_pass.set_bind_group(1, &self.material.texture_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.mesh.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..self.mesh.index_count, 0, 0..1);
    }
}
