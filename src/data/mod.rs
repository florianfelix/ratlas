use iced::wgpu;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub mod effector;
mod load_obj;
pub mod material;
pub mod mesh;

pub use effector::{Effector, PickingEffector};
pub use load_obj::load_mesh_from_obj;

use crate::app::app_state::canvas_renderer::CanvasRenderer;
use crate::data::material::Material;
use crate::data::mesh::Mesh;

/// Resolves an asset file path, checking the current directory, executable directory,
/// parent directory (e.g. AppImage mounted bundle), or system share directories.
pub fn resolve_asset_path(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if path.exists() {
        return path.to_path_buf();
    }
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let candidate1 = exe_dir.join(path);
            if candidate1.exists() {
                return candidate1;
            }
            let candidate2 = exe_dir.join("..").join(path);
            if candidate2.exists() {
                return candidate2;
            }
            let candidate3 = exe_dir.join("..").join("share").join("ratlas").join(path);
            if candidate3.exists() {
                return candidate3;
            }
        }
    }
    path.to_path_buf()
}

// 3. The Unified RenderObject.
// Combines geometry mesh, visual material, and a dedicated Uniform Buffer for spatial transform.
pub struct RenderObject {
    pub name: String,
    pub tag: String,
    pub id: Option<String>,
    pub mesh: Arc<Mesh>,
    pub material: Arc<Material>,

    // Visibility flag (e.g. for horizon / frustum culling)
    pub visible: bool,

    // Transform tracking
    pub position: glam::Vec3,
    pub rotation: glam::Quat,
    pub scale: glam::Vec3,
    pub base_scale: glam::Vec3, // Base scale before effector modulation
    pub color_tint: glam::Vec4, // (r, g, b, a) color tint multiplier

    pub transform_buffer: wgpu::Buffer,
    pub transform_bind_group: wgpu::BindGroup,

    // GPU device and bind group layout retained for fast instancing
    pub device: Arc<wgpu::Device>,
    pub transform_layout: Arc<wgpu::BindGroupLayout>,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TransformUniform {
    pub model_matrix: [[f32; 4]; 4],
    pub color_tint: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub model_matrix: [[f32; 4]; 4],
    pub color_tint: [f32; 4],
}

impl InstanceRaw {
    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = [
        wgpu::VertexAttribute {
            offset: 0,
            shader_location: 5,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 16,
            shader_location: 6,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 32,
            shader_location: 7,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 48,
            shader_location: 8,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 64,
            shader_location: 9,
            format: wgpu::VertexFormat::Float32x4,
        },
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Lightweight CPU representation of an instanced marker on the globe.
#[derive(Debug, Clone)]
pub struct MarkerInstance {
    pub id: String,
    pub name: String,
    pub position: glam::Vec3,
    pub rotation: glam::Quat,
    pub scale: glam::Vec3,
    pub base_scale: glam::Vec3,
    pub color_tint: glam::Vec4,
    pub visible: bool,
}

impl MarkerInstance {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        position: glam::Vec3,
        rotation: glam::Quat,
        scale: glam::Vec3,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            position,
            rotation,
            scale,
            base_scale: scale,
            color_tint: glam::Vec4::ONE,
            visible: true,
        }
    }
}

impl RenderObject {
    pub fn new(
        name: impl Into<String>,
        device: Arc<wgpu::Device>,
        mesh: Arc<Mesh>,
        material: Arc<Material>,
        transform_layout: Arc<wgpu::BindGroupLayout>,
    ) -> Self {
        let transform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Object Transform Buffer"),
            contents: bytemuck::cast_slice(&[TransformUniform {
                model_matrix: glam::Mat4::IDENTITY.to_cols_array_2d(),
                color_tint: glam::Vec4::ONE.to_array(),
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let transform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Object Transform Bind Group"),
            layout: &transform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: transform_buffer.as_entire_binding(),
            }],
        });

        Self {
            name: name.into(),
            tag: String::new(),
            id: None,
            mesh,
            material,
            visible: true,
            position: glam::Vec3::ZERO,
            rotation: glam::Quat::IDENTITY,
            scale: glam::Vec3::ONE,
            base_scale: glam::Vec3::ONE,
            color_tint: glam::Vec4::ONE,
            transform_buffer,
            transform_bind_group,
            device,
            transform_layout,
        }
    }

    /// Convenience builder to create a complete RenderObject from asset files directly
    pub fn from_files(
        canvas_renderer: &CanvasRenderer,
        obj_path: impl AsRef<Path>,
        texture_path: impl AsRef<Path>,
    ) -> Self {
        let resolved_obj = resolve_asset_path(obj_path.as_ref());
        let resolved_texture = resolve_asset_path(texture_path.as_ref());

        let gpu = &canvas_renderer.gpu;
        let mesh = Mesh::from_obj(&gpu.device, &resolved_obj).unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to load {:?} ({e}), using fallback mesh",
                resolved_obj
            );
            Mesh::fallback_triangle(&gpu.device)
        });
        let name = resolved_obj
            .file_name()
            .unwrap_or(OsStr::new("default_name"))
            .to_str()
            .unwrap_or("default_name");

        let material = Material::from_image_path(
            &gpu.device,
            &gpu.queue,
            canvas_renderer.pipelines.default_pipeline.clone(),
            &canvas_renderer.pipelines.texture_layout,
            &resolved_texture,
        );

        Self::new(
            name,
            gpu.device.clone(),
            Arc::new(mesh),
            Arc::new(material),
            canvas_renderer.pipelines.transform_layout.clone(),
        )
    }

    /// Creates an independent instance sharing mesh, material, and pipeline resources,
    /// but with its own transform buffer, bind group, and transform properties.
    pub fn create_instance(&self) -> Self {
        let transform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{} Instance Transform Buffer", self.name)),
                contents: bytemuck::cast_slice(&[TransformUniform {
                    model_matrix: self.local_matrix().to_cols_array_2d(),
                    color_tint: self.color_tint.to_array(),
                }]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let transform_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("{} Instance Transform Bind Group", self.name)),
            layout: &self.transform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: transform_buffer.as_entire_binding(),
            }],
        });

        Self {
            name: self.name.clone(),
            tag: self.tag.clone(),
            id: self.id.clone(),
            mesh: self.mesh.clone(),
            material: self.material.clone(),
            visible: self.visible,
            position: self.position,
            rotation: self.rotation,
            scale: self.scale,
            base_scale: self.base_scale,
            color_tint: self.color_tint,
            transform_buffer,
            transform_bind_group,
            device: self.device.clone(),
            transform_layout: self.transform_layout.clone(),
        }
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_transform(
        mut self,
        position: glam::Vec3,
        rotation: glam::Quat,
        scale: glam::Vec3,
    ) -> Self {
        self.position = position;
        self.rotation = rotation;
        self.scale = scale;
        self.base_scale = scale;
        self
    }

    pub fn with_color_tint(mut self, tint: glam::Vec4) -> Self {
        self.color_tint = tint;
        self
    }

    /// Computes the position and outward surface normal on a sphere at given latitude and longitude
    /// (in degrees) with specified radius.
    /// Coordinates are aligned with the equirectangular earth texture mapping.
    pub fn sphere_coords(lat_deg: f32, lon_deg: f32, radius: f32) -> (glam::Vec3, glam::Vec3) {
        let lat_rad = lat_deg.to_radians();
        let lon_rad = lon_deg.to_radians();

        let x = lat_rad.cos() * lon_rad.cos();
        let y = lat_rad.sin();
        let z = -lat_rad.cos() * lon_rad.sin();

        let normal = glam::Vec3::new(x, y, z).normalize();
        let position = normal * radius;
        (position, normal)
    }

    pub fn local_matrix(&self) -> glam::Mat4 {
        glam::Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }

    pub fn update_transform(&self, queue: &wgpu::Queue, mvp: glam::Mat4) {
        let uniform = TransformUniform {
            model_matrix: mvp.to_cols_array_2d(),
            color_tint: self.color_tint.to_array(),
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
