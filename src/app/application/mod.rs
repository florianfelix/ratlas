use anyhow::Result;
use iced_wgpu::wgpu;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

use crate::{
    app::{
        app_state::{AppState, canvas_renderer::CanvasRenderer, gpu::Gpu},
        ui::IcedUi,
    },
    data::RenderObject,
};

#[derive(Debug, Clone)]
pub enum ProxyMessage {}

#[allow(clippy::large_enum_variant)]
pub enum Ratlas {
    Uninitialized {
        proxy: EventLoopProxy<ProxyMessage>,
    },
    Running {
        state: AppState,
        window: Arc<Window>,
        gpu: Gpu,
        #[allow(dead_code)]
        proxy: EventLoopProxy<ProxyMessage>,
        ui: IcedUi,
        canvas_renderer: CanvasRenderer,
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
            let canvas_renderer = CanvasRenderer::new(gpu.clone());

            // Create initial world in AppState and load default sphere object
            let mut state = AppState::default();
            let default_sphere = RenderObject::from_files(
                &canvas_renderer,
                "assets/sphere.obj",
                "assets/sphere_diffuse.jpg",
            );
            state.world.add_object(default_sphere);

            window.request_redraw();

            *self = Self::Running {
                state,
                window,
                gpu,
                proxy: proxy.clone(),
                ui,
                canvas_renderer,
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
            state,
            gpu,
            window,
            ui,
            canvas_renderer,
            ..
        } = self
        else {
            return;
        };

        if ui.handle_event(window, &window_event) {
            window.request_redraw();
        }

        // Example: interact with world objects on window events
        match &window_event {
            WindowEvent::CursorMoved { .. } => {
                // Future viewport mouse drag / object manipulation hooks can go here
            }
            _ => {}
        }

        match window_event {
            WindowEvent::RedrawRequested => {
                let physical_size = window.inner_size();
                if physical_size.width == 0 || physical_size.height == 0 {
                    return;
                }

                // 1. Update world objects in AppState (e.g. spin sphere around Y)
                for obj in &mut state.world.objects {
                    obj.rotation *= glam::Quat::from_rotation_y(0.01);
                    obj.position = glam::Vec3::from_array([
                        obj.position[0],
                        ui.state.counter as f32 * 0.01,
                        obj.position[0],
                    ]);
                }

                // 2. Update 3D viewport canvas with current world objects
                let aspect_ratio = physical_size.width as f32 / physical_size.height as f32;
                canvas_renderer.update(&state.world.objects, aspect_ratio);

                // 3. Update UI overlay
                let (viewport, _event_statuses) = ui.update(window);
                let _counter = ui.state.counter;

                // 4. Render scene from AppState world + UI overlay
                let surface_format = gpu.surface_config.format;
                match gpu.render_frame(canvas_renderer, &state.world.objects, |view| {
                    ui.draw(surface_format, view, &viewport);
                }) {
                    Ok(_) => {
                        window.request_redraw();
                    }
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
