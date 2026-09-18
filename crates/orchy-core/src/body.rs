use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::Result;

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct Body(String);

impl Body {
    pub fn new(value: impl Into<String>) -> Self {
        let mut value: String = value.into();
        while value.ends_with('\n') || value.ends_with(' ') {
            value.pop();
        }
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    pub fn append(&self, extra: &str) -> Self {
        if self.is_empty() {
            return Self::new(extra);
        }
        Self::new(format!("{}\n\n{}", self.0, extra.trim()))
    }

    pub fn sections(&self) -> Vec<Section<'_>> {
        let mut sections = Vec::new();
        let mut current: Option<(String, usize, usize)> = None;
        let mut offset = 0;

        for line in self.0.split_inclusive('\n') {
            let trimmed = line.trim_end();
            if let Some(level) = heading_level(trimmed) {
                if let Some((heading, start, end)) = current.take() {
                    sections.push(Section {
                        heading,
                        level: 0,
                        body: &self.0[start..end],
                    });
                }
                let _ = level;
                current = Some((
                    trimmed.trim_start_matches('#').trim().to_owned(),
                    offset + line.len(),
                    offset + line.len(),
                ));
            } else if let Some((_, _, end)) = current.as_mut() {
                *end = offset + line.len();
            }
            offset += line.len();
        }
        if let Some((heading, start, end)) = current {
            sections.push(Section {
                heading,
                level: 0,
                body: &self.0[start..end],
            });
        }
        sections
    }

    pub fn replace_section(&self, heading: &str, replacement: &str) -> Option<Self> {
        let mut out = String::new();
        let mut replaced = false;
        let mut skipping = false;

        for line in self.0.split_inclusive('\n') {
            let trimmed = line.trim_end();
            if heading_level(trimmed).is_some() {
                let name = trimmed.trim_start_matches('#').trim();
                if name.eq_ignore_ascii_case(heading) {
                    out.push_str(line);
                    out.push_str(replacement.trim());
                    out.push('\n');
                    replaced = true;
                    skipping = true;
                    continue;
                }
                skipping = false;
            }
            if !skipping {
                out.push_str(line);
            }
        }
        replaced.then(|| Self::new(out))
    }

    pub fn replace_once(&self, needle: &str, replacement: &str) -> Result<Option<Self>> {
        let count = self.0.matches(needle).count();
        match count {
            0 => Ok(None),
            1 => Ok(Some(Self::new(self.0.replacen(needle, replacement, 1)))),
            n => Err(crate::error::DomainError::validation(format!(
                "`{needle}` appears {n} times; it must be unique to be replaced"
            ))),
        }
    }
}

fn heading_level(line: &str) -> Option<usize> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes).then_some(hashes)
}

#[derive(Debug, PartialEq, Eq)]
pub struct Section<'a> {
    pub heading: String,
    pub level: usize,
    pub body: &'a str,
}

impl fmt::Display for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for Body {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<Body> for String {
    fn from(body: Body) -> Self {
        body.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_whitespace_is_normalised_so_rewrites_do_not_churn() {
        assert_eq!(Body::new("hello\n\n\n").as_str(), "hello");
        assert_eq!(Body::new("hello   ").as_str(), "hello");
    }

    #[test]
    fn append_separates_with_a_blank_line() {
        assert_eq!(Body::new("a").append("b").as_str(), "a\n\nb");
    }

    #[test]
    fn append_to_an_empty_body_does_not_leave_leading_blank_lines() {
        assert_eq!(Body::new("").append("b").as_str(), "b");
    }

    #[test]
    fn sections_are_split_on_headings() {
        let body = Body::new("# One\nalpha\n\n# Two\nbeta\n");
        let sections = body.sections();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].heading, "One");
        assert!(sections[0].body.contains("alpha"));
        assert_eq!(sections[1].heading, "Two");
        assert!(sections[1].body.contains("beta"));
    }

    #[test]
    fn replace_section_swaps_only_the_named_section() {
        let body = Body::new("# One\nalpha\n\n# Two\nbeta\n");
        let out = body.replace_section("Two", "gamma").unwrap();
        assert!(out.as_str().contains("alpha"));
        assert!(out.as_str().contains("gamma"));
        assert!(!out.as_str().contains("beta"));
    }

    #[test]
    fn replace_section_returns_none_when_the_heading_is_absent() {
        assert!(
            Body::new("# One\nalpha")
                .replace_section("Nope", "x")
                .is_none()
        );
    }

    #[test]
    fn replace_once_refuses_an_ambiguous_match() {
        let body = Body::new("a\na\n");
        assert!(body.replace_once("a", "b").is_err());
    }

    #[test]
    fn replace_once_reports_a_miss_as_none_not_an_error() {
        assert_eq!(Body::new("a").replace_once("zzz", "b").unwrap(), None);
    }

    #[test]
    fn replace_once_swaps_a_unique_match() {
        let out = Body::new("hello world")
            .replace_once("world", "there")
            .unwrap();
        assert_eq!(out.unwrap().as_str(), "hello there");
    }
}
