use crate::{app::app_state::canvas_renderer::CanvasRenderer, data::RenderObject, wgpu};
use std::sync::Arc;
#[allow(unused)]
use tracing::info;
use winit::dpi::PhysicalSize;

#[derive(Clone)]
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface: Option<Arc<wgpu::Surface<'static>>>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub adapter: Arc<wgpu::Adapter>,
    pub depth_texture: Arc<wgpu::Texture>,
    pub depth_view: Arc<wgpu::TextureView>,
}

impl Gpu {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width: config.width.max(1),
            height: config.height.max(1),
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    pub async fn from_window(window: Arc<winit::window::Window>) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = Arc::new(instance.create_surface(window.clone()).unwrap());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find suitable GPU adapter");

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let adapter = Arc::new(adapter);

        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                *f == wgpu::TextureFormat::Bgra8Unorm || *f == wgpu::TextureFormat::Rgba8Unorm
            })
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let (depth_texture, depth_view) = Self::create_depth_texture(&device, &surface_config);

        Self {
            instance,
            device,
            queue,
            surface: Some(surface),
            surface_config,
            adapter,
            depth_texture: Arc::new(depth_texture),
            depth_view: Arc::new(depth_view),
        }
    }

    /// Recreates the surface and depth texture for a newly restored window,
    /// reusing the existing adapter, device, and queue.
    pub fn recreate_surface(&mut self, window: Arc<winit::window::Window>) {
        let surface = Arc::new(self.instance.create_surface(window.clone()).unwrap());
        let size = window.inner_size();
        self.surface_config.width = size.width.max(1);
        self.surface_config.height = size.height.max(1);
        surface.configure(&self.device, &self.surface_config);
        let (depth_texture, depth_view) =
            Self::create_depth_texture(&self.device, &self.surface_config);
        self.surface = Some(surface);
        self.depth_texture = Arc::new(depth_texture);
        self.depth_view = Arc::new(depth_view);
    }

    /// Drops the WGPU surface, releasing the underlying Arc<Window> reference.
    pub fn destroy_surface(&mut self) {
        self.surface = None;
    }

    pub fn create_iced_renderer(&self) -> iced_wgpu::Renderer {
        let engine = iced_wgpu::Engine::new(
            &self.adapter,
            (*self.device).clone(),
            (*self.queue).clone(),
            self.surface_config.format,
            None,
            iced_wgpu::graphics::shell::Shell::headless(),
        );
        iced_wgpu::Renderer::new(engine, iced::Font::DEFAULT, iced::Pixels(16.0))
    }

    pub fn handle_resize(&mut self, physical_size: PhysicalSize<u32>) {
        if physical_size.width > 0 && physical_size.height > 0 {
            self.surface_config.width = physical_size.width;
            self.surface_config.height = physical_size.height;
            let _ = self.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            });
            if let Some(ref surface) = self.surface {
                surface.configure(&self.device, &self.surface_config);
            }

            let (depth_texture, depth_view) =
                Self::create_depth_texture(&self.device, &self.surface_config);
            self.depth_texture = Arc::new(depth_texture);
            self.depth_view = Arc::new(depth_view);
        }
    }

    #[tracing::instrument(skip_all)]
    pub fn render_frame<F>(
        &mut self,
        canvas_renderer: &CanvasRenderer,
        objects: &[RenderObject],
        render_iced: F,
    ) -> Result<(), wgpu::SurfaceError>
    where
        F: FnOnce(&wgpu::TextureView),
    {
        let Some(ref surface) = self.surface else {
            return Ok(());
        };
        let frame = surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // 1. Scene Pass (Fullscreen background shader + 3D viewport rendering with depth testing)
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            canvas_renderer.render(objects, &mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // 2. Render Iced UI overlay on top
        render_iced(&view);

        frame.present();

        Ok(())
    }
}
