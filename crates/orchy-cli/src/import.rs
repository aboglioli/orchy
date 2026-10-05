use std::fs;
use std::path::Path;

use orchy_application::create_document::CreateDocumentCommand;
use orchy_core::{Kind, Relation};
use orchy_store_vault::markdown::MarkdownFile;
use serde_json::Value;

use crate::error::{CliError, CliResult};
use crate::stdin;

/// Fields orchy keeps for itself; an imported file's values for them are not carried over.
const OWNED: [&str; 8] = [
    "id",
    "type",
    "kind",
    "namespace",
    "status",
    "created",
    "updated",
    "title",
];

pub(crate) struct Import {
    pub source: String,
    pub kind: String,
    pub title: Option<String>,
    pub namespace: Option<String>,
    pub tags: Vec<String>,
}

pub(crate) fn read(source: &str) -> CliResult<String> {
    if source == "-" {
        return stdin::or_read(None);
    }
    if source.starts_with("https://") || source.starts_with("http://") {
        return ureq::get(source)
            .call()
            .and_then(|mut response| response.body_mut().read_to_string())
            .map_err(|e| CliError::config(format!("fetching {source}: {e}")));
    }
    Ok(fs::read_to_string(source)?)
}

pub(crate) fn command(import: Import, text: &str) -> CliResult<CreateDocumentCommand> {
    let file = MarkdownFile::parse(text)
        .map_err(|e| CliError::config(format!("{}: {e}", import.source)))?;

    let title = import
        .title
        .or_else(|| file.frontmatter.string("title").map(str::to_owned))
        .or_else(|| first_heading(file.body.as_str()))
        .or_else(|| name_of(&import.source))
        .ok_or_else(|| CliError::config("nothing to title the document with: pass --title"))?;

    let mut tags = import.tags;
    if let Some(Value::Array(listed)) = file.frontmatter.get("tags") {
        tags.extend(listed.iter().filter_map(Value::as_str).map(str::to_owned));
    }

    let links: Vec<&str> = file
        .frontmatter
        .keys()
        .filter(|key| key.parse::<Relation>().is_ok())
        .collect();
    if !links.is_empty() {
        return Err(CliError::config(format!(
            "{} links to other entities through `{}`; links are not imported, because nothing \
             says what they point at exists here. Remove those fields, import, then `orchy link`",
            import.source,
            links.join("`, `")
        )));
    }

    let fields = file
        .frontmatter
        .iter()
        .filter(|(key, _)| !OWNED.contains(key) && *key != "tags" && !Kind::is_projected_field(key))
        .map(|(key, value)| (key.to_owned(), value.clone()))
        .collect();

    Ok(CreateDocumentCommand {
        kind: import.kind,
        title,
        namespace: import.namespace,
        body: Some(file.body.as_str().to_owned()),
        tags,
        fields,
        ..Default::default()
    })
}

fn first_heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(|heading| heading.trim().to_owned())
        .filter(|heading| !heading.is_empty())
}

fn name_of(source: &str) -> Option<String> {
    if source == "-" {
        return None;
    }
    let last = source.trim_end_matches('/').rsplit('/').next()?;
    Path::new(last)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.replace(['-', '_'], " "))
        .filter(|stem| !stem.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import(source: &str) -> Import {
        Import {
            source: source.to_owned(),
            kind: "note".to_owned(),
            title: None,
            namespace: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn frontmatter_becomes_title_tags_and_fields_of_the_document() {
        let text = "---\ntitle: Deploys\ntags: [ops]\nowner: alan\nid: 123\ncreated: 2020-01-01\n---\n\nSteps.\n";
        let command = command(import("notes/deploys.md"), text).unwrap();
        assert_eq!(command.title, "Deploys");
        assert_eq!(command.tags, vec!["ops"]);
        assert_eq!(
            command.fields,
            vec![("owner".to_owned(), Value::from("alan"))]
        );
        assert_eq!(command.body.as_deref(), Some("Steps."));
    }

    #[test]
    fn links_in_an_imported_file_are_refused_rather_than_stored_unchecked() {
        let text = "---\ntitle: Deploys\nsupersedes: [document:01ARZ3NDEKTSV4RRFFQ69G5FAV]\n---\n\nSteps.\n";
        let err = command(import("deploys.md"), text).unwrap_err();
        assert!(err.to_string().contains("`supersedes`"), "{err}");
    }

    #[test]
    fn without_a_title_the_first_heading_or_the_file_name_is_used() {
        let from_heading = command(import("x.md"), "# Rollback plan\n\nundo").unwrap();
        assert_eq!(from_heading.title, "Rollback plan");
        let from_name = command(import("docs/rollback-plan.md"), "undo").unwrap();
        assert_eq!(from_name.title, "rollback plan");
    }
}
