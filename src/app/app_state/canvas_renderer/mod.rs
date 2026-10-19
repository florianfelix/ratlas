use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use iced::wgpu;

use crate::{
    app::app_state::{camera::Camera, gpu::Gpu},
    data::{
        CameraUniform, InstanceRaw, MarkerInstance, RenderObject, material::Material, mesh::Mesh,
        resolve_asset_path,
    },
};

pub mod pipeline_manager;
pub use pipeline_manager::PipelineManager;

pub struct CanvasRenderer {
    pub gpu: Gpu,
    pub pipelines: PipelineManager,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub instance_buffer: wgpu::Buffer,
    pub marker_mesh: Arc<Mesh>,
    pub marker_material: Arc<Material>,
    pub visible_instance_count: AtomicU32,
}

impl CanvasRenderer {
    /// Maximum capacity of the instanced marker GPU buffer (65,536 instances = ~5.2 MB VRAM).
    /// Comfortably accommodates the entire worldwide radio-browser station catalog.
    pub const MAX_INSTANCES: usize = 65_536;

    pub fn new(gpu: Gpu) -> Self {
        let pipelines = PipelineManager::new(&gpu.device, gpu.surface_config.format);

        // 1. Global Camera Uniform Buffer (Slot 0 for instanced pipeline)
        let camera_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instanced Camera Buffer"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Instanced Camera Bind Group"),
            layout: &pipelines.camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // 2. Dynamic GPU Instance Buffer (slot 1 vertex buffer: Model Matrix + Color Tint per instance)
        // Pre-allocated for up to 65,536 active markers (~5.2 MB VRAM)
        let instance_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instanced Marker GPU Buffer"),
            size: (Self::MAX_INSTANCES * std::mem::size_of::<InstanceRaw>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 3. Shared Marker Geometry and Material
        let resolved_obj = resolve_asset_path("assets/marker.obj");
        let resolved_texture = resolve_asset_path("assets/marker.jpg");

        let marker_mesh = Arc::new(Mesh::from_obj(&gpu.device, &resolved_obj).unwrap_or_else(
            |e| {
                tracing::warn!(
                    "Failed to load {:?} ({e}), using fallback mesh",
                    resolved_obj
                );
                Mesh::fallback_triangle(&gpu.device)
            },
        ));

        let marker_material = Arc::new(Material::from_image_path(
            &gpu.device,
            &gpu.queue,
            pipelines.default_pipeline.clone(),
            &pipelines.texture_layout,
            &resolved_texture,
        ));

        Self {
            gpu,
            pipelines,
            camera_buffer,
            camera_bind_group,
            instance_buffer,
            marker_mesh,
            marker_material,
            visible_instance_count: AtomicU32::new(0),
        }
    }

    /// Updates transform uniform buffers for single objects and uploads visible marker instances
    pub fn update(
        &self,
        camera: &Camera,
        objects: &[RenderObject],
        instances: &[MarkerInstance],
        aspect_ratio: f32,
    ) {
        let view = camera.view_matrix();
        let proj = camera.projection_matrix(aspect_ratio);
        let vp = proj * view;

        // 1. Update camera uniform for instanced pipeline
        let cam_uniform = CameraUniform {
            view_proj: vp.to_cols_array_2d(),
        };
        self.gpu
            .queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[cam_uniform]));

        // 2. Pack and upload visible marker instances in a single buffer write
        let mut raw_data = Vec::with_capacity(instances.len());
        for instance in instances {
            if !instance.visible {
                continue;
            }
            let model = glam::Mat4::from_scale_rotation_translation(
                instance.scale,
                instance.rotation,
                instance.position,
            );
            raw_data.push(InstanceRaw {
                model_matrix: model.to_cols_array_2d(),
                color_tint: instance.color_tint.to_array(),
            });
        }

        // Clamp to buffer capacity to strictly prevent GPU buffer overrun
        if raw_data.len() > Self::MAX_INSTANCES {
            tracing::warn!(
                "Visible marker count ({}) exceeds GPU instance buffer capacity ({}), clamping",
                raw_data.len(),
                Self::MAX_INSTANCES
            );
            raw_data.truncate(Self::MAX_INSTANCES);
        }

        if !raw_data.is_empty() {
            self.gpu
                .queue
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&raw_data));
        }
        self.visible_instance_count
            .store(raw_data.len() as u32, Ordering::Relaxed);

        // 3. Update single scene objects (e.g. Earth) with default pipeline
        for object in objects {
            if !object.visible {
                continue;
            }
            let mvp = vp * object.local_matrix();
            object.update_transform(&self.gpu.queue, mvp);
        }
    }

    /// Renders single scene objects and all marker instances (in a single instanced draw call)
    pub fn render<'a>(
        &'a self,
        objects: &'a [RenderObject],
        render_pass: &mut wgpu::RenderPass<'a>,
    ) {
        // 1. Render single scene objects (Earth, country outlines)
        for object in objects {
            if !object.visible {
                continue;
            }
            object.draw(render_pass);
        }

        // 2. Render all visible marker instances in 1 single draw call
        let count = self.visible_instance_count.load(Ordering::Relaxed);
        if count > 0 {
            render_pass.set_pipeline(&self.pipelines.instanced_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            if let Some(ref bg) = self.marker_material.texture_bind_group {
                render_pass.set_bind_group(1, bg, &[]);
            }
            render_pass.set_vertex_buffer(0, self.marker_mesh.vertex_buffer.slice(..));
            let buffer_bytes = (count as usize * std::mem::size_of::<InstanceRaw>()) as u64;
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(0..buffer_bytes));
            render_pass.set_index_buffer(
                self.marker_mesh.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );
            render_pass.draw_indexed(0..self.marker_mesh.index_count, 0, 0..count);
        }
    }
}
