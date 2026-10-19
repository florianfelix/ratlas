use super::tags::TagMatchMode;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::app_state::dummy_station;

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
}
