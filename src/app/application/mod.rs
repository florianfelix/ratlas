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

pub enum Ratlas {
    Uninitialized {
        proxy: EventLoopProxy<ProxyMessage>,
    },
    Running {
        state: AppState,
        window: Arc<Window>,
        gpu: Gpu,
        proxy: EventLoopProxy<ProxyMessage>,
        iced_instance: iced_winit::program::Instance<Ui>,
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

            *self = Self::Running {
                state: AppState::default(),
                window,
                gpu,
                proxy: proxy.clone(),
                iced_instance,
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
            window: _,
            iced_instance: _,
            ..
        } = self
        else {
            return;
        };

        match window_event {
            WindowEvent::RedrawRequested => {
                info!("RedrawRequested");
            }
            WindowEvent::Resized(physical_size) => gpu.handle_resize(physical_size),
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => {}
        }
    }
}
