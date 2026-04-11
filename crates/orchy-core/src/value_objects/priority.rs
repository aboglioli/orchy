use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Priority::Low => "low",
            Priority::Normal => "normal",
            Priority::High => "high",
            Priority::Critical => "critical",
        };
        write!(f, "{s}")
    }
}

impl FromStr for Priority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(Priority::Low),
            "normal" => Ok(Priority::Normal),
            "high" => Ok(Priority::High),
            "critical" => Ok(Priority::Critical),
            other => Err(format!("unknown priority: {other}")),
        }
    }
}
