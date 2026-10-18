use std::sync::Arc;
use winit::window::Window;

use crate::app::app_state::{AppState, EarthConfig, StationFilter, gpu::Gpu};
use iced_winit::program::Program;

pub mod components;
pub mod message;
mod root_view;

pub use message::{Message, handle_message};

#[derive(Debug)]
pub struct UiState {
    pub counter: i32,
    pub volume: f32,
    pub mouse_pos: Option<glam::Vec2>,
    pub hovered_station_full: Option<radiobrowser::ApiStation>,
    pub selected_station: Option<radiobrowser::ApiStation>,
    pub playing_station: Option<radiobrowser::ApiStation>,
    pub playback_status: Option<crate::playback::PlaybackStatus>,
    pub station_limit: crate::app::app_state::StationLimit,
    pub displayed_stations_count: usize,
    pub total_stations_count: usize,
    pub station_filter: StationFilter,
    pub tag_search_query: String,
    pub api_tags: Vec<radiobrowser::ApiTag>,
    pub earth_config: EarthConfig,
    pub active_theme: iced::Theme,
}

pub type Element<'a> = iced_wgpu::core::Element<'a, Message, iced::Theme, iced_wgpu::Renderer>;

pub struct UiUpdateResult {
    pub viewport: iced_wgpu::graphics::Viewport,
    #[allow(dead_code)]
    pub event_statuses: Vec<iced_wgpu::core::event::Status>,
    pub messages: Vec<Message>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            counter: 0,
            volume: 1.0,
            mouse_pos: None,
            hovered_station_full: None,
            selected_station: None,
            playing_station: None,
            playback_status: None,
            station_limit: crate::app::app_state::StationLimit::default(),
            displayed_stations_count: 0,
            total_stations_count: 0,
            station_filter: StationFilter::default(),
            tag_search_query: String::new(),
            api_tags: Vec::new(),
            earth_config: EarthConfig::default(),
            active_theme: iced::Theme::Dark,
        }
    }
}

pub struct Ui {}

impl iced_winit::program::Program for Ui {
    type State = UiState;
    type Message = Message;
    type Theme = iced::Theme;
    type Renderer = iced_wgpu::Renderer;
    type Executor = iced_winit::futures::backend::null::Executor;

    #[tracing::instrument(skip_all, name = "ProgramUpdate")]
    fn update(&self, state: &mut Self::State, message: Self::Message) -> iced::Task<Self::Message> {
        handle_message(state, message)
    }

    fn name() -> &'static str {
        "Ratlas"
    }

    fn theme(
        &self,
        state: &Self::State,
        _window: iced_wgpu::core::window::Id,
    ) -> Option<Self::Theme> {
        Some(state.active_theme.clone())
    }
    fn settings(&self) -> iced::Settings {
        iced::Settings::default()
    }

    fn boot(&self) -> (Self::State, iced::Task<Self::Message>) {
        (UiState::default(), iced::Task::none())
    }

    fn window(&self) -> Option<iced_wgpu::core::window::Settings> {
        None
    }

    fn view<'a>(
        &self,
        state: &'a Self::State,
        _window: iced_wgpu::core::window::Id,
    ) -> iced_wgpu::core::Element<'a, Self::Message, Self::Theme, Self::Renderer> {
        self.root_view(state)
    }
}

pub struct IcedUi {
    pub window_id: iced::window::Id,
    pub program: Ui,
    pub state: UiState,
    pub renderer: iced_wgpu::Renderer,
    pub cache: iced_runtime::user_interface::Cache,
    pub cursor_position: iced_wgpu::core::mouse::Cursor,
    pub modifiers: winit::keyboard::ModifiersState,
    pub clipboard: iced_winit::clipboard::Clipboard,
    pub events: Vec<iced_wgpu::core::Event>,
    pub pending_messages: Vec<Message>,
}

impl IcedUi {
    #[tracing::instrument(skip_all)]
    pub fn new(window: Arc<Window>, gpu: &Gpu) -> Self {
        let program = Ui {};
        let (state, _task) = program.boot();
        let renderer = gpu.create_iced_renderer();
        let clipboard = iced_winit::clipboard::Clipboard::connect(window);
        let window_id = iced::window::Id::unique();

        Self {
            window_id,
            program,
            state,
            renderer,
            cache: iced_runtime::user_interface::Cache::default(),
            cursor_position: iced_wgpu::core::mouse::Cursor::Unavailable,
            modifiers: winit::keyboard::ModifiersState::default(),
            clipboard,
            events: Vec::new(),
            pending_messages: Vec::new(),
        }
    }

    /// Disconnects the UI clipboard from the window, releasing the Arc<Window> reference.
    pub fn disconnect_window(&mut self) {
        self.clipboard = iced_winit::clipboard::Clipboard::unconnected();
    }

    /// Reconnects the UI clipboard and window state to a newly restored window
    pub fn reconnect_window(&mut self, window: Arc<Window>) {
        self.window_id = iced::window::Id::unique();
        self.clipboard = iced_winit::clipboard::Clipboard::connect(window);
        self.cache = iced_runtime::user_interface::Cache::default();
    }

    /// Syncs relevant presentation state from AppState into UiState before rendering
    pub fn apply_appstate(&mut self, state: &AppState) {
        self.state.hovered_station_full = state
            .hovered_station
            .as_ref()
            .and_then(|id| state.find_station(id))
            .cloned();
        self.state.selected_station = state
            .selected_station
            .as_ref()
            .and_then(|id| state.find_station(id))
            .cloned();
        self.state.playing_station = state.playing_station().cloned();
        self.state.playback_status = Some(state.playback_status.clone());
        self.state.station_limit = state.station_limit;
        self.state.station_filter = state.station_filter.clone();
        self.state.displayed_stations_count = state.radio_stations.len();
        self.state.total_stations_count = state.all_radio_stations.len();
        self.state.earth_config = state.earth_config.clone();
        if self.state.api_tags.len() != state.api_tags.len() {
            self.state.api_tags = state.api_tags.clone();
        }
        self.state.mouse_pos = state.mouse_pos;
        self.state.active_theme = state.active_theme.clone();
    }

    #[tracing::instrument(skip_all)]
    pub fn handle_event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> bool {
        match event {
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                let logical_pos = position.to_logical::<f64>(window.scale_factor());
                self.cursor_position = iced_wgpu::core::mouse::Cursor::Available(
                    iced_wgpu::core::Point::new(logical_pos.x as f32, logical_pos.y as f32),
                );
            }
            winit::event::WindowEvent::CursorLeft { .. } => {
                self.cursor_position = iced_wgpu::core::mouse::Cursor::Unavailable;
            }
            winit::event::WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            _ => {}
        }

        let Some(iced_event) = iced_winit::conversion::window_event(
            event.clone(),
            window.scale_factor() as f32,
            self.modifiers,
        ) else {
            return false;
        };

        let physical_size = window.inner_size();
        let viewport = iced_wgpu::graphics::Viewport::with_physical_size(
            iced_wgpu::core::Size::new(physical_size.width, physical_size.height),
            window.scale_factor() as f32,
        );

        let mut user_interface = iced_runtime::UserInterface::build(
            self.program.view(&self.state, self.window_id),
            viewport.logical_size(),
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );

        let mut messages = Vec::new();
        let (state, event_statuses) = user_interface.update(
            &[iced_event],
            self.cursor_position,
            &mut self.renderer,
            &mut self.clipboard,
            &mut messages,
        );

        let is_captured = event_statuses
            .iter()
            .any(|s| matches!(s, iced_wgpu::core::event::Status::Captured));

        let is_over_interactive = match state {
            iced_runtime::user_interface::State::Updated {
                mouse_interaction, ..
            } => mouse_interaction != iced_wgpu::core::mouse::Interaction::None,
            _ => false,
        };

        // Also check if the cursor is within the top toolbar or inspector bounds
        let is_over_ui_bounds = if let iced_wgpu::core::mouse::Cursor::Available(pos) =
            self.cursor_position
        {
            let width = physical_size.width as f32 / window.scale_factor() as f32;
            let is_top_bar = if self.state.tag_search_query.trim().is_empty() {
                pos.y <= 48.0
            } else {
                pos.y <= 85.0
            };
            let is_inspector = self.state.selected_station.is_some() && pos.x >= (width - 215.0);
            let is_hud = (self.state.playback_status.is_some()
                || self.state.hovered_station_full.is_some())
                && pos.x <= 230.0
                && pos.y > 48.0
                && pos.y <= 180.0;
            is_top_bar || is_inspector || is_hud
        } else {
            false
        };

        self.cache = user_interface.into_cache();

        for message in &messages {
            let _ = self.program.update(&mut self.state, message.clone());
        }
        self.pending_messages.extend(messages);

        is_captured || is_over_interactive || is_over_ui_bounds
    }

    /// Pure update pass: advances the Iced UI interface, draining all UI interaction messages
    /// and returning the calculated viewport together with the list of produced messages.
    /// Does not touch winit Window or external system channels.
    pub fn update(&mut self, window: &Window) -> UiUpdateResult {
        let physical_size = window.inner_size();
        let viewport = iced_wgpu::graphics::Viewport::with_physical_size(
            iced_wgpu::core::Size::new(physical_size.width, physical_size.height),
            window.scale_factor() as f32,
        );

        let mut user_interface = iced_runtime::UserInterface::build(
            self.program.view(&self.state, self.window_id),
            viewport.logical_size(),
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );

        let mut messages = std::mem::take(&mut self.pending_messages);
        let (_state, event_statuses) = user_interface.update(
            &self.events,
            self.cursor_position,
            &mut self.renderer,
            &mut self.clipboard,
            &mut messages,
        );
        self.events.clear();

        if !messages.is_empty() {
            self.cache = user_interface.into_cache();
            for message in &messages {
                let _task = self.program.update(&mut self.state, message.clone());
            }
            user_interface = iced_runtime::UserInterface::build(
                self.program.view(&self.state, self.window_id),
                viewport.logical_size(),
                std::mem::take(&mut self.cache),
                &mut self.renderer,
            );
        }

        let theme = self
            .program
            .theme(&self.state, self.window_id)
            .unwrap_or(iced::Theme::Dark);

        user_interface.draw(
            &mut self.renderer,
            &theme,
            &iced_wgpu::core::renderer::Style {
                text_color: theme.palette().text,
            },
            self.cursor_position,
        );

        self.cache = user_interface.into_cache();

        UiUpdateResult {
            viewport,
            event_statuses,
            messages,
        }
    }

    #[tracing::instrument(skip_all)]
    pub fn draw(
        &mut self,
        surface_format: iced_wgpu::wgpu::TextureFormat,
        view: &iced_wgpu::wgpu::TextureView,
        viewport: &iced_wgpu::graphics::Viewport,
    ) {
        self.renderer.present(
            None, // transparent overlay on top of WGPU scene
            surface_format,
            view,
            viewport,
        );
    }
}
