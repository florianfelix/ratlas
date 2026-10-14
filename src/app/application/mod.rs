use anyhow::Result;
use iced_wgpu::wgpu;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

use crate::app::{
    app_state::{AppState, gpu::Gpu},
    ui::IcedUi,
};

#[derive(Debug, Clone)]
pub enum ProxyMessage {}

#[allow(clippy::large_enum_variant)]
pub enum Ratlas {
    Uninitialized {
        proxy: EventLoopProxy<ProxyMessage>,
    },
    Running {
        #[allow(dead_code)]
        state: AppState,
        window: Arc<Window>,
        gpu: Gpu,
        #[allow(dead_code)]
        proxy: EventLoopProxy<ProxyMessage>,
        ui: IcedUi,
    },
}

impl Ratlas {
    pub fn run() -> Result<()> {
        let event_loop = EventLoop::<ProxyMessage>::with_user_event().build()?;
        let proxy = event_loop.create_proxy();

        let mut app = Ratlas::Uninitialized { proxy };
        event_loop.run_app(&mut app)?;
        Ok(())
    }
}

impl ApplicationHandler<ProxyMessage> for Ratlas {
    #[tracing::instrument(skip_all)]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Self::Uninitialized { proxy } = self {
            let window_attributes = Window::default_attributes()
                .with_title("Ratlas floating")
                .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0))
                .with_min_inner_size(winit::dpi::LogicalSize::new(640.0, 480.0));

            let window = Arc::new(
                event_loop
                    .create_window(window_attributes)
                    .expect("unable to create window"),
            );
            let gpu = pollster::block_on(Gpu::from_window(window.clone()));
            let ui = IcedUi::new(window.clone(), &gpu);

            window.request_redraw();

            *self = Self::Running {
                state: AppState::default(),
                window,
                gpu,
                proxy: proxy.clone(),
                ui,
            };
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {}

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {}

    #[tracing::instrument(skip_all)]
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        window_event: WindowEvent,
    ) {
        let Self::Running {
            gpu, window, ui, ..
        } = self
        else {
            return;
        };

        if ui.handle_event(window, &window_event) {
            window.request_redraw();
        }

        match window_event {
            WindowEvent::RedrawRequested => {
                let physical_size = window.inner_size();
                if physical_size.width == 0 || physical_size.height == 0 {
                    return;
                }

                let (viewport, _event_statuses) = ui.update(window);
                let _counter = ui.state.counter;

                let surface_format = gpu.surface_config.format;
                match gpu.render_frame(|view| {
                    ui.draw(surface_format, view, &viewport);
                }) {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        let size = window.inner_size();
                        gpu.handle_resize(size);
                        window.request_redraw();
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        event_loop.exit();
                    }
                    Err(e) => {
                        tracing::error!("Surface error: {:?}", e);
                    }
                }
            }
            WindowEvent::Resized(physical_size) => {
                gpu.handle_resize(physical_size);
                window.request_redraw();
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => {}
        }
    }
}
