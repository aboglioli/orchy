use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

impl Priority {
    pub const ALL: [Self; 4] = [Self::Low, Self::Normal, Self::High, Self::Urgent];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Priority {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.as_str() == s)
            .ok_or_else(|| DomainError::validation(format!("unknown priority: {s}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_from_low_to_urgent_so_sorting_surfaces_the_important_work() {
        let mut all = vec![Priority::Urgent, Priority::Low, Priority::High];
        all.sort();
        assert_eq!(all, vec![Priority::Low, Priority::High, Priority::Urgent]);
        assert!(Priority::Urgent > Priority::Normal);
    }

    #[test]
    fn defaults_to_normal() {
        assert_eq!(Priority::default(), Priority::Normal);
    }

    #[test]
    fn round_trips_through_its_wire_name() {
        for p in Priority::ALL {
            assert_eq!(p.as_str().parse::<Priority>().unwrap(), p);
        }
        assert!("critical".parse::<Priority>().is_err());
    }
}
