use anyhow::Result;
use std::sync::Arc;
use tracing::info;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

use crate::app::{
    app_state::{AppState, gpu::Gpu},
    ui::Ui,
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
        window_id: iced::window::Id,
        gpu: Gpu,
        #[allow(dead_code)]
        proxy: EventLoopProxy<ProxyMessage>,
        iced_instance: iced_winit::program::Instance<Ui>,
        iced_renderer: iced_wgpu::Renderer,
        ui_cache: iced_runtime::user_interface::Cache,
        cursor_position: iced_wgpu::core::mouse::Cursor,
        modifiers: winit::keyboard::ModifiersState,
        clipboard: iced_winit::clipboard::Clipboard,
        iced_events: Vec<iced_wgpu::core::Event>,
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
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Self::Uninitialized { proxy } = self {
            let window_attributes = Window::default_attributes()
                .with_title("Ratlas")
                .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0))
                .with_min_inner_size(winit::dpi::LogicalSize::new(640.0, 480.0));

            let window = Arc::new(
                event_loop
                    .create_window(window_attributes)
                    .expect("unable to create window"),
            );
            let gpu = pollster::block_on(Gpu::from_window(window.clone()));

            let ui_program = Ui::default();
            let (iced_instance, _task) = iced_winit::program::Instance::new(ui_program);
            let iced_renderer = gpu.create_iced_renderer();
            let clipboard = iced_winit::clipboard::Clipboard::connect(window.clone());
            let window_id = iced::window::Id::unique();

            window.request_redraw();

            *self = Self::Running {
                state: AppState::default(),
                window,
                window_id,
                gpu,
                proxy: proxy.clone(),
                iced_instance,
                iced_renderer,
                ui_cache: iced_runtime::user_interface::Cache::default(),
                cursor_position: iced_wgpu::core::mouse::Cursor::Unavailable,
                modifiers: winit::keyboard::ModifiersState::default(),
                clipboard,
                iced_events: Vec::new(),
            };
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {}

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        window_event: WindowEvent,
    ) {
        let Self::Running {
            gpu,
            window,
            window_id,
            iced_instance,
            iced_renderer,
            ui_cache,
            cursor_position,
            modifiers,
            clipboard,
            iced_events,
            ..
        } = self
        else {
            return;
        };

        // 1. Update cursor & modifiers tracking (converting to logical coordinates for HiDPI/Wayland)
        match &window_event {
            WindowEvent::CursorMoved { position, .. } => {
                let logical_pos = position.to_logical::<f64>(window.scale_factor());
                *cursor_position = iced_wgpu::core::mouse::Cursor::Available(
                    iced_wgpu::core::Point::new(logical_pos.x as f32, logical_pos.y as f32),
                );
            }
            WindowEvent::CursorLeft { .. } => {
                *cursor_position = iced_wgpu::core::mouse::Cursor::Unavailable;
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                *modifiers = new_modifiers.state();
            }
            _ => {}
        }

        // 2. Convert winit event to Iced event
        if let Some(iced_event) = iced_winit::conversion::window_event(
            window_event.clone(),
            window.scale_factor() as f32,
            *modifiers,
        ) {
            iced_events.push(iced_event);
            window.request_redraw();
        }

        // 3. Handle window events
        match window_event {
            WindowEvent::RedrawRequested => {
                let physical_size = window.inner_size();
                if physical_size.width == 0 || physical_size.height == 0 {
                    return;
                }

                let viewport = iced_wgpu::graphics::Viewport::with_physical_size(
                    iced_wgpu::core::Size::new(physical_size.width, physical_size.height),
                    window.scale_factor() as f32,
                );

                // Build UserInterface
                let mut user_interface = iced_runtime::UserInterface::build(
                    iced_instance.view(*window_id),
                    viewport.logical_size(),
                    std::mem::take(ui_cache),
                    iced_renderer,
                );

                // Process pending Iced events
                let mut messages = Vec::new();
                let (_state, _event_statuses) = user_interface.update(
                    iced_events,
                    *cursor_position,
                    iced_renderer,
                    clipboard,
                    &mut messages,
                );
                iced_events.clear();

                if !messages.is_empty() {
                    *ui_cache = user_interface.into_cache();
                    for message in messages {
                        info!("Processing Iced Message: {:?}", message);
                        let _task = iced_instance.update(message);
                    }
                    user_interface = iced_runtime::UserInterface::build(
                        iced_instance.view(*window_id),
                        viewport.logical_size(),
                        std::mem::take(ui_cache),
                        iced_renderer,
                    );
                }

                // Draw UI primitives
                user_interface.draw(
                    iced_renderer,
                    &iced::Theme::Dark,
                    &iced_wgpu::core::renderer::Style {
                        text_color: iced::Color::WHITE,
                    },
                    *cursor_position,
                );

                *ui_cache = user_interface.into_cache();

                // Render frame
                let surface_format = gpu.surface_config.format;
                match gpu.render_frame(|view| {
                    iced_renderer.present(
                        None, // transparent overlay on top of WGPU scene
                        surface_format,
                        view,
                        &viewport,
                    );
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
