use serde::{Deserialize, Serialize};
use std::fmt;

/// Hierarchical slash-separated path.
/// Each part must be non-empty ASCII alphanumeric + hyphen + underscore.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Namespace(String);

impl Namespace {
    fn validate(s: &str) -> Result<(), String> {
        if s.is_empty() {
            return Err("namespace must not be empty".to_string());
        }
        for part in s.split('/') {
            if part.is_empty() {
                return Err("namespace parts must not be empty (check for leading, trailing, or double slashes)".to_string());
            }
            for ch in part.chars() {
                if !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' {
                    return Err(format!("invalid character '{ch}' in namespace part '{part}'"));
                }
            }
        }
        Ok(())
    }

    pub fn starts_with(&self, prefix: &Namespace) -> bool {
        if self.0 == prefix.0 {
            return true;
        }
        self.0.starts_with(&format!("{}/", prefix.0))
    }
}

impl TryFrom<String> for Namespace {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::validate(&s)?;
        Ok(Namespace(s))
    }
}

impl TryFrom<&str> for Namespace {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_string())
    }
}

impl From<Namespace> for String {
    fn from(n: Namespace) -> Self {
        n.0
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Namespace {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_simple() {
        let ns = Namespace::try_from("myapp").unwrap();
        assert_eq!(ns.as_ref(), "myapp");
    }

    #[test]
    fn valid_hierarchical() {
        let ns = Namespace::try_from("myapp/tasks/processing").unwrap();
        assert_eq!(ns.as_ref(), "myapp/tasks/processing");
    }

    #[test]
    fn valid_with_hyphens_and_underscores() {
        let ns = Namespace::try_from("my-app/task_queue/v2").unwrap();
        assert_eq!(ns.as_ref(), "my-app/task_queue/v2");
    }

    #[test]
    fn empty_fails() {
        assert!(Namespace::try_from("").is_err());
    }

    #[test]
    fn empty_part_fails() {
        assert!(Namespace::try_from("my//app").is_err(), "double slash should fail");
        assert!(Namespace::try_from("/myapp").is_err(), "leading slash should fail");
        assert!(Namespace::try_from("myapp/").is_err(), "trailing slash should fail");
    }

    #[test]
    fn invalid_chars_fail() {
        assert!(Namespace::try_from("my@app").is_err(), "@ should fail");
        assert!(Namespace::try_from("my.app").is_err(), ". should fail");
        assert!(Namespace::try_from("my app").is_err(), "space should fail");
    }

    #[test]
    fn starts_with_works() {
        let ns = Namespace::try_from("myapp/tasks/processing").unwrap();
        let prefix = Namespace::try_from("myapp/tasks").unwrap();
        let other = Namespace::try_from("myapp/other").unwrap();
        let exact = Namespace::try_from("myapp/tasks/processing").unwrap();

        assert!(ns.starts_with(&prefix));
        assert!(!ns.starts_with(&other));
        assert!(ns.starts_with(&exact));
        assert!(!prefix.starts_with(&ns));
    }
}
