use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
#[serde(remote = "radiobrowser::ApiStation")]
struct ApiStationDef {
    pub changeuuid: String,
    pub stationuuid: String,
    pub serveruuid: Option<String>,
    pub name: String,
    pub url: String,
    pub url_resolved: String,
    pub homepage: String,
    pub favicon: String,
    pub tags: String,
    pub country: String,
    pub countrycode: String,
    pub iso_3166_2: Option<String>,
    pub state: String,
    pub language: String,
    pub languagecodes: Option<String>,
    pub votes: i32,
    pub lastchangetime_iso8601: Option<chrono::DateTime<chrono::Utc>>,
    pub codec: String,
    pub bitrate: u32,
    pub hls: i8,
    pub lastcheckok: i8,
    pub lastchecktime_iso8601: Option<chrono::DateTime<chrono::Utc>>,
    pub lastcheckoktime_iso8601: Option<chrono::DateTime<chrono::Utc>>,
    pub lastlocalchecktime_iso8601: Option<chrono::DateTime<chrono::Utc>>,
    pub clicktimestamp_iso8601: Option<chrono::DateTime<chrono::Utc>>,
    pub clickcount: u32,
    pub clicktrend: i32,
    pub ssl_error: Option<u8>,
    pub geo_lat: Option<f64>,
    pub geo_long: Option<f64>,
    pub has_extended_info: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct StationListCache {
    #[serde(with = "station_seq")]
    pub stations: Vec<radiobrowser::ApiStation>,
}

mod station_seq {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(
        stations: &[radiobrowser::ApiStation],
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(stations.len()))?;
        for s in stations {
            seq.serialize_element(&ApiStationDefRef(s))?;
        }
        seq.end()
    }

    struct ApiStationDefRef<'a>(&'a radiobrowser::ApiStation);
    impl<'a> Serialize for ApiStationDefRef<'a> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            ApiStationDef::serialize(self.0, serializer)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<radiobrowser::ApiStation>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wrapper(#[serde(with = "ApiStationDef")] radiobrowser::ApiStation);

        let list = Vec::<Wrapper>::deserialize(deserializer)?;
        Ok(list.into_iter().map(|w| w.0).collect())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "radiobrowser::ApiTag")]
struct ApiTagDef {
    pub name: String,
    pub stationcount: u32,
}

#[derive(Serialize, Deserialize)]
struct TagListCache {
    #[serde(with = "tag_seq")]
    pub tags: Vec<radiobrowser::ApiTag>,
}

mod tag_seq {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(tags: &[radiobrowser::ApiTag], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(tags.len()))?;
        for t in tags {
            seq.serialize_element(&ApiTagDefRef(t))?;
        }
        seq.end()
    }

    struct ApiTagDefRef<'a>(&'a radiobrowser::ApiTag);
    impl<'a> Serialize for ApiTagDefRef<'a> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            ApiTagDef::serialize(self.0, serializer)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<radiobrowser::ApiTag>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wrapper(#[serde(with = "ApiTagDef")] radiobrowser::ApiTag);

        let list = Vec::<Wrapper>::deserialize(deserializer)?;
        Ok(list.into_iter().map(|w| w.0).collect())
    }
}

pub fn get_stations_cache_path() -> Option<PathBuf> {
    ProjectDirs::from("de", "ff", "ratlas")
        .map(|proj_dirs| proj_dirs.data_dir().join("stations_cache.json"))
}

pub fn get_tags_cache_path() -> Option<PathBuf> {
    ProjectDirs::from("de", "ff", "ratlas")
        .map(|proj_dirs| proj_dirs.data_dir().join("tags_cache.json"))
}

pub fn load_cached_stations() -> Option<Vec<radiobrowser::ApiStation>> {
    let cache_path = get_stations_cache_path()?;
    if !cache_path.exists() {
        return None;
    }

    let file = File::open(&cache_path).ok()?;
    let reader = BufReader::new(file);
    match serde_json::from_reader::<_, StationListCache>(reader) {
        Ok(cache) => {
            tracing::info!(
                "Loaded {} stations from cache ({})",
                cache.stations.len(),
                cache_path.display()
            );
            Some(cache.stations)
        }
        Err(e) => {
            tracing::warn!("Failed to deserialize stations cache: {e}");
            None
        }
    }
}

pub fn save_cached_stations(stations: &[radiobrowser::ApiStation]) {
    let Some(cache_path) = get_stations_cache_path() else {
        return;
    };

    if let Some(parent) = cache_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            tracing::error!("Failed to create cache directory: {e}");
            return;
        }
    }

    let tmp_path = cache_path.with_extension("tmp");
    let file = match File::create(&tmp_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Failed to create cache file {}: {e}", tmp_path.display());
            return;
        }
    };

    let writer = BufWriter::new(file);
    let cache = StationListCache {
        stations: stations.to_vec(),
    };

    if let Err(e) = serde_json::to_writer(writer, &cache) {
        tracing::error!("Failed to write stations cache: {e}");
        let _ = fs::remove_file(&tmp_path);
        return;
    }

    if let Err(e) = fs::rename(&tmp_path, &cache_path) {
        tracing::error!(
            "Failed to rename {} to {}: {e}",
            tmp_path.display(),
            cache_path.display()
        );
    } else {
        tracing::info!(
            "Saved {} stations to cache ({})",
            stations.len(),
            cache_path.display()
        );
    }
}

pub fn load_cached_tags() -> Option<Vec<radiobrowser::ApiTag>> {
    let cache_path = get_tags_cache_path()?;
    if !cache_path.exists() {
        return None;
    }

    let file = File::open(&cache_path).ok()?;
    let reader = BufReader::new(file);
    match serde_json::from_reader::<_, TagListCache>(reader) {
        Ok(cache) => {
            tracing::info!(
                "Loaded {} tags from cache ({})",
                cache.tags.len(),
                cache_path.display()
            );
            Some(cache.tags)
        }
        Err(e) => {
            tracing::warn!("Failed to deserialize tags cache: {e}");
            None
        }
    }
}

pub fn save_cached_tags(tags: &[radiobrowser::ApiTag]) {
    let Some(cache_path) = get_tags_cache_path() else {
        return;
    };

    if let Some(parent) = cache_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            tracing::error!("Failed to create cache directory: {e}");
            return;
        }
    }

    let tmp_path = cache_path.with_extension("tmp");
    let file = match File::create(&tmp_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Failed to create cache file {}: {e}", tmp_path.display());
            return;
        }
    };

    let writer = BufWriter::new(file);
    let cache = TagListCache {
        tags: tags.to_vec(),
    };

    if let Err(e) = serde_json::to_writer(writer, &cache) {
        tracing::error!("Failed to write tags cache: {e}");
        let _ = fs::remove_file(&tmp_path);
        return;
    }

    if let Err(e) = fs::rename(&tmp_path, &cache_path) {
        tracing::error!(
            "Failed to rename {} to {}: {e}",
            tmp_path.display(),
            cache_path.display()
        );
    } else {
        tracing::info!(
            "Saved {} tags to cache ({})",
            tags.len(),
            cache_path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_serialization_roundtrip() {
        let dummy = radiobrowser::ApiStation {
            changeuuid: "c1".into(),
            stationuuid: "s1".into(),
            serveruuid: None,
            name: "Test Station".into(),
            url: "http://stream.org".into(),
            url_resolved: "http://stream.org".into(),
            homepage: "http://home.org".into(),
            favicon: "".into(),
            tags: "jazz,blues".into(),
            country: "Germany".into(),
            countrycode: "DE".into(),
            iso_3166_2: None,
            state: "Berlin".into(),
            language: "german".into(),
            languagecodes: None,
            votes: 123,
            lastchangetime_iso8601: None,
            codec: "MP3".into(),
            bitrate: 128,
            hls: 0,
            lastcheckok: 1,
            lastchecktime_iso8601: None,
            lastcheckoktime_iso8601: None,
            lastlocalchecktime_iso8601: None,
            clicktimestamp_iso8601: None,
            clickcount: 456,
            clicktrend: 10,
            ssl_error: Some(0),
            geo_lat: Some(52.52),
            geo_long: Some(13.405),
            has_extended_info: Some(false),
        };

        let list = vec![dummy.clone()];
        let cache = StationListCache { stations: list };
        let json = serde_json::to_string(&cache).expect("serialize cache");
        let decoded: StationListCache = serde_json::from_str(&json).expect("deserialize cache");

        assert_eq!(decoded.stations.len(), 1);
        assert_eq!(decoded.stations[0].name, "Test Station");
        assert_eq!(decoded.stations[0].stationuuid, "s1");
        assert_eq!(decoded.stations[0].geo_lat, Some(52.52));
    }

    #[test]
    fn test_tag_cache_serialization_roundtrip() {
        let dummy = radiobrowser::ApiTag {
            name: "classical".into(),
            stationcount: 4200,
        };

        let list = vec![dummy.clone()];
        let cache = TagListCache { tags: list };
        let json = serde_json::to_string(&cache).expect("serialize tag cache");
        let decoded: TagListCache = serde_json::from_str(&json).expect("deserialize tag cache");

        assert_eq!(decoded.tags.len(), 1);
        assert_eq!(decoded.tags[0].name, "classical");
        assert_eq!(decoded.tags[0].stationcount, 4200);
    }
}
