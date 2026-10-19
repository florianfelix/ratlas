use crate::app::app_state::AppState;

impl AppState {
    pub fn set_station_limit(&mut self, limit: StationLimit) {
        if self.station_limit != limit {
            self.station_limit = limit;
            self.rebuild_station_markers();
        }
    }

    pub fn set_station_filter(&mut self, filter: StationFilter) {
        if self.station_filter != filter {
            let country_changed = self.station_filter.country_code != filter.country_code;
            let has_new_country = filter.country_code.is_some();
            self.station_filter = filter;
            self.rebuild_station_markers();

            if country_changed && has_new_country {
                self.focus_on_country();
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagMatchMode {
    #[default]
    And,
    Or,
}

impl std::fmt::Display for TagMatchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagMatchMode::And => write!(f, "AND"),
            TagMatchMode::Or => write!(f, "OR"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationFilter {
    pub country_code: Option<isocountry::CountryCode>,
    pub tags: Vec<String>,
    pub tag_match_mode: TagMatchMode,
}

impl Default for StationFilter {
    fn default() -> Self {
        Self {
            country_code: None,
            tags: vec![],
            tag_match_mode: TagMatchMode::default(),
        }
    }
}

impl StationFilter {
    pub fn matches(&self, station: &radiobrowser::ApiStation) -> bool {
        if let Some(ref cc) = self.country_code {
            if !station.countrycode.eq_ignore_ascii_case(cc.alpha2()) {
                return false;
            }
        }
        if !self.tags.is_empty() {
            let mut tag_matches = self.tags.iter().map(|tag| {
                station
                    .tags
                    .split(',')
                    .any(|t| t.trim().eq_ignore_ascii_case(tag))
            });
            let matched = match self.tag_match_mode {
                TagMatchMode::And => tag_matches.all(|m| m),
                TagMatchMode::Or => tag_matches.any(|m| m),
            };
            if !matched {
                return false;
            }
        }

        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StationLimit {
    Limit500,
    #[default]
    Limit1000,
    Limit2500,
    Limit5000,
    All,
}

impl StationLimit {
    pub const ALL: [StationLimit; 5] = [
        StationLimit::Limit500,
        StationLimit::Limit1000,
        StationLimit::Limit2500,
        StationLimit::Limit5000,
        StationLimit::All,
    ];

    pub fn to_count(self) -> Option<usize> {
        match self {
            StationLimit::Limit500 => Some(500),
            StationLimit::Limit1000 => Some(1000),
            StationLimit::Limit2500 => Some(2500),
            StationLimit::Limit5000 => Some(5000),
            StationLimit::All => None,
        }
    }
}

impl std::fmt::Display for StationLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StationLimit::Limit500 => write!(f, "500"),
            StationLimit::Limit1000 => write!(f, "1,000"),
            StationLimit::Limit2500 => write!(f, "2,500"),
            StationLimit::Limit5000 => write!(f, "5,000"),
            StationLimit::All => write!(f, "All"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::app_state::dummy_station;
    use crate::app::ui::Message;

    #[test]
    fn test_tag_search_and_matching() {
        let mut station = dummy_station("1", "Rock Station");
        station.tags = "classic rock, 80s, rock".to_string();

        let mut filter = StationFilter::default();
        filter.tags = vec!["rock".to_string()];
        assert!(filter.matches(&station));

        filter.tags = vec!["jazz".to_string()];
        assert!(!filter.matches(&station));

        // Multi-tag matching AND vs OR
        filter.tags = vec!["rock".to_string(), "jazz".to_string()];
        filter.tag_match_mode = TagMatchMode::And;
        assert!(!filter.matches(&station));

        filter.tag_match_mode = TagMatchMode::Or;
        assert!(filter.matches(&station));
    }

    #[test]
    fn test_tag_match_mode_toggle_message() {
        let mut state = AppState::default();
        assert_eq!(state.station_filter.tag_match_mode, TagMatchMode::And);

        state.handle_message(&Message::ToggleTagMatchMode);
        assert_eq!(state.station_filter.tag_match_mode, TagMatchMode::Or);

        state.handle_message(&Message::ToggleTagMatchMode);
        assert_eq!(state.station_filter.tag_match_mode, TagMatchMode::And);
    }
}
