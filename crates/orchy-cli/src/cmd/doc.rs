use orchy_application::Application;
use orchy_application::create_document::CreateDocumentCommand;
use orchy_application::dto::{DocumentDto, HitDto};
use orchy_application::edit_document::{EditDocumentCommand, EditMode};
use orchy_application::link_entities::LinkEntitiesCommand;
use orchy_application::promote_document::PromoteDocumentCommand;
use orchy_application::read_document::ReadDocumentCommand;
use orchy_application::recall::RecallCommand;
use orchy_application::set_document_field::SetDocumentFieldCommand;
use orchy_application::supersede_document::SupersedeDocumentCommand;
use orchy_application::traverse_graph::TraverseGraphCommand;
use orchy_application::update_document::UpdateDocumentCommand;

use crate::error::{CliError, CliResult};
use crate::output::{Output, short};
use crate::{resolve, stdin};

pub(crate) async fn new(
    app: &Application,
    kind: String,
    title: String,
    namespace: Option<String>,
    tags: Vec<String>,
    body: Option<String>,
    out: &Output,
) -> CliResult<()> {
    let document = app
        .create_document
        .execute(CreateDocumentCommand {
            kind,
            title,
            namespace,
            body,
            tags,
        })
        .await?;
    out.emit(&document, |d| format!("{}  {}", short(&d.id), d.title))
}

pub(crate) async fn read(
    app: &Application,
    target: String,
    section: Option<String>,
    out: &Output,
) -> CliResult<()> {
    let document_id = resolve::document(app, &target).await?;
    let response = app
        .read_document
        .execute(ReadDocumentCommand {
            document_id,
            section,
        })
        .await?;
    out.emit(&response, |r| match &r.section {
        Some(body) => body.clone(),
        None => detail(&r.document, out),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn edit(
    app: &Application,
    target: String,
    section: Option<String>,
    replace_in: Option<String>,
    replace: bool,
    if_match: Option<String>,
    content: Option<String>,
    out: &Output,
) -> CliResult<()> {
    let mode = match (section, replace_in, replace) {
        (Some(heading), None, false) => EditMode::Section(heading),
        (None, Some(needle), false) => EditMode::ReplaceIn(needle),
        (None, None, true) => EditMode::Replace,
        (None, None, false) => EditMode::Append,
        _ => {
            return Err(CliError::config(
                "choose one of --section, --replace-in or --replace",
            ));
        }
    };

    let document_id = resolve::document(app, &target).await?;
    let document = app
        .edit_document
        .execute(EditDocumentCommand {
            document_id,
            content: stdin::or_read(content)?,
            mode,
            if_match,
        })
        .await?;
    out.emit(&document, |d| {
        format!("{}  {}", short(&d.id), out.dim(&d.content_hash[..12]))
    })
}

pub(crate) async fn set(
    app: &Application,
    target: String,
    assignments: Vec<String>,
    out: &Output,
) -> CliResult<()> {
    let mut fields = Vec::new();
    for assignment in &assignments {
        let (field, value) = assignment
            .split_once('=')
            .ok_or_else(|| CliError::config(format!("`{assignment}` is not field=value")))?;
        let parsed = serde_json::from_str(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.to_owned()));
        fields.push((field.to_owned(), parsed));
    }

    let document_id = resolve::document(app, &target).await?;
    let document = app
        .set_document_field
        .execute(SetDocumentFieldCommand {
            document_id,
            fields,
        })
        .await?;
    out.emit(&document, |d| format!("{}  updated", short(&d.id)))
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn recall(
    app: &Application,
    query: Vec<String>,
    kind: Vec<String>,
    entities: Vec<String>,
    tags: Vec<String>,
    namespace: Option<String>,
    anchor: Option<String>,
    limit: Option<usize>,
    out: &Output,
) -> CliResult<()> {
    let hits = app
        .recall
        .execute(RecallCommand {
            text: query.join(" "),
            entities,
            kind,
            retired: false,
            status: Vec::new(),
            namespace,
            anchor,
            tags,
            limit,
        })
        .await?;
    out.emit(&hits, |h| render_hits(h, out))
}

pub(crate) async fn link(
    app: &Application,
    from: String,
    to: String,
    rel: String,
    remove: bool,
    out: &Output,
) -> CliResult<()> {
    let edge = app
        .link_entities
        .execute(LinkEntitiesCommand {
            from,
            to,
            relation: rel,
            remove,
        })
        .await?;
    out.emit(&edge, |e| {
        format!("{} -{}-> {}", short(&e.from), e.relation, short(&e.to))
    })
}

pub(crate) async fn graph(
    app: &Application,
    from: String,
    depth: u8,
    out: &Output,
) -> CliResult<()> {
    let hops = app
        .traverse_graph
        .execute(TraverseGraphCommand {
            from,
            depth: Some(depth),
        })
        .await?;
    out.emit(&hops, |h| {
        if h.is_empty() {
            return "no links".to_owned();
        }
        h.iter()
            .map(|hop| {
                format!(
                    "{}{} -{}-> {}",
                    "  ".repeat(hop.depth.saturating_sub(1) as usize),
                    short(&hop.edge.from),
                    hop.edge.relation,
                    short(&hop.edge.to)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

pub(crate) async fn supersede(
    app: &Application,
    old: String,
    by: String,
    out: &Output,
) -> CliResult<()> {
    let old_id = resolve::document(app, &old).await?;
    let new_id = resolve::document(app, &by).await?;
    let document = app
        .supersede_document
        .execute(SupersedeDocumentCommand { old_id, new_id })
        .await?;
    out.emit(&document, |d| format!("{}  superseded", short(&d.id)))
}

pub(crate) async fn set_status(
    app: &Application,
    target: String,
    status: &str,
    out: &Output,
) -> CliResult<()> {
    let document_id = resolve::document(app, &target).await?;
    let document = app
        .update_document
        .execute(UpdateDocumentCommand {
            document_id,
            status: Some(status.to_owned()),
            ..Default::default()
        })
        .await?;
    out.emit(&document, |d| {
        format!("{}  {}", short(&d.id), d.status.as_deref().unwrap_or(""))
    })
}

pub(crate) async fn promote(
    app: &Application,
    target: String,
    into: String,
    namespace: Option<String>,
    out: &Output,
) -> CliResult<()> {
    let document_id = resolve::document(app, &target).await?;
    let document = app
        .promote_document
        .execute(PromoteDocumentCommand {
            document_id,
            into,
            namespace,
        })
        .await?;
    out.emit(&document, |d| {
        format!("{}  promoted to {}", short(&d.id), d.namespace)
    })
}

fn render_hits(hits: &[HitDto], out: &Output) -> String {
    if hits.is_empty() {
        return "nothing found".to_owned();
    }
    hits.iter()
        .map(|h| {
            let heading = h.heading.as_deref().unwrap_or("(body)");
            format!(
                "{}  {}  {}\n  {}",
                out.dim(short(&h.id)),
                out.dim(&h.kind),
                out.bold(heading),
                h.excerpt.lines().next().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn detail(document: &DocumentDto, out: &Output) -> String {
    let mut lines = vec![
        format!("{}  {}", out.bold(&document.id), document.title),
        format!("  type       {}", document.kind),
        format!("  namespace  {}", document.namespace),
    ];
    if let Some(status) = &document.status {
        lines.push(format!("  status     {status}"));
    }
    if !document.tags.is_empty() {
        lines.push(format!("  tags       {}", document.tags.join(", ")));
    }
    lines.push(String::new());
    lines.push(document.body.clone());
    lines.join("\n")
}
