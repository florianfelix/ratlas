use tokio::runtime::Builder;
use winit::event_loop::EventLoopProxy;

use crate::app::application::{ProxyMessage, RadioMessage};

struct RadioStations {
    rx_radio: crossbeam_channel::Receiver<RadioMessage>,
    proxy: EventLoopProxy<ProxyMessage>,
    stations: Vec<serde_json::Value>,
}

pub fn radio_api_worker(
    rx_radio: crossbeam_channel::Receiver<RadioMessage>,
    proxy: EventLoopProxy<ProxyMessage>,
) {
    let mut radio_stations = RadioStations {
        rx_radio,
        proxy,
        stations: vec![],
    };

    let runtime = Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("radio_api_worker")
        .build()
        .unwrap();
    runtime.block_on(radio_stations.run());
}

impl RadioStations {
    async fn run(&mut self) {
        while let Ok(msg) = self.rx_radio.recv() {
            match msg {
                RadioMessage::GetPopular => todo!(),
            }
        }
    }

    async fn get_popular(&mut self) {}
}
