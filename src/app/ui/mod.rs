use tracing::info;

#[derive(Debug, Clone)]
pub enum Message {
    ButtonClicked,
}

pub struct Ui {}

impl Default for Ui {
    fn default() -> Self {
        Self {}
    }
}

impl iced_winit::program::Program for Ui {
    type State = ();
    type Message = Message;
    type Theme = iced::Theme;
    type Renderer = iced_wgpu::Renderer;
    type Executor = iced_winit::futures::backend::null::Executor;

    fn update(
        &self,
        _state: &mut Self::State,
        message: Self::Message,
    ) -> iced::Task<Self::Message> {
        info!("Iced Message {:?}", message);
        iced::Task::none()
    }

    fn name() -> &'static str {
        "Ratlas"
    }

    fn settings(&self) -> iced::Settings {
        iced::Settings::default()
    }

    fn boot(&self) -> (Self::State, iced::Task<Self::Message>) {
        ((), iced::Task::none())
    }

    fn window(&self) -> Option<iced_wgpu::core::window::Settings> {
        None
    }

    fn view<'a>(
        &self,
        _state: &'a Self::State,
        _window: iced_wgpu::core::window::Id,
    ) -> iced_wgpu::core::Element<'a, Self::Message, Self::Theme, Self::Renderer> {
        use iced::widget::{button, column, container, text};

        let content = column![
            text("Iced + WGPU Viewport Integration").size(24),
            button("click me").on_press(Message::ButtonClicked),
        ]
        .align_x(iced::Alignment::Center)
        .spacing(20);

        container(content).center(iced::Length::Fill).into()
    }
}
