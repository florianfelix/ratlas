#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackStatus {
    Stopped,
    Loading { url: String, name: String },
    Playing { url: String, name: String },
    Paused { url: String, name: String },
    Error { message: String },
}

#[derive(Debug, Clone)]
pub enum PlaybackMessage {
    Play { url: String, name: String },
    Stop,
    Pause,
    Resume,
    TogglePlayPause,
    SetVolume(f32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playback_status_transitions() {
        let status = PlaybackStatus::Stopped;
        assert_eq!(status, PlaybackStatus::Stopped);

        let loading = PlaybackStatus::Loading {
            url: "http://stream.org".to_string(),
            name: "Radio 1".to_string(),
        };
        assert_ne!(status, loading);
    }
}
