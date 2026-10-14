use anyhow::Result;
use iced_wgpu::wgpu;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
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
        is_mouse_pressed: bool,
        last_mouse_pos: Option<(f64, f64)>,
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
                is_mouse_pressed: false,
                last_mouse_pos: None,
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
            is_mouse_pressed,
            last_mouse_pos,
            ..
        } = self
        else {
            return;
        };

        if ui.handle_event(window, &window_event) {
            window.request_redraw();
        }

        // Camera orbit and zoom interaction on viewport mouse events
        match &window_event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: button_state,
                ..
            } => {
                *is_mouse_pressed = *button_state == ElementState::Pressed;
                if !*is_mouse_pressed {
                    *last_mouse_pos = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let current_pos = (position.x, position.y);
                if *is_mouse_pressed {
                    if let Some((prev_x, prev_y)) = *last_mouse_pos {
                        let delta_x = (current_pos.0 - prev_x) as f32;
                        let delta_y = (current_pos.1 - prev_y) as f32;

                        // Rotate camera around target
                        state.camera.rotate_around_target(delta_x, delta_y);
                        window.request_redraw();
                    }
                }
                *last_mouse_pos = Some(current_pos);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let zoom_amount = match delta {
                    MouseScrollDelta::LineDelta(_x, y) => *y * 0.25,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) * 0.005,
                };
                state.camera.zoom(zoom_amount);
                window.request_redraw();
            }
            WindowEvent::CursorLeft { .. } => {
                *is_mouse_pressed = false;
                *last_mouse_pos = None;
            }
            _ => {}
        }

        match window_event {
            WindowEvent::RedrawRequested => {
                let physical_size = window.inner_size();
                if physical_size.width == 0 || physical_size.height == 0 {
                    return;
                }

                // 1. Update UI overlay and dispatch messages directly to AppState
                let ui_result = ui.update(window);
                for message in &ui_result.messages {
                    state.handle_message(message);
                    window.request_redraw();
                }

                // 2. Update 3D viewport canvas with camera and world objects
                let aspect_ratio = physical_size.width as f32 / physical_size.height as f32;
                canvas_renderer.update(&state.camera, &state.world.objects, aspect_ratio);

                // 3. Render scene from AppState world + UI overlay
                let surface_format = gpu.surface_config.format;
                match gpu.render_frame(canvas_renderer, &state.world.objects, |view| {
                    ui.draw(surface_format, view, &ui_result.viewport);
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
