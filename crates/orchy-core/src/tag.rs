use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Tag(String);

impl Tag {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim().trim_start_matches('#').to_lowercase();
        if value.is_empty() {
            return Err(DomainError::validation("tag must not be empty"));
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '/')
        {
            return Err(DomainError::validation(format!(
                "tag `{value}` may only contain lowercase letters, digits, `-` and `/`"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Tag {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for Tag {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<Tag> for String {
    fn from(tag: Tag) -> Self {
        tag.0
    }
}

pub fn apply(tags: &mut Vec<Tag>, add: Vec<Tag>, remove: &[Tag]) {
    for tag in add {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags.retain(|t| !remove.contains(t));
    tags.sort();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(s: &str) -> Tag {
        Tag::new(s).unwrap()
    }

    #[test]
    fn normalises_case_and_strips_a_leading_hash() {
        assert_eq!(tag("#Rust").as_str(), "rust");
        assert_eq!(tag("RUST"), tag("rust"));
    }

    #[test]
    fn allows_hierarchical_tags() {
        assert!(Tag::new("lang/rust").is_ok());
    }

    #[test]
    fn rejects_spaces_and_emptiness() {
        assert!(Tag::new("").is_err());
        assert!(Tag::new("#").is_err());
        assert!(Tag::new("two words").is_err());
    }

    #[test]
    fn apply_is_set_semantics_so_it_is_merge_safe() {
        let mut tags = vec![tag("a"), tag("b")];
        apply(&mut tags, vec![tag("b"), tag("c")], &[]);
        assert_eq!(tags, vec![tag("a"), tag("b"), tag("c")], "no duplicates");

        apply(&mut tags, vec![], &[tag("b")]);
        assert_eq!(tags, vec![tag("a"), tag("c")]);
    }

    #[test]
    fn apply_removes_after_adding_so_a_conflicting_pair_removes() {
        let mut tags = vec![tag("a")];
        apply(&mut tags, vec![tag("b")], &[tag("b")]);
        assert_eq!(tags, vec![tag("a")]);
    }

    #[test]
    fn apply_keeps_tags_sorted_for_a_stable_serialisation() {
        let mut tags = vec![];
        apply(&mut tags, vec![tag("z"), tag("a"), tag("m")], &[]);
        assert_eq!(tags, vec![tag("a"), tag("m"), tag("z")]);
    }
}
