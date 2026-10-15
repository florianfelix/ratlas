use crossbeam_channel::{Receiver, Sender};

#[derive(Debug, Clone)]
pub enum RadioMessage {
    GetPopular,
}

#[derive(Clone)]
pub struct Channels {
    pub tx_radio: Sender<RadioMessage>,
    pub rx_radio: Receiver<RadioMessage>,
    pub tx_playback: Sender<crate::playback::PlaybackMessage>,
    pub rx_playback: Receiver<crate::playback::PlaybackMessage>,
}

impl Default for Channels {
    fn default() -> Self {
        Self::new()
    }
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
    TrayIconEvent(tray_icon::TrayIconEvent),
    MenuEvent(tray_icon::menu::MenuEvent),
}
