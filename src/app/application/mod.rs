mod input;
mod render;
mod types;
mod window;

use anyhow::Result;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

pub use input::MouseTracker;
pub use types::{Channels, ProxyMessage, RadioMessage};

use crate::{
    app::{
        app_state::{AppState, canvas_renderer::CanvasRenderer, gpu::Gpu},
        tray::{TrayAction, TrayManager},
        ui::IcedUi,
    },
    data::RenderObject,
    radio_api,
};

#[allow(clippy::large_enum_variant)]
pub enum Ratlas {
    Uninitialized {
        proxy: EventLoopProxy<ProxyMessage>,
        channels: Channels,
    },
    Running {
        state: AppState,
        window: Option<Arc<Window>>,
        gpu: Gpu,
        #[allow(dead_code)]
        proxy: EventLoopProxy<ProxyMessage>,
        channels: Channels,
        ui: IcedUi,
        canvas_renderer: CanvasRenderer,
        mouse: MouseTracker,
        tray: TrayManager,
    },
}

impl Ratlas {
    pub fn run() -> Result<()> {
        let event_loop = EventLoop::<ProxyMessage>::with_user_event().build()?;
        event_loop.set_control_flow(ControlFlow::Wait);
        let proxy = event_loop.create_proxy();
        let channels = Channels::new();

        // 1. RadioBrowser API Worker Thread
        let rx_radio = channels.rx_radio.clone();
        let proxy_radio = proxy.clone();
        let _ = std::thread::spawn(|| radio_api::radio_api_worker(rx_radio, proxy_radio));
        channels.tx_radio.send(RadioMessage::GetPopular).unwrap();

        // 2. Audio Playback Worker Thread
        let rx_playback = channels.rx_playback.clone();
        let proxy_playback = proxy.clone();
        let _ = std::thread::spawn(move || {
            crate::playback::playback_worker(rx_playback, proxy_playback);
        });

        // 3. Tray and Menu event forwarding to Proxy
        TrayManager::init_event_handlers(
            proxy.clone(),
            ProxyMessage::TrayIconEvent,
            ProxyMessage::MenuEvent,
        );

        let mut app = Ratlas::Uninitialized { proxy, channels };
        event_loop.run_app(&mut app)?;
        Ok(())
    }
}

impl ApplicationHandler<ProxyMessage> for Ratlas {
    #[tracing::instrument(skip_all)]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Self::Uninitialized { proxy, channels } = self {
            let window = window::create_window(event_loop);
            let gpu = pollster::block_on(Gpu::from_window(window.clone()));
            let ui = IcedUi::new(window.clone(), &gpu);
            let canvas_renderer = CanvasRenderer::new(gpu.clone());

            // Build system tray icon and context menu
            let tray = TrayManager::new().expect("failed to create tray icon");

            // Create initial world in AppState and load default sphere object
            let mut state = AppState::default();
            let mut default_sphere = RenderObject::from_files(
                &canvas_renderer,
                "assets/sphere.obj",
                "assets/sphere_diffuse.jpg",
            );
            default_sphere.tag = "earth".to_string();
            state.world.add_object(default_sphere);

            let mut marker = RenderObject::from_files(
                &canvas_renderer,
                "assets/marker.obj",
                "assets/marker.jpg",
            );
            marker.name = "marker".to_string();
            marker.tag = "marker_template".to_string();
            state.objects.insert("marker".to_string(), marker);

            window.request_redraw();

            *self = Self::Running {
                state,
                window: Some(window),
                gpu,
                proxy: proxy.clone(),
                channels: channels.clone(),
                ui,
                canvas_renderer,
                mouse: MouseTracker::default(),
                tray,
            };
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ProxyMessage) {
        match event {
            ProxyMessage::RadioStationsLoaded(stations) => {
                tracing::info!(
                    "Received {} popular radio stations with geolocation info",
                    stations.len()
                );
                for (i, station) in stations.iter().enumerate() {
                    tracing::info!(
                        "[{:>2}] {:<30} | lat: {:>8.4}, lon: {:>8.4} | clicks: {}",
                        i + 1,
                        station.name,
                        station.geo_lat.unwrap_or(0.0),
                        station.geo_long.unwrap_or(0.0),
                        station.clickcount
                    );
                }
                if let Self::Running { state, window, .. } = self {
                    state.update_station_markers(stations);
                    if let Some(w) = window {
                        w.request_redraw();
                    }
                }
            }
            ProxyMessage::PlaybackStatusChanged(status) => {
                tracing::info!("Playback status updated: {:?}", status);
                if let Self::Running { state, window, .. } = self {
                    state.playback_status = status;
                    if let Some(w) = window {
                        w.request_redraw();
                    }
                }
            }
            ProxyMessage::TrayIconEvent(tray_event) => {
                if let Self::Running {
                    tray,
                    window,
                    gpu,
                    canvas_renderer,
                    ui,
                    mouse,
                    ..
                } = self
                {
                    if let Some(action) = tray.handle_tray_event(&tray_event) {
                        match action {
                            TrayAction::ToggleWindow => {
                                window::toggle_window(
                                    event_loop,
                                    window,
                                    gpu,
                                    canvas_renderer,
                                    ui,
                                    mouse,
                                );
                            }
                            TrayAction::Quit => {
                                tracing::info!("Tray quit action triggered");
                                event_loop.exit();
                            }
                        }
                    }
                }
            }
            ProxyMessage::MenuEvent(menu_event) => {
                if let Self::Running {
                    tray,
                    window,
                    gpu,
                    canvas_renderer,
                    ui,
                    mouse,
                    ..
                } = self
                {
                    if let Some(action) = tray.handle_menu_event(&menu_event) {
                        match action {
                            TrayAction::ToggleWindow => {
                                window::toggle_window(
                                    event_loop,
                                    window,
                                    gpu,
                                    canvas_renderer,
                                    ui,
                                    mouse,
                                );
                            }
                            TrayAction::Quit => {
                                tracing::info!("Tray menu quit selected: exiting application");
                                event_loop.exit();
                            }
                        }
                    }
                }
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {}

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {}

    #[tracing::instrument(skip_all)]
    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        window_event: WindowEvent,
    ) {
        let Self::Running {
            state,
            gpu,
            window: opt_window,
            channels,
            ui,
            canvas_renderer,
            mouse,
            ..
        } = self
        else {
            return;
        };

        let Some(window) = opt_window.as_ref() else {
            return;
        };

        if ui.handle_event(window, &window_event) {
            window.request_redraw();
        }

        // Camera orbit, hover, selection, and effector interaction on viewport mouse events
        match window_event {
            WindowEvent::MouseInput {
                button,
                state: button_state,
                ..
            } => {
                input::handle_mouse_input(button, button_state, state, channels, window, mouse);
            }
            WindowEvent::CursorMoved { position, .. } => {
                input::handle_cursor_moved(position, state, window, mouse);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                input::handle_mouse_wheel(delta, state, window);
            }
            WindowEvent::CursorLeft { .. } => {
                input::handle_cursor_left(state, window, mouse);
            }
            WindowEvent::RedrawRequested => {
                render::render_frame(state, gpu, canvas_renderer, ui, channels, window);
            }
            WindowEvent::Resized(physical_size) => {
                gpu.handle_resize(physical_size);
                window.request_redraw();
            }
            WindowEvent::CloseRequested => {
                window::hide_window(opt_window, gpu, canvas_renderer, ui, mouse);
            }
            _ => {}
        }
    }
}
