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

    #[test]
    fn test_station_limit_to_count() {
        assert_eq!(StationLimit::Limit500.to_count(), Some(500));
        assert_eq!(StationLimit::Limit1000.to_count(), Some(1000));
        assert_eq!(StationLimit::Limit2500.to_count(), Some(2500));
        assert_eq!(StationLimit::Limit5000.to_count(), Some(5000));
        assert_eq!(StationLimit::All.to_count(), None);
    }
}
