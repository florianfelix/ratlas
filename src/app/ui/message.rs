use iced::Task;
use tracing::info;

use super::UiState;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Message {
    ButtonClicked,
    ResetCamera,
    TogglePlayback,
    StopPlayback,
    PlayStation { url: String, name: String },
}

/// Handles UI state transitions driven by Iced messages
#[tracing::instrument(skip_all, name = "handle_message")]
pub fn handle_message(state: &mut UiState, message: Message) -> Task<Message> {
    info!("Handling UI Message: {:?}", message);

    match message {
        Message::ButtonClicked => {
            state.counter += 1;
        }
        Message::ResetCamera => {
            // Can trigger camera reset or set dirty flags
        }
        Message::TogglePlayback | Message::StopPlayback | Message::PlayStation { .. } => {
            // Application-level worker handles playback messages
        }
    }

    Task::none()
}
