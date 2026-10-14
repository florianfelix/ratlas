use tokio::runtime::Builder;
use winit::event_loop::EventLoopProxy;

use crate::app::application::{ProxyMessage, RadioMessage};

struct RadioStations {
    rx_radio: crossbeam_channel::Receiver<RadioMessage>,
    proxy: EventLoopProxy<ProxyMessage>,
    #[allow(dead_code)]
    stations: Vec<radiobrowser::ApiStation>,
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
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(radio_stations.run());
}

impl RadioStations {
    async fn run(&mut self) {
        while let Ok(msg) = self.rx_radio.recv() {
            match msg {
                RadioMessage::GetPopular => self.get_popular().await,
            }
        }
    }

    async fn get_popular(&mut self) {
        tracing::info!("Radio API: fetching top 20 popular stations with geolocation...");
        let api_result = radiobrowser::RadioBrowserAPI::new().await;
        let api = match api_result {
            Ok(api) => api,
            Err(e) => {
                tracing::warn!(
                    "RadioBrowserAPI::new() failed ({:?}), falling back to direct server...",
                    e
                );
                match radiobrowser::RadioBrowserAPI::new_from_dns_a("all.api.radio-browser.info")
                    .await
                {
                    Ok(api) => api,
                    Err(e) => {
                        tracing::error!("Failed to initialize RadioBrowserAPI: {:?}", e);
                        return;
                    }
                }
            }
        };

        match api
            .get_stations()
            .has_geo_info(true)
            .order(radiobrowser::StationOrder::Clickcount)
            .reverse(true)
            .limit("20")
            .send()
            .await
        {
            Ok(stations) => {
                tracing::info!("Radio API: retrieved {} stations", stations.len());
                self.stations = stations.clone();
                if let Err(e) = self
                    .proxy
                    .send_event(ProxyMessage::RadioStationsLoaded(stations))
                {
                    tracing::error!("Radio API: failed to send stations to event loop: {:?}", e);
                }
            }
            Err(e) => {
                tracing::error!("Radio API: failed to query stations: {:?}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_fetch_popular_stations_with_geo_info() {
        let api_result = radiobrowser::RadioBrowserAPI::new().await;
        let api = match api_result {
            Ok(api) => api,
            Err(_) => radiobrowser::RadioBrowserAPI::new_from_dns_a("all.api.radio-browser.info")
                .await
                .expect("Failed to init RadioBrowserAPI"),
        };

        let stations = api
            .get_stations()
            .has_geo_info(true)
            .order(radiobrowser::StationOrder::Clickcount)
            .reverse(true)
            .limit("20")
            .send()
            .await
            .expect("Failed to fetch stations");

        assert_eq!(stations.len(), 20);
        for station in &stations {
            assert!(
                station.geo_lat.is_some(),
                "Station {} missing latitude",
                station.name
            );
            assert!(
                station.geo_long.is_some(),
                "Station {} missing longitude",
                station.name
            );
            println!(
                "Station: {:<30} | Lat: {:>8.4} | Lon: {:>8.4} | Clicks: {}",
                station.name,
                station.geo_lat.unwrap(),
                station.geo_long.unwrap(),
                station.clickcount
            );
        }
    }
}
