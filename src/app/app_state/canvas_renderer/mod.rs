use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use std::time::Instant;

use iced::wgpu;

use crate::{
    app::app_state::{camera::Camera, gpu::Gpu},
    data::{
        BackgroundUniform, CameraUniform, InstanceRaw, MarkerInstance, RenderObject,
        material::Material, mesh::Mesh, resolve_asset_path,
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
    pub background_buffer: wgpu::Buffer,
    pub background_bind_group: wgpu::BindGroup,
    pub marker_mesh: Arc<Mesh>,
    pub marker_material: Arc<Material>,
    pub visible_instance_count: AtomicU32,
    pub start_time: Instant,
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

        // 3. Background Uniform Buffer (slot 0 for fullscreen background shader)
        let default_bg = BackgroundUniform::default();
        let background_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Background Uniform Buffer"),
            size: std::mem::size_of::<BackgroundUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue
            .write_buffer(&background_buffer, 0, bytemuck::cast_slice(&[default_bg]));

        let background_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Background Bind Group"),
            layout: &pipelines.background_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: background_buffer.as_entire_binding(),
            }],
        });

        // 4. Shared Marker Geometry and Material
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
            background_buffer,
            background_bind_group,
            marker_mesh,
            marker_material,
            visible_instance_count: AtomicU32::new(0),
            start_time: Instant::now(),
        }
    }

    /// Sets custom background gradient colors and resolution.
    pub fn set_background_colors(&self, top: [f32; 4], bottom: [f32; 4], resolution: [f32; 2]) {
        let bg_uniform = BackgroundUniform {
            color_top: top,
            color_bottom: bottom,
            resolution,
            time: self.start_time.elapsed().as_secs_f32(),
            _padding: 0.0,
        };
        self.gpu.queue.write_buffer(
            &self.background_buffer,
            0,
            bytemuck::cast_slice(&[bg_uniform]),
        );
    }

    /// Updates background uniform, camera uniform, and uploads visible marker instances.
    pub fn update(
        &self,
        camera: &Camera,
        objects: &[RenderObject],
        instances: &[MarkerInstance],
        aspect_ratio: f32,
        viewport_size: [f32; 2],
        theme: Option<&iced::Theme>,
    ) {
        // 0. Update background uniform buffer
        let time = self.start_time.elapsed().as_secs_f32();
        let (color_top, color_bottom) = if let Some(th) = theme {
            let p = th.palette();
            (
                [
                    p.background.r * 0.35,
                    p.background.g * 0.35,
                    p.background.b * 0.45,
                    1.0,
                ],
                [
                    p.background.r * 0.75,
                    p.background.g * 0.75,
                    p.background.b * 0.85,
                    1.0,
                ],
            )
        } else {
            ([0.03, 0.03, 0.08, 1.0], [0.08, 0.07, 0.16, 1.0])
        };

        let bg_uniform = BackgroundUniform {
            color_top,
            color_bottom,
            resolution: viewport_size,
            time,
            _padding: 0.0,
        };
        self.gpu.queue.write_buffer(
            &self.background_buffer,
            0,
            bytemuck::cast_slice(&[bg_uniform]),
        );

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

    /// Renders fullscreen background shader, single scene objects, and all marker instances
    pub fn render<'a>(
        &'a self,
        objects: &'a [RenderObject],
        render_pass: &mut wgpu::RenderPass<'a>,
    ) {
        // 0. Render fullscreen background triangle controlled by shader + uniform buffer
        render_pass.set_pipeline(&self.pipelines.background_pipeline);
        render_pass.set_bind_group(0, &self.background_bind_group, &[]);
        render_pass.draw(0..3, 0..1);

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
