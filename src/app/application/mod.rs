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
        app_state::{AppState, Camera, canvas_renderer::CanvasRenderer, gpu::Gpu},
        ui::IcedUi,
    },
    data::RenderObject,
    radio_api,
};

#[derive(Debug, Clone)]
pub enum RadioMessage {
    GetPopular,
}

#[derive(Clone)]
pub struct Channels {
    pub tx_radio: crossbeam_channel::Sender<RadioMessage>,
    pub rx_radio: crossbeam_channel::Receiver<RadioMessage>,
    pub tx_playback: crossbeam_channel::Sender<crate::playback::PlaybackMessage>,
    pub rx_playback: crossbeam_channel::Receiver<crate::playback::PlaybackMessage>,
}

impl Channels {
    pub fn new() -> Self {
        let (tx_radio, rx_radio) = crossbeam_channel::unbounded::<RadioMessage>();
        let (tx_playback, rx_playback) =
            crossbeam_channel::unbounded::<crate::playback::PlaybackMessage>();
        Self {
            tx_radio,
            rx_radio,
            tx_playback,
            rx_playback,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ProxyMessage {
    RadioStationsLoaded(Vec<radiobrowser::ApiStation>),
    PlaybackStatusChanged(crate::playback::PlaybackStatus),
}

#[allow(clippy::large_enum_variant)]
pub enum Ratlas {
    Uninitialized {
        proxy: EventLoopProxy<ProxyMessage>,
        channels: Channels,
    },
    Running {
        state: AppState,
        window: Arc<Window>,
        gpu: Gpu,
        #[allow(dead_code)]
        proxy: EventLoopProxy<ProxyMessage>,
        channels: Channels,
        ui: IcedUi,
        canvas_renderer: CanvasRenderer,
        is_mouse_pressed: bool,
        mouse_press_pos: Option<(f64, f64)>,
        last_mouse_pos: Option<(f64, f64)>,
    },
}

impl Ratlas {
    pub fn run() -> Result<()> {
        let event_loop = EventLoop::<ProxyMessage>::with_user_event().build()?;
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

        let mut app = Ratlas::Uninitialized { proxy, channels };
        event_loop.run_app(&mut app)?;
        Ok(())
    }
}

impl ApplicationHandler<ProxyMessage> for Ratlas {
    #[tracing::instrument(skip_all)]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Self::Uninitialized { proxy, channels } = self {
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
                window,
                gpu,
                proxy: proxy.clone(),
                channels: channels.clone(),
                ui,
                canvas_renderer,
                is_mouse_pressed: false,
                mouse_press_pos: None,
                last_mouse_pos: None,
            };
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: ProxyMessage) {
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
                    window.request_redraw();
                }
            }
            ProxyMessage::PlaybackStatusChanged(status) => {
                tracing::info!("Playback status updated: {:?}", status);
                if let Self::Running { state, window, .. } = self {
                    state.playback_status = status;
                    window.request_redraw();
                }
            }
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
            channels,
            ui,
            canvas_renderer,
            is_mouse_pressed,
            mouse_press_pos,
            last_mouse_pos,
            ..
        } = self
        else {
            return;
        };

        if ui.handle_event(window, &window_event) {
            window.request_redraw();
        }

        // Camera orbit, hover, selection, and effector interaction on viewport mouse events
        match &window_event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: button_state,
                ..
            } => {
                let pressed = *button_state == ElementState::Pressed;
                *is_mouse_pressed = pressed;
                if pressed {
                    *mouse_press_pos = *last_mouse_pos;
                } else {
                    state.camera.stop_drag();

                    // If released with minimal mouse travel, treat as a click / selection
                    if let (Some(press_pos), Some(release_pos)) =
                        (*mouse_press_pos, *last_mouse_pos)
                    {
                        let dx = (release_pos.0 - press_pos.0) as f32;
                        let dy = (release_pos.1 - press_pos.1) as f32;
                        let dist_sq = dx * dx + dy * dy;

                        if dist_sq < 25.0 {
                            // Less than 5px movement: process click
                            let physical_size = window.inner_size();
                            let ray = state.camera.screen_to_ray(
                                release_pos,
                                (physical_size.width, physical_size.height),
                            );
                            let clicked_station = state.pick_station(&ray).cloned();
                            if let Some(ref station) = clicked_station {
                                let stream_url = if !station.url_resolved.is_empty() {
                                    station.url_resolved.clone()
                                } else {
                                    station.url.clone()
                                };
                                let _ = channels.tx_playback.send(
                                    crate::playback::PlaybackMessage::Play {
                                        url: stream_url,
                                        name: station.name.clone(),
                                    },
                                );
                            }
                            state.set_selected_station(
                                clicked_station.as_ref().map(|s| s.stationuuid.clone()),
                            );
                            window.request_redraw();
                        }
                    }

                    *mouse_press_pos = None;
                    *last_mouse_pos = None;
                    window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let current_pos = (position.x, position.y);
                let physical_size = window.inner_size();
                let ray = state
                    .camera
                    .screen_to_ray(current_pos, (physical_size.width, physical_size.height));

                // Update 3D point on globe under cursor for mouse lens effector
                state.mouse_globe_pos = ray
                    .intersect_sphere(glam::Vec3::ZERO, Camera::SPHERE_RADIUS)
                    .map(|t| ray.origin + ray.direction * t);

                if *is_mouse_pressed {
                    if let Some((prev_x, prev_y)) = *last_mouse_pos {
                        let delta_x = (current_pos.0 - prev_x) as f32;
                        let delta_y = (current_pos.1 - prev_y) as f32;

                        // Rotate camera around target with distance-scaled sensitivity and momentum
                        state.camera.rotate_around_target(delta_x, delta_y);
                        window.request_redraw();
                    }
                } else {
                    // Hover detection when mouse is moving without dragging
                    let hovered_uuid = state.pick_station(&ray).map(|s| s.stationuuid.clone());
                    state.set_hovered_station(hovered_uuid);
                    // Mouse lens effector follows cursor across globe surface
                    state.apply_effectors();
                    window.request_redraw();
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
                *mouse_press_pos = None;
                state.mouse_globe_pos = None;
                state.camera.stop_drag();
                state.set_hovered_station(None);
                state.apply_effectors();
                window.request_redraw();
            }
            _ => {}
        }

        match window_event {
            WindowEvent::RedrawRequested => {
                let physical_size = window.inner_size();
                if physical_size.width == 0 || physical_size.height == 0 {
                    return;
                }

                // Smoothly update camera towards target angles/radius with damping
                if state.camera.update() {
                    window.request_redraw();
                }

                // Apply dynamic effectors based on current camera and mouse positions
                state.apply_effectors();

                // Sync hovered / selected station details and playback status to UI overlay state
                ui.state.hovered_station_name = state
                    .hovered_station
                    .as_ref()
                    .and_then(|id| state.find_station(id))
                    .map(|s| s.name.clone());
                ui.state.selected_station = state
                    .selected_station
                    .as_ref()
                    .and_then(|id| state.find_station(id))
                    .cloned();
                ui.state.playback_status = Some(state.playback_status.clone());

                // 1. Update UI overlay and dispatch messages directly to AppState or playback channel
                let ui_result = ui.update(window);
                for message in &ui_result.messages {
                    match message {
                        crate::app::ui::Message::TogglePlayback => {
                            let _ = channels
                                .tx_playback
                                .send(crate::playback::PlaybackMessage::TogglePlayPause);
                        }
                        crate::app::ui::Message::StopPlayback => {
                            let _ = channels
                                .tx_playback
                                .send(crate::playback::PlaybackMessage::Stop);
                        }
                        crate::app::ui::Message::PlayStation { url, name } => {
                            let _ =
                                channels
                                    .tx_playback
                                    .send(crate::playback::PlaybackMessage::Play {
                                        url: url.clone(),
                                        name: name.clone(),
                                    });
                        }
                        _ => {
                            state.handle_message(message);
                        }
                    }
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
