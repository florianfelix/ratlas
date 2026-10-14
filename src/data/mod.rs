use iced::wgpu;
use std::sync::Arc;
use wgpu::util::DeviceExt;

mod load_obj;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
    pub normal: [f32; 3],
}

// 1. Holds the raw GPU geometry data (Vertices and Indices).
// This is reusable across multiple objects (e.g., thousands of globes can use one Mesh).
pub struct Mesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

/// 2. Holds the visual surface configurations (Textures, Samplers, Shaders, and Bind Groups).
pub struct Material {
    pub pipeline: Arc<wgpu::RenderPipeline>,
    pub texture_bind_group: wgpu::BindGroup,
}

// 3. The Unified "Blender Object" equivalent.
// Combines a geometry mesh, a visual appearance material, and a dedicated
// Uniform Buffer instance tracking its unique spatial transformation data (position, spin).
pub struct RenderObject {
    pub mesh: Arc<Mesh>,
    pub material: Arc<Material>,

    // Transform tracking
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
        device: &wgpu::Device,
        mesh: Arc<Mesh>,
        material: Arc<Material>,
        matrix_bind_group_layout: &wgpu::BindGroupLayout, // Layout for Uniform slot 0
    ) -> Self {
        // Create a unique GPU buffer to store this object's specific transform matrix
        let transform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Object Transform Buffer"),
            contents: bytemuck::cast_slice(&[TransformUniform {
                model_matrix: glam::Mat4::IDENTITY.to_cols_array_2d(),
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Hook the buffer up to its bind group
        let transform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Object Transform Bind Group"),
            layout: matrix_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: transform_buffer.as_entire_binding(),
            }],
        });

        Self {
            mesh,
            material,
            transform_buffer,
            transform_bind_group,
        }
    }

    /// Update function called inside your winit Event Loop before rendering
    pub fn update_transform(&self, queue: &wgpu::Queue, model_matrix: glam::Mat4) {
        let uniform = TransformUniform {
            model_matrix: model_matrix.to_cols_array_2d(),
        };
        queue.write_buffer(&self.transform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    /// High-level draw call pass orchestration
    pub fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        // Set configuration pipeline
        render_pass.set_pipeline(&self.material.pipeline);

        // Bind layout space 0: Dynamic Position (Transform Matrix)
        render_pass.set_bind_group(0, &self.transform_bind_group, &[]);

        // Bind layout space 1: Static Appearance (Textures & Samplers)
        render_pass.set_bind_group(1, &self.material.texture_bind_group, &[]);

        // Draw structural geometry elements
        render_pass.set_vertex_buffer(0, self.mesh.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..self.mesh.index_count, 0, 0..1);
    }
}
