use std::sync::Arc;

use winit::dpi::PhysicalSize;

#[derive(Clone)]
pub struct Gpu {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: Arc<wgpu::Surface<'static>>,
    surface_config: wgpu::SurfaceConfiguration,
}
impl Gpu {
    pub async fn from_window(window: Arc<winit::window::Window>) -> Self {
        let instance = wgpu::Instance::default();
        let surface = Arc::new(instance.create_surface(window.clone()).unwrap());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .expect("Failed to find suitable GPU adapter");

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let device = Arc::new(device);
        let queue = Arc::new(queue);

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

        Self {
            device,
            queue,
            surface,
            surface_config,
        }
    }

    pub fn handle_resize(&mut self, physical_size: PhysicalSize<u32>) {
        if physical_size.width > 0 && physical_size.height > 0 {
            self.surface_config.width = physical_size.width;
            self.surface_config.height = physical_size.height;
            let _ = self.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            });
            self.surface.configure(&self.device, &self.surface_config);
        }
    }
}
