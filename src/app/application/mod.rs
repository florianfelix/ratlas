mod input;
mod render;
mod types;
mod update;
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
    playback::PlaybackStatus,
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
        channels
            .tx_radio
            .send(RadioMessage::GetPopular)
            .expect("This should work");
        channels
            .tx_radio
            .send(RadioMessage::GetTags)
            .expect("This should work");

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
            let tray = TrayManager::new().unwrap_or_else(|e| {
                tracing::warn!("Failed to initialize system tray icon: {e}");
                TrayManager::empty()
            });

            // Create initial world in AppState and load default sphere object
            let mut state = AppState::default();
            let mut default_sphere = RenderObject::from_files(
                &canvas_renderer,
                "assets/sphere.obj",
                state.earth_config.base_texture.asset_path(),
            );
            default_sphere.tag = "earth".to_string();
            state.world.add_object(default_sphere);

            // Load country outlines mesh
            let mut country_outlines = RenderObject::unlit_from_obj(
                &canvas_renderer,
                "assets/country_outlines.obj",
                state.earth_config.outline_color,
            );
            country_outlines.tag = "country_outlines".to_string();
            // Scale slightly above 1.000 so outline ribbons lie cleanly above earth sphere and below markers (1.002)
            country_outlines.scale = glam::Vec3::splat(1.0008);
            country_outlines.base_scale = country_outlines.scale;
            country_outlines.visible = state.earth_config.show_country_outlines;
            state.world.add_object(country_outlines);

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

    #[tracing::instrument(skip_all)]
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ProxyMessage) {
        match event {
            ProxyMessage::RadioStationsLoaded(stations) => {
                tracing::info!(
                    "Received {} popular radio stations with geolocation info",
                    stations.len()
                );
                if let Self::Running { state, window, .. } = self {
                    state.update_station_markers(stations);
                    if let Some(w) = window {
                        w.request_redraw();
                    }
                }
            }
            ProxyMessage::RadioStationTags(tags) => {
                tracing::info!("Retrieved {} tags", tags.len());
                if let Self::Running { state, .. } = self {
                    state.api_tags = tags;
                }
            }
            ProxyMessage::PlaybackStatusChanged(status) => {
                tracing::info!("Playback status updated: {:?}", status);
                if let Self::Running { state, window, .. } = self {
                    if matches!(
                        status,
                        PlaybackStatus::Stopped | PlaybackStatus::Error { .. }
                    ) {
                        state.set_playing_station(None);
                    }
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

        let is_mouse_event = matches!(
            window_event,
            WindowEvent::MouseInput { .. }
                | WindowEvent::CursorMoved { .. }
                | WindowEvent::MouseWheel { .. }
        );

        let swallowed_by_ui = ui.handle_event(window, &window_event);
        if swallowed_by_ui {
            window.request_redraw();
            if is_mouse_event && !mouse.is_pressed {
                state.mouse_globe_pos = None;
                return;
            }
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
                // 1. Update 3D world (camera damping & dynamic effectors)
                state.update(window);

                // 2. Project AppState into UiState
                ui.apply_appstate(state);

                // 3. Update Iced UI overlay and dispatch messages to AppState / channels
                let viewport = update::update_ui(ui, state, canvas_renderer, channels, window);

                // 4. Pure GPU rendering (strictly immutable state)
                render::render_frame(state, gpu, canvas_renderer, ui, &viewport, window);
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
