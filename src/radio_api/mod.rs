pub mod cache;

use radiobrowser::RadioBrowserAPI;
use std::sync::Arc;
use tokio::runtime::Builder;
use winit::event_loop::EventLoopProxy;

use crate::app::application::{ProxyMessage, RadioMessage};

/// Standard User-Agent header sent to the Radio-Browser API and streaming servers.
/// Radio-Browser requests client applications send a speaking HTTP user agent string (e.g. app/version).
pub const APP_USER_AGENT: &str = concat!("ratlas/", env!("CARGO_PKG_VERSION"));

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
    http_client: reqwest::Client,
    servers: Arc<[String]>,
}

async fn init_api_and_servers() -> (RadioBrowserAPI, Vec<String>) {
    let servers = match RadioBrowserAPI::get_default_servers().await {
        Ok(s) if !s.is_empty() => s,
        _ => vec![
            "all.api.radio-browser.info".to_string(),
            "de1.api.radio-browser.info".to_string(),
        ],
    };

    let api_result = radiobrowser::RadioBrowserAPI::new().await;
    let api = match api_result {
        Ok(api) => api,
        Err(e) => {
            tracing::warn!(
                "RadioBrowserAPI::new() failed ({:?}), falling back to direct server...",
                e
            );

            match radiobrowser::RadioBrowserAPI::new_from_dns_a("all.api.radio-browser.info").await
            {
                Ok(api) => api,
                Err(e) => {
                    tracing::error!("Failed to initialize RadioBrowserAPI: {:?}", e);
                    panic!("useless without api: {:?}", e);
                }
            }
        }
    };

    (api, servers)
}

/// Sends a /json/url/{stationuuid} request to Radio-Browser to record a user click/play
/// and update popularity statistics in the directory database using our speaking User-Agent.
async fn record_station_click(
    client: &reqwest::Client,
    servers: Arc<[String]>,
    stationuuid: String,
) {
    if stationuuid.is_empty() {
        return;
    }

    tracing::info!(
        "Radio API: recording click for station {} with agent '{}'",
        stationuuid,
        APP_USER_AGENT
    );

    for server in servers.iter() {
        let url = format!("https://{}/json/url/{}", server, stationuuid);
        match client.post(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<radiobrowser::ApiStationClickResult>().await {
                    Ok(res) => {
                        tracing::info!(
                            "Radio API: click recorded for station {} (ok: {}, msg: '{}')",
                            stationuuid,
                            res.ok,
                            res.message
                        );
                    }
                    Err(_) => {
                        tracing::debug!(
                            "Radio API: click recorded for station {} (status 200)",
                            stationuuid
                        );
                    }
                }
                return;
            }
            Ok(resp) => {
                tracing::warn!(
                    "Radio API: server {} returned status {} for click {}",
                    server,
                    resp.status(),
                    stationuuid
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Radio API: failed to record click on {} for {}: {:?}",
                    server,
                    stationuuid,
                    e
                );
            }
        }
    }
}

pub fn radio_api_worker(
    rx_radio: crossbeam_channel::Receiver<RadioMessage>,
    proxy: EventLoopProxy<ProxyMessage>,
) {
    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("radio_api_worker")
        .enable_all()
        .build()
        .unwrap();

    let (api, servers) = runtime.block_on(init_api_and_servers());
    let http_client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let mut radio_stations = RadioStations {
        rx_radio,
        proxy,
        stations: vec![],
        api,
        http_client,
        servers: Arc::from(servers),
    };

    runtime.block_on(radio_stations.run());
}

type ApiResultStations =
    Result<Vec<radiobrowser::ApiStation>, Box<dyn std::error::Error + 'static>>;
type ApiResultTags = Result<Vec<radiobrowser::ApiTag>, Box<dyn std::error::Error + 'static>>;

impl RadioStations {
    async fn run(&mut self) {
        while let Ok(msg) = self.rx_radio.recv() {
            match msg {
                RadioMessage::GetPopular => self.get_popular(false).await,
                RadioMessage::ForceRefresh => {
                    self.get_popular(true).await;
                    self.get_tags(true).await;
                }
                RadioMessage::GetTags => self.get_tags(false).await,
                RadioMessage::RecordStationClick(uuid) => {
                    let client = self.http_client.clone();
                    let servers = self.servers.clone();
                    tokio::spawn(async move {
                        record_station_click(&client, servers, uuid).await;
                    });
                }
            }
        }
    }

    fn send_station_result(&self, api_result: ApiResultStations, save_to_cache: bool) {
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

                if save_to_cache {
                    cache::save_cached_stations(&supported);
                }

                if let Err(e) = self
                    .proxy
                    .send_event(ProxyMessage::RadioStationsLoaded(supported))
                {
                    tracing::error!("Radio API: failed to send stations to event loop: {:?}", e);
                }
            }
        }
    }

    fn send_tag_result(&self, api_result: ApiResultTags, save_to_cache: bool) {
        match api_result {
            Err(e) => {
                tracing::error!("Radio API: failed to query tags: {:?}", e);
            }
            Ok(tags) => {
                tracing::info!("Radio API: retrieved {} tags", tags.len());
                if save_to_cache {
                    cache::save_cached_tags(&tags);
                }
                if let Err(e) = self.proxy.send_event(ProxyMessage::RadioStationTags(tags)) {
                    tracing::error!("Radio API: failed to send ApiTags to event loop: {:?}", e);
                }
            }
        }
    }

    #[tracing::instrument(skip_all)]
    async fn get_popular(&mut self, force_refresh: bool) {
        if !force_refresh {
            if let Some(cached) = cache::load_cached_stations() {
                tracing::info!("Using {} cached stations", cached.len());
                if let Err(e) = self
                    .proxy
                    .send_event(ProxyMessage::RadioStationsLoaded(cached))
                {
                    tracing::error!("Radio API: failed to send cached stations: {:?}", e);
                }
                return;
            }
        }

        tracing::info!("Radio API: fetching popular stations from network with geolocation...");

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
        self.send_station_result(api_result, true);
    }

    #[tracing::instrument(skip_all)]
    async fn get_tags(&mut self, force_refresh: bool) {
        if !force_refresh {
            if let Some(cached) = cache::load_cached_tags() {
                tracing::info!("Using {} cached tags", cached.len());
                if let Err(e) = self
                    .proxy
                    .send_event(ProxyMessage::RadioStationTags(cached))
                {
                    tracing::error!("Radio API: failed to send cached tags: {:?}", e);
                }
                return;
            }
        }

        tracing::info!("Radio API: fetching tags from network");

        let tag_result = self
            .api
            .get_tags()
            .hidebroken(true)
            .limit("100000")
            .send()
            .await;
        self.send_tag_result(tag_result, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_user_agent_format() {
        assert!(APP_USER_AGENT.starts_with("ratlas/"));
        assert_eq!(
            APP_USER_AGENT,
            concat!("ratlas/", env!("CARGO_PKG_VERSION"))
        );
    }

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
                .expect("Failed to connect to RadioBrowser via DNS fallback"),
        };

        let result = api
            .get_stations()
            .has_geo_info(true)
            .hidebroken(true)
            .order(radiobrowser::StationOrder::Clickcount)
            .reverse(true)
            .limit("5")
            .send()
            .await;

        assert!(result.is_ok());
        let stations = result.unwrap();
        assert!(!stations.is_empty());
        for station in stations {
            assert!(station.geo_lat.is_some());
            assert!(station.geo_long.is_some());
        }
    }
}
