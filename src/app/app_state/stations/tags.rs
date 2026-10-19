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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_match_mode_display() {
        assert_eq!(TagMatchMode::And.to_string(), "AND");
        assert_eq!(TagMatchMode::Or.to_string(), "OR");
    }
}
