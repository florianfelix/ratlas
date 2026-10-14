use iced::wgpu;
use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;
use wgpu::util::DeviceExt;

mod load_obj;
pub mod material;
pub mod mesh;
pub use load_obj::load_mesh_from_obj;

use crate::app::app_state::canvas_renderer::CanvasRenderer;
use crate::data::material::Material;
use crate::data::mesh::Mesh;

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
            canvas_renderer.pipelines.default_pipeline.clone(),
            &canvas_renderer.pipelines.texture_layout,
            texture_path,
        );

        Self::new(
            name,
            &gpu.device,
            Arc::new(mesh),
            Arc::new(material),
            &canvas_renderer.pipelines.transform_layout,
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
