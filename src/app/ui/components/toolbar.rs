use crate::app::{
    app_state::{StationLimit, TagMatchMode},
    ui::{Element, Message, UiState},
};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
#[allow(unused)]
use iced::{
    Length,
    widget::{Space, button, column, container, pick_list, row, text, text_input},
};

/// Formats a positive integer with thousands separators (e.g. 12450 -> "12,450")
pub fn format_count(count: u32) -> String {
    let s = count.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(*c);
    }
    result
}

/// Fuzzy matches a query string against the list of API tags using Skim fuzzy matching,
/// ordered by fuzzy score descending and popularity (station count) descending.
pub fn search_tags<'a>(
    query: &str,
    tags: &'a [radiobrowser::ApiTag],
    active_tags: &[String],
    max_results: usize,
) -> Vec<(&'a radiobrowser::ApiTag, i64)> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let matcher = SkimMatcherV2::default();
    let query_lower = trimmed.to_lowercase();
    let first_char = query_lower.chars().next();

    let mut matches: Vec<(&'a radiobrowser::ApiTag, i64)> = tags
        .iter()
        .filter(|tag| {
            if tag.stationcount == 0 {
                return false;
            }
            if active_tags
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&tag.name))
            {
                return false;
            }
            if let Some(ch) = first_char {
                if !tag.name.to_lowercase().contains(ch) {
                    return false;
                }
            }
            true
        })
        .filter_map(|tag| {
            matcher
                .fuzzy_match(&tag.name, trimmed)
                .map(|score| (tag, score))
        })
        .collect();

    matches.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.0.stationcount.cmp(&a.0.stationcount))
    });

    matches.truncate(max_results);
    matches
}

fn wdgt_station_limit<'a>(state: &'a UiState) -> Element<'a> {
    pick_list(
        &StationLimit::ALL[..],
        Some(state.station_limit),
        Message::SetStationLimit,
    )
    .text_size(12)
    .padding([3, 8])
    .into()
}

fn wdgt_country_select<'a>(state: &'a UiState) -> Element<'a> {
    let mut wdgt = row![
        pick_list(
            isocountry::CountryCode::as_array().as_slice(),
            state.station_filter.country_code,
            {
                let filter = state.station_filter.clone();
                move |selected| {
                    let mut f = filter.clone();
                    f.country_code = Some(selected);
                    Message::StationFilterChanged(f)
                }
            },
        )
        .placeholder("All Countries")
        .text_size(12)
        .padding([3, 8])
        .width(200.0),
    ];
    if state.station_filter.country_code.is_some() {
        let mut clear_filter = state.station_filter.clone();
        clear_filter.country_code = None;
        wdgt = wdgt.push(
            button(text("✕").size(10).align_y(iced::Alignment::Center))
                .padding([3, 6])
                .on_press(Message::StationFilterChanged(clear_filter)),
        );
    }

    wdgt.into()
}

fn wdgt_display_station_count<'a>(state: &'a UiState) -> Element<'a> {
    let count_str = format!(
        "({} / {})",
        state.displayed_stations_count, state.total_stations_count
    );
    text(count_str)
        .size(11)
        .align_y(iced::Alignment::Center)
        .into()
}

pub fn wdgt_tag_selected<'a>(state: &'a UiState) -> Element<'a> {
    let mut wdgt = column![].spacing(8.0);

    if !state.station_filter.tags.is_empty() {
        let and_btn = button(text("AND").size(10).align_y(iced::Alignment::Center))
            .padding([2, 5])
            .style(
                if state.station_filter.tag_match_mode == TagMatchMode::And {
                    button::primary
                } else {
                    button::secondary
                },
            )
            .on_press(Message::SetTagMatchMode(TagMatchMode::And));

        let or_btn = button(text("OR").size(10).align_y(iced::Alignment::Center))
            .padding([2, 5])
            .style(if state.station_filter.tag_match_mode == TagMatchMode::Or {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::SetTagMatchMode(TagMatchMode::Or));

        let mode_group = row![and_btn, or_btn]
            .spacing(10)
            .align_y(iced::Alignment::Center);

        wdgt = wdgt.push(mode_group);
    }

    let mut chips = row![].spacing(4.0);
    for tag in &state.station_filter.tags {
        let chip = button(
            row![
                text(tag.clone()).size(11).align_y(iced::Alignment::Center),
                text("✕").size(9).align_y(iced::Alignment::Center),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
        )
        .padding([2, 6])
        .on_press(Message::RemoveTagFilter(tag.clone()));

        chips = chips.push(chip);
    }
    wdgt = wdgt.push(chips.wrap());

    wdgt.into()
}

fn wdgt_tag_selector<'a>(state: &'a UiState) -> Element<'a> {
    let mut wdgt = row![text("Tags:").size(12).align_y(iced::Alignment::Center),];

    // Tag fuzzy suggestions computation
    let suggestions = search_tags(
        &state.tag_search_query,
        &state.api_tags,
        &state.station_filter.tags,
        5,
    );

    // Tag search text input
    let mut search_input = text_input("Filter tag...", &state.tag_search_query)
        .on_input(Message::TagSearchInputChanged)
        .size(12)
        .padding([3, 8])
        .width(Length::Fixed(110.0));

    if let Some((top_tag, _)) = suggestions.first() {
        search_input = search_input.on_submit(Message::AddTagFilter(top_tag.name.clone()));
    } else if !state.tag_search_query.trim().is_empty() {
        search_input = search_input.on_submit(Message::AddTagFilter(
            state.tag_search_query.trim().to_string(),
        ));
    }

    wdgt = wdgt.push(search_input);

    if !state.tag_search_query.is_empty() {
        wdgt = wdgt.push(
            button(text("✕").size(10).align_y(iced::Alignment::Center))
                .padding([3, 6])
                .style(button::warning)
                .on_press(Message::TagSearchInputChanged(String::new())),
        );
    }

    let mut suggestions_row = row![].spacing(6).align_y(iced::Alignment::Center);

    for (tag, _score) in &suggestions {
        let label = format!("{} ({})", tag.name, format_count(tag.stationcount));
        suggestions_row = suggestions_row.push(
            button(text(label).size(11).align_y(iced::Alignment::Center))
                .padding([2, 6])
                .on_press(Message::AddTagFilter(tag.name.clone())),
        );
    }

    wdgt = wdgt.push(suggestions_row.wrap());
    wdgt.into()
}

fn wdgt_earth_controls<'a>(state: &'a UiState) -> Element<'a> {
    let borders_label = if state.earth_config.show_country_outlines {
        "Borders: ON"
    } else {
        "Borders: OFF"
    };

    let borders_btn = button(
        text(borders_label)
            .size(11)
            .align_y(iced::Alignment::Center),
    )
    .padding([3, 8])
    .style(if state.earth_config.show_country_outlines {
        button::primary
    } else {
        button::secondary
    })
    .on_press(Message::ToggleCountryOutlines);

    let theme_label = format!("Earth: {}", state.earth_config.base_texture.label());
    let theme_btn = button(text(theme_label).size(11).align_y(iced::Alignment::Center))
        .padding([3, 8])
        .style(button::secondary)
        .on_press(Message::CycleEarthBaseTexture);

    row![borders_btn, theme_btn]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
}

pub fn toolbar<'a>(state: &'a UiState) -> Element<'a> {
    let left_controls = row![
        wdgt_station_limit(state),
        wdgt_display_station_count(state),
        wdgt_country_select(state),
        wdgt_tag_selector(state)
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let right_controls = wdgt_earth_controls(state);

    let content = row![
        left_controls,
        Space::new().width(Length::Fill),
        right_controls,
    ]
    .align_y(iced::Alignment::Center);

    container(content)
        .style(container::bordered_box)
        .width(Length::Fill)
        .padding(6)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_count() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1000), "1,000");
        assert_eq!(format_count(12450), "12,450");
        assert_eq!(format_count(1000000), "1,000,000");
    }

    #[test]
    fn test_search_tags_ranking_and_filtering() {
        let tags = vec![
            radiobrowser::ApiTag {
                name: "rock".to_string(),
                stationcount: 10000,
            },
            radiobrowser::ApiTag {
                name: "classic rock".to_string(),
                stationcount: 5000,
            },
            radiobrowser::ApiTag {
                name: "indie rock".to_string(),
                stationcount: 2000,
            },
            radiobrowser::ApiTag {
                name: "pop".to_string(),
                stationcount: 8000,
            },
        ];

        let active_tags = vec!["rock".to_string()];
        let results = search_tags("roc", &tags, &active_tags, 5);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0.name, "classic rock");
        assert_eq!(results[1].0.name, "indie rock");
    }
}
