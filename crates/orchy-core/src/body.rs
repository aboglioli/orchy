use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::{DomainError, Result};

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
        self.spans()
            .into_iter()
            .map(|span| Section {
                heading: span.heading,
                level: span.level,
                body: &self.0[span.body_start..span.end],
            })
            .collect()
    }

    /// The text before the first heading: often the most important sentence of a document.
    pub fn preamble(&self) -> &str {
        let end = self
            .spans()
            .first()
            .map_or(self.0.len(), |span| span.heading_start);
        self.0[..end].trim()
    }

    /// The section under `heading` (case-insensitive). With several such headings, `nth`
    /// (1-based) picks one; without it the address is ambiguous and nothing is guessed.
    pub fn section(&self, heading: &str, nth: Option<usize>) -> Result<Section<'_>> {
        let span = self.pick(heading, nth)?;
        Ok(Section {
            body: &self.0[span.body_start..span.end],
            heading: span.heading,
            level: span.level,
        })
    }

    pub fn replace_section(
        &self,
        heading: &str,
        nth: Option<usize>,
        replacement: &str,
    ) -> Result<Self> {
        let span = self.pick(heading, nth)?;
        let mut out = String::with_capacity(self.0.len() + replacement.len());
        out.push_str(&self.0[..span.body_start]);
        out.push_str(replacement.trim());
        out.push('\n');
        if span.end < self.0.len() {
            out.push('\n');
            out.push_str(&self.0[span.end..]);
        }
        Ok(Self::new(out))
    }

    fn pick(&self, heading: &str, nth: Option<usize>) -> Result<Span> {
        let mut matching: Vec<Span> = self
            .spans()
            .into_iter()
            .filter(|span| span.heading.eq_ignore_ascii_case(heading.trim()))
            .collect();
        match (nth, matching.len()) {
            (_, 0) => Err(DomainError::not_found("section", heading)),
            (None, 1) => Ok(matching.remove(0)),
            (None, count) => Err(DomainError::Ambiguous {
                input: heading.trim().to_owned(),
                count,
            }),
            (Some(n), count) if n >= 1 && n <= count => Ok(matching.remove(n - 1)),
            (Some(n), count) => Err(DomainError::not_found(
                "section",
                format!("{heading} #{n} (there are {count})"),
            )),
        }
    }

    fn spans(&self) -> Vec<Span> {
        let mut spans: Vec<Span> = Vec::new();
        let mut fence: Option<&str> = None;
        let mut offset = 0;

        for line in self.0.split_inclusive('\n') {
            let trimmed = line.trim_end();
            let marker = fence_marker(trimmed);
            match (fence, marker) {
                (Some(open), Some(close)) if close == open => fence = None,
                (None, Some(open)) => fence = Some(open),
                _ => {}
            }
            let heading = if fence.is_none() && marker.is_none() {
                heading_level(trimmed)
            } else {
                None
            };
            if let Some(level) = heading {
                if let Some(last) = spans.last_mut() {
                    last.end = offset;
                }
                spans.push(Span {
                    heading: trimmed.trim_start_matches('#').trim().to_owned(),
                    level,
                    heading_start: offset,
                    body_start: offset + line.len(),
                    end: self.0.len(),
                });
            }
            offset += line.len();
        }
        spans
    }

    pub fn replace_once(&self, needle: &str, replacement: &str) -> Result<Option<Self>> {
        let count = self.0.matches(needle).count();
        match count {
            0 => Ok(None),
            1 => Ok(Some(Self::new(self.0.replacen(needle, replacement, 1)))),
            n => Err(DomainError::validation(format!(
                "`{needle}` appears {n} times; it must be unique to be replaced"
            ))),
        }
    }
}

/// An ATX heading: one to six `#`, then a space or the end of the line, as markdown has it.
/// `#tag` is not a heading.
fn heading_level(line: &str) -> Option<usize> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    let rest = &line[hashes..];
    ((1..=6).contains(&hashes) && (rest.is_empty() || rest.starts_with(' '))).then_some(hashes)
}

/// The fence a code block opens or closes with; headings inside one are code, not structure.
fn fence_marker(line: &str) -> Option<&'static str> {
    let line = line.trim_start();
    if line.starts_with("```") {
        return Some("```");
    }
    if line.starts_with("~~~") {
        return Some("~~~");
    }
    None
}

struct Span {
    heading: String,
    level: usize,
    heading_start: usize,
    body_start: usize,
    end: usize,
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
        let out = body.replace_section("Two", None, "gamma").unwrap();
        assert!(out.as_str().contains("alpha"));
        assert!(out.as_str().contains("gamma"));
        assert!(!out.as_str().contains("beta"));
    }

    #[test]
    fn a_missing_heading_is_not_found() {
        let err = Body::new("# One\nalpha")
            .replace_section("Nope", None, "x")
            .unwrap_err();
        assert!(matches!(err, DomainError::NotFound { .. }), "{err:?}");
    }

    #[test]
    fn two_sections_sharing_a_heading_are_ambiguous_until_one_is_picked() {
        let body = Body::new("## Notes\nfirst\n\n## Notes\nsecond\n\n## End\nend");
        let err = body.replace_section("notes", None, "x").unwrap_err();
        assert!(
            matches!(err, DomainError::Ambiguous { count: 2, .. }),
            "{err:?}"
        );

        let out = body.replace_section("Notes", Some(2), "changed").unwrap();
        assert_eq!(
            out.as_str(),
            "## Notes\nfirst\n\n## Notes\nchanged\n\n## End\nend",
            "only the second changes, and the blank line before the next heading stays"
        );
        assert_eq!(body.section("Notes", Some(1)).unwrap().body.trim(), "first");
    }

    #[test]
    fn sections_know_their_level() {
        let body = Body::new("# Top\n## Sub\n### Deep");
        let levels: Vec<usize> = body.sections().iter().map(|s| s.level).collect();
        assert_eq!(levels, vec![1, 2, 3]);
    }

    #[test]
    fn a_hash_inside_a_code_block_is_not_a_heading() {
        let body = Body::new("## Setup\n```bash\n# install first\nmake\n```\n\n## Use\nrun");
        let headings: Vec<String> = body.sections().into_iter().map(|s| s.heading).collect();
        assert_eq!(headings, vec!["Setup", "Use"]);
    }

    #[test]
    fn a_hashtag_is_not_a_heading() {
        assert!(Body::new("#tag and more\ntext").sections().is_empty());
    }

    #[test]
    fn the_preamble_is_what_comes_before_the_first_heading() {
        assert_eq!(
            Body::new("Intro line.\n\n## Details\nx").preamble(),
            "Intro line."
        );
        assert_eq!(
            Body::new("no headings at all").preamble(),
            "no headings at all"
        );
        assert_eq!(Body::new("## Starts with one").preamble(), "");
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
