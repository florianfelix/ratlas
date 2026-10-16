use radiobrowser::RadioBrowserAPI;
use tokio::runtime::Builder;
use winit::event_loop::EventLoopProxy;

use crate::app::application::{ProxyMessage, RadioMessage};

/// Filters out HLS (.m3u8) stations that require segmented chunk streaming,
/// ensuring only direct audio streams (MP3, AAC, OGG, FLAC, M3U/PLS redirects)
/// are presented to the user on the globe.
pub fn filter_supported_stations(
    stations: Vec<radiobrowser::ApiStation>,
) -> Vec<radiobrowser::ApiStation> {
    stations
        .into_iter()
        .filter(|s| {
            if s.hls == 1 {
                return false;
            }
            let url_lower = s.url.to_lowercase();
            let url_res_lower = s.url_resolved.to_lowercase();
            if url_lower.contains(".m3u8") || url_res_lower.contains(".m3u8") {
                return false;
            }
            true
        })
        .collect()
}

struct RadioStations {
    rx_radio: crossbeam_channel::Receiver<RadioMessage>,
    proxy: EventLoopProxy<ProxyMessage>,
    #[allow(dead_code)]
    stations: Vec<radiobrowser::ApiStation>,
    api: RadioBrowserAPI,
}

fn create_api() -> RadioBrowserAPI {
    pollster::block_on(async {
        let api_result = radiobrowser::RadioBrowserAPI::new().await;

        match api_result {
            Ok(api) => api,
            Err(e) => {
                tracing::warn!(
                    "RadioBrowserAPI::new() failed ({:?}), falling back to direct server...",
                    e
                );

                match radiobrowser::RadioBrowserAPI::new_from_dns_a("all.api.radio-browser.info")
                    .await
                {
                    Ok(api) => Ok(api),
                    Err(e) => {
                        tracing::error!("Failed to initialize RadioBrowserAPI: {:?}", e);
                        Err(anyhow::anyhow!(
                            "Failed to initialize RadioBrowserAPI: {:?}",
                            e
                        ))
                    }
                }
                .expect("useless without api")
            }
        }
    })
}

pub fn radio_api_worker(
    rx_radio: crossbeam_channel::Receiver<RadioMessage>,
    proxy: EventLoopProxy<ProxyMessage>,
) {
    let api = create_api();
    let mut radio_stations = RadioStations {
        rx_radio,
        proxy,
        stations: vec![],
        api,
    };

    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("radio_api_worker")
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(radio_stations.run());
}

type ApiResultStations =
    Result<Vec<radiobrowser::ApiStation>, Box<dyn std::error::Error + 'static>>;
type ApiResultTags = Result<Vec<radiobrowser::ApiTag>, Box<dyn std::error::Error + 'static>>;

impl RadioStations {
    async fn run(&mut self) {
        while let Ok(msg) = self.rx_radio.recv() {
            match msg {
                RadioMessage::GetPopular => self.get_popular().await,
                RadioMessage::GetTags => self.get_tags().await,
            }
        }
    }

    fn send_station_result(&self, api_result: ApiResultStations) {
        match api_result {
            Err(e) => {
                tracing::error!("Radio API: failed to query stations: {:?}", e);
            }
            Ok(stations) => {
                let total = stations.len();
                let supported = filter_supported_stations(stations);

                tracing::info!(
                    "Radio API: retrieved {} stations ({} playable direct streams after filtering HLS)",
                    total,
                    supported.len()
                );

                if let Err(e) = self
                    .proxy
                    .send_event(ProxyMessage::RadioStationsLoaded(supported))
                {
                    tracing::error!("Radio API: failed to send stations to event loop: {:?}", e);
                }
            }
        }
    }

    fn send_tag_result(&self, api_result: ApiResultTags) {
        match api_result {
            Err(e) => {
                tracing::error!("Radio API: failed to query stations: {:?}", e);
            }
            Ok(tags) => {
                tracing::info!("Radio API: retrieved {} tags", tags.len());
                if let Err(e) = self.proxy.send_event(ProxyMessage::RadioStationTags(tags)) {
                    tracing::error!("Radio API: failed to send ApiTags to event loop: {:?}", e);
                }
            }
        }
    }

    #[tracing::instrument(skip_all)]
    async fn get_popular(&mut self) {
        tracing::info!("Radio API: fetching popular stations with geolocation...");

        let api_result = self
            .api
            .get_stations()
            .has_geo_info(true)
            .hidebroken(true)
            .order(radiobrowser::StationOrder::Clickcount)
            .reverse(true)
            .limit("100000")
            .send()
            .await;
        self.send_station_result(api_result);
    }

    #[tracing::instrument(skip_all)]
    async fn get_tags(&mut self) {
        tracing::info!("Radio API: fetching tags");

        let tag_result = self
            .api
            .get_tags()
            .hidebroken(true)
            .limit("100000")
            .send()
            .await;
        self.send_tag_result(tag_result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_supported_stations() {
        let s1 = create_dummy_station("Direct MP3", "http://stream.org/live.mp3", 0);
        let s2 = create_dummy_station("HLS Stream Flag", "http://stream.org/live", 1);
        let s3 = create_dummy_station("HLS Stream M3U8", "http://stream.org/live.m3u8", 0);
        let s4 = create_dummy_station("M3U Playlist", "http://stream.org/live.m3u", 0);

        let filtered = filter_supported_stations(vec![s1.clone(), s2, s3, s4.clone()]);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].name, "Direct MP3");
        assert_eq!(filtered[1].name, "M3U Playlist");
    }

    fn create_dummy_station(name: &str, url: &str, hls: i8) -> radiobrowser::ApiStation {
        radiobrowser::ApiStation {
            changeuuid: "".to_string(),
            stationuuid: "".to_string(),
            serveruuid: None,
            name: name.to_string(),
            url: url.to_string(),
            url_resolved: url.to_string(),
            homepage: "".to_string(),
            favicon: "".to_string(),
            tags: "".to_string(),
            country: "".to_string(),
            countrycode: "".to_string(),
            iso_3166_2: None,
            state: "".to_string(),
            language: "".to_string(),
            languagecodes: None,
            votes: 0,
            lastchangetime_iso8601: None,
            codec: "MP3".to_string(),
            bitrate: 128,
            hls,
            lastcheckok: 1,
            lastchecktime_iso8601: None,
            lastcheckoktime_iso8601: None,
            lastlocalchecktime_iso8601: None,
            clicktimestamp_iso8601: None,
            clickcount: 10,
            clicktrend: 0,
            ssl_error: Some(0),
            geo_lat: Some(10.0),
            geo_long: Some(20.0),
            has_extended_info: None,
        }
    }

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
            .hidebroken(true)
            .order(radiobrowser::StationOrder::Clickcount)
            .reverse(true)
            .limit("20")
            .send()
            .await
            .expect("Failed to fetch stations");

        let supported = filter_supported_stations(stations);
        assert!(!supported.is_empty());
        for station in &supported {
            assert_ne!(station.hls, 1, "HLS station should have been filtered out");
            assert!(
                !station.url.to_lowercase().contains(".m3u8"),
                "M3U8 url should have been filtered out"
            );
        }
    }
}
