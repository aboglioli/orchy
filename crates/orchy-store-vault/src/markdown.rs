use std::collections::BTreeMap;
use std::mem;

use orchy_core::{Body, DomainError, Frontmatter, Result};
use serde_json::Value;

const FENCE: &str = "---";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MarkdownFile {
    pub frontmatter: Frontmatter,
    pub body: Body,
}

impl MarkdownFile {
    pub fn parse(source: &str) -> Result<Self> {
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);
        let Some(rest) = source.strip_prefix(FENCE) else {
            return Ok(Self {
                frontmatter: Frontmatter::new(),
                body: Body::new(source),
            });
        };
        let rest = rest.strip_prefix('\n').unwrap_or(rest);

        let Some((yaml_len, fence_len)) = find_closing_fence(rest) else {
            return Err(DomainError::validation(
                "frontmatter block is opened with `---` but never closed",
            ));
        };
        let yaml = &rest[..yaml_len];
        let after_fence = &rest[yaml_len + fence_len..];
        // one blank line between the fence and the body is orchy's own formatting, not content
        let body = after_fence.strip_prefix('\n').unwrap_or(after_fence);

        Ok(Self {
            frontmatter: parse_frontmatter(yaml)?,
            body: Body::new(body),
        })
    }

    /// Renders over the file as it is on disk so a human's formatting survives in the diff:
    /// comments are kept, and a field whose value did not change keeps its original lines.
    pub fn render_over(&self, original: &str) -> Result<String> {
        let original = original.strip_prefix('\u{feff}').unwrap_or(original);
        let Some(rest) = original.strip_prefix(FENCE) else {
            return self.render();
        };
        let rest = rest.strip_prefix('\n').unwrap_or(rest);
        let Some((yaml_len, _)) = find_closing_fence(rest) else {
            return self.render();
        };
        let yaml = &rest[..yaml_len];
        let Ok(before) = parse_frontmatter(yaml) else {
            return self.render();
        };
        if self.frontmatter.is_empty() {
            return self.render();
        }

        let (blocks, trailing) = blocks_of(yaml);
        let mut out = String::new();
        for (key, value) in self.frontmatter.iter() {
            let block = blocks.iter().find(|b| b.key == key);
            if let Some(block) = block {
                out.push_str(&block.leading);
            }
            match block {
                Some(block) if before.get(key) == Some(value) => out.push_str(&block.text),
                _ => {
                    let mut single = Frontmatter::new();
                    single.set(key, value.clone());
                    out.push_str(&render_frontmatter(&single)?);
                }
            }
        }
        out.push_str(&trailing);

        if self.body.is_empty() {
            return Ok(format!("{FENCE}\n{out}{FENCE}\n"));
        }
        Ok(format!("{FENCE}\n{out}{FENCE}\n\n{}\n", self.body.as_str()))
    }

    pub fn render(&self) -> Result<String> {
        if self.frontmatter.is_empty() {
            return Ok(format!("{}\n", self.body.as_str()));
        }
        let yaml = render_frontmatter(&self.frontmatter)?;
        if self.body.is_empty() {
            return Ok(format!("{FENCE}\n{yaml}{FENCE}\n"));
        }
        Ok(format!(
            "{FENCE}\n{yaml}{FENCE}\n\n{}\n",
            self.body.as_str()
        ))
    }
}

struct Block {
    key: String,
    leading: String,
    text: String,
}

/// Splits frontmatter into one block per top-level key: its own lines, plus the comment and
/// blank lines just above it. Comments after the last key are returned apart.
fn blocks_of(yaml: &str) -> (Vec<Block>, String) {
    let mut blocks: Vec<Block> = Vec::new();
    let mut pending = String::new();
    for line in yaml.split_inclusive('\n') {
        let is_aside = line.trim().is_empty() || line.starts_with('#');
        let is_continuation = line.starts_with([' ', '\t', '-']);
        if is_aside {
            pending.push_str(line);
            continue;
        }
        if is_continuation && let Some(last) = blocks.last_mut() {
            last.text.push_str(&pending);
            pending.clear();
            last.text.push_str(line);
            continue;
        }
        let key = line
            .split_once(':')
            .map(|(key, _)| key.trim().trim_matches(['"', '\'']).to_owned())
            .unwrap_or_default();
        blocks.push(Block {
            key,
            leading: mem::take(&mut pending),
            text: line.to_owned(),
        });
    }
    (blocks, pending)
}

/// Returns `(bytes of yaml before the fence, bytes of the fence line itself)`.
fn find_closing_fence(rest: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == FENCE {
            return Some((offset, line.len()));
        }
        offset += line.len();
    }
    None
}

fn parse_frontmatter(yaml: &str) -> Result<Frontmatter> {
    if yaml.trim().is_empty() {
        return Ok(Frontmatter::new());
    }
    let parsed: BTreeMap<String, Value> = serde_saphyr::from_str(yaml)
        .map_err(|e| DomainError::validation(format!("frontmatter is not valid YAML: {e}")))?;

    let order = key_order(yaml);
    let mut frontmatter = Frontmatter::new();
    for key in &order {
        if let Some(value) = parsed.get(key) {
            frontmatter.set(key.clone(), value.clone());
        }
    }
    for (key, value) in parsed {
        if !order.contains(&key) {
            frontmatter.set(key, value);
        }
    }
    Ok(frontmatter)
}

/// serde deserialises into an unordered map, so the raw text is rescanned to recover the
/// order the author wrote.
fn key_order(yaml: &str) -> Vec<String> {
    yaml.lines()
        .filter(|line| !line.starts_with([' ', '\t', '-', '#']))
        .filter_map(|line| line.split_once(':'))
        .map(|(key, _)| key.trim().trim_matches(['"', '\'']).to_owned())
        .filter(|key| !key.is_empty())
        .collect()
}

fn render_frontmatter(frontmatter: &Frontmatter) -> Result<String> {
    let mut out = String::new();
    for (key, value) in frontmatter.iter() {
        let rendered = serde_saphyr::to_string(&value).map_err(|e| {
            DomainError::validation(format!("`{key}` cannot be written as YAML: {e}"))
        })?;
        let rendered = rendered.trim_end();

        // decided by type, not by whether the emitter produced a newline: a one-element list
        // renders as `- a` and would be written back as `tags: - a`, which is not YAML
        let is_block = matches!(value, Value::Array(_) | Value::Object(_));
        if is_block {
            if matches!(value, Value::Array(items) if items.is_empty()) {
                out.push_str(&format!("{key}: []\n"));
                continue;
            }
            if matches!(value, Value::Object(fields) if fields.is_empty()) {
                out.push_str(&format!("{key}: {{}}\n"));
                continue;
            }
            out.push_str(&format!("{key}:\n"));
            for line in rendered.lines() {
                out.push_str(&format!("  {line}\n"));
            }
        } else {
            out.push_str(&format!("{key}: {rendered}\n"));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_frontmatter_and_body() {
        let file = MarkdownFile::parse(
            "---\ntype: decision\ntitle: Rotate keys\n---\n\n# Context\nbody\n",
        )
        .unwrap();
        assert_eq!(file.frontmatter.string("type"), Some("decision"));
        assert_eq!(file.frontmatter.string("title"), Some("Rotate keys"));
        assert!(file.body.as_str().starts_with("# Context"));
    }

    #[test]
    fn a_file_without_frontmatter_is_all_body() {
        let file = MarkdownFile::parse("# Just a note\n").unwrap();
        assert!(file.frontmatter.is_empty());
        assert_eq!(file.body.as_str(), "# Just a note");
    }

    #[test]
    fn an_unterminated_frontmatter_block_is_an_error_not_a_silent_body() {
        let err = MarkdownFile::parse("---\ntype: note\n\n# Body\n").unwrap_err();
        assert!(err.to_string().contains("never closed"), "{err}");
    }

    #[test]
    fn a_horizontal_rule_in_the_body_is_not_mistaken_for_a_fence() {
        let file = MarkdownFile::parse("---\ntype: note\n---\n\nabove\n\n---\n\nbelow\n").unwrap();
        assert_eq!(file.frontmatter.string("type"), Some("note"));
        assert!(file.body.as_str().contains("above"));
        assert!(file.body.as_str().contains("below"));
    }

    #[test]
    fn key_order_survives_a_round_trip() {
        let source = "---\nzebra: 1\nalpha: 2\nmiddle: 3\n---\n\nbody\n";
        let file = MarkdownFile::parse(source).unwrap();
        assert_eq!(
            file.frontmatter.keys().collect::<Vec<_>>(),
            vec!["zebra", "alpha", "middle"],
            "an author's key order must not be alphabetised behind their back"
        );
        let rendered = file.render().unwrap();
        let again = MarkdownFile::parse(&rendered).unwrap();
        assert_eq!(again.frontmatter, file.frontmatter);
        assert_eq!(again.body, file.body);
    }

    #[test]
    fn lists_and_scalars_both_round_trip() {
        let mut frontmatter = Frontmatter::new();
        frontmatter.set("type", json!("decision"));
        frontmatter.set("tags", json!(["rust", "auth"]));
        frontmatter.set("count", json!(3));
        frontmatter.set("live", json!(true));

        let file = MarkdownFile {
            frontmatter,
            body: Body::new("# Body"),
        };
        let rendered = file.render().unwrap();
        let parsed = MarkdownFile::parse(&rendered).unwrap();

        assert_eq!(parsed.frontmatter.string("type"), Some("decision"));
        assert_eq!(parsed.frontmatter.strings("tags"), vec!["rust", "auth"]);
        assert_eq!(parsed.frontmatter.get("count"), Some(&json!(3)));
        assert_eq!(parsed.frontmatter.get("live"), Some(&json!(true)));
    }

    #[test]
    fn an_authors_unknown_keys_are_preserved_verbatim() {
        let source = "---\ntype: note\nmy_own_field: hello\nnested:\n  a: 1\n---\n\nbody\n";
        let file = MarkdownFile::parse(source).unwrap();
        assert_eq!(file.frontmatter.string("my_own_field"), Some("hello"));
        assert!(file.frontmatter.contains("nested"));

        let again = MarkdownFile::parse(&file.render().unwrap()).unwrap();
        assert_eq!(again.frontmatter.string("my_own_field"), Some("hello"));
        assert!(again.frontmatter.contains("nested"));
    }

    #[test]
    fn a_document_with_no_body_ends_at_the_fence() {
        let mut frontmatter = Frontmatter::new();
        frontmatter.set("type", json!("task"));
        let rendered = MarkdownFile {
            frontmatter,
            body: Body::new(""),
        }
        .render()
        .unwrap();
        assert_eq!(rendered, "---\ntype: task\n---\n");
        assert!(
            !rendered.ends_with("\n\n"),
            "a bodyless file must not trail blank lines"
        );
        assert!(MarkdownFile::parse(&rendered).unwrap().body.is_empty());
    }

    #[test]
    fn an_empty_frontmatter_block_is_not_an_error() {
        let file = MarkdownFile::parse("---\n---\n\nbody\n").unwrap();
        assert!(file.frontmatter.is_empty());
        assert_eq!(file.body.as_str(), "body");
    }

    #[test]
    fn rendering_is_stable_so_an_untouched_document_does_not_churn() {
        let source = "---\ntype: note\ntags:\n  - a\n---\n\n# Body\n";
        let once = MarkdownFile::parse(source).unwrap().render().unwrap();
        let twice = MarkdownFile::parse(&once).unwrap().render().unwrap();
        assert_eq!(once, twice, "render must be a fixed point");
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_frontmatter() {
        let file = MarkdownFile::parse("\u{feff}---\ntype: note\n---\n\nbody\n").unwrap();
        assert_eq!(file.frontmatter.string("type"), Some("note"));
    }

    #[test]
    fn rewriting_keeps_comments_and_the_lines_of_unchanged_fields() {
        let original = "---\nid: 1\n# who signs off\nreviewer: bob\ntags: [a, b]   # flow style\nowner: 'carol'\n# end of fields\n---\n\nbody\n";
        let mut file = MarkdownFile::parse(original).unwrap();
        file.frontmatter.set("reviewer", json!("alan"));

        let rendered = file.render_over(original).unwrap();
        assert_eq!(
            rendered,
            "---\nid: 1\n# who signs off\nreviewer: alan\ntags: [a, b]   # flow style\nowner: 'carol'\n# end of fields\n---\n\nbody\n"
        );
        assert_eq!(MarkdownFile::parse(&rendered).unwrap(), file);
    }
}
