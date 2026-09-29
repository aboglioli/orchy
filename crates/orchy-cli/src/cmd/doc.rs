use std::io::{self, Write};

use orchy_application::Application;
use orchy_application::consolidate_documents::ConsolidateDocumentsCommand;
use orchy_application::create_document::CreateDocumentCommand;
use orchy_application::dto::{DocumentDto, HitDto};
use orchy_application::edit_document::EditDocumentCommand;
use orchy_application::explain_entity::ExplainEntityCommand;
use orchy_application::export_vault::ExportVaultCommand;
use orchy_application::link_entities::LinkEntitiesCommand;
use orchy_application::promote_document::PromoteDocumentCommand;
use orchy_application::read_document::ReadDocumentCommand;
use orchy_application::recall::RecallCommand;
use orchy_application::reject_document::RejectDocumentCommand;
use orchy_application::set_document_field::SetDocumentFieldCommand;
use orchy_application::supersede_document::SupersedeDocumentCommand;
use orchy_application::traverse_graph::{TraversalHopDto, TraverseGraphCommand};
use orchy_application::update_document::UpdateDocumentCommand;

use crate::cli::GraphFormat;
use crate::error::{CliError, CliResult};
use crate::output::{Output, short};
use crate::resolve;

pub(crate) async fn new(
    app: &Application,
    command: CreateDocumentCommand,
    out: &Output,
) -> CliResult<()> {
    let created = app.create_document.execute(command).await?;
    out.emit(&created, |c| {
        let mut lines = vec![format!("{}  {}", short(&c.document.id), c.document.title)];
        if !c.similar.is_empty() {
            lines.push("similar, supersede or link instead of repeating:".to_owned());
            lines.extend(
                c.similar
                    .iter()
                    .map(|h| format!("  {}  {}", short(&h.id), h.title)),
            );
        }
        lines.join("\n")
    })
}

pub(crate) async fn read(
    app: &Application,
    target: String,
    section: Option<String>,
    nth: Option<usize>,
    out: &Output,
) -> CliResult<()> {
    let document_id = resolve::document(app, &target).await?;
    let response = app
        .read_document
        .execute(ReadDocumentCommand {
            document_id,
            section,
            nth,
        })
        .await?;
    out.emit(&response, |r| {
        if let Some(body) = &r.section {
            return body.clone();
        }
        let links = r
            .edges
            .iter()
            .map(|e| (e.relation.as_str(), e.to.as_str()))
            .chain(r.mentions.iter().map(|m| ("mentions", m.as_str())));
        let mut header: Vec<String> = links
            .map(|(relation, to)| format!("  {relation:<10} {to}"))
            .collect();
        let text = detail(&r.document, out);
        match text.split_once("\n\n") {
            Some((top, body)) if !header.is_empty() => {
                header.insert(0, top.to_owned());
                format!("{}\n\n{body}", header.join("\n"))
            }
            _ => text,
        }
    })
}

pub(crate) async fn edit(
    app: &Application,
    mut command: EditDocumentCommand,
    out: &Output,
) -> CliResult<()> {
    command.document_id = resolve::document(app, &command.document_id).await?;
    let document = app.edit_document.execute(command).await?;
    out.emit(&document, |d| {
        format!("{}  {}", short(&d.id), out.dim(&d.content_hash[..12]))
    })
}

pub(crate) async fn set(
    app: &Application,
    target: String,
    assignments: Vec<String>,
    if_match: Option<String>,
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
            if_match,
        })
        .await?;
    out.emit(&document, |d| format!("{}  updated", short(&d.id)))
}

pub(crate) async fn recall(
    app: &Application,
    command: RecallCommand,
    out: &Output,
) -> CliResult<()> {
    let found = app.recall.execute(command).await?;
    out.emit(&found.hits, |h| {
        let mut text = render_hits(h, out);
        if let Some(note) = out.truncated(h.len(), found.total) {
            text.push_str(&format!("\n\n{note}"));
        }
        text
    })
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
    relations: Vec<String>,
    format: GraphFormat,
    out: &Output,
) -> CliResult<()> {
    let hops = app
        .traverse_graph
        .execute(TraverseGraphCommand {
            from,
            depth: Some(depth),
            relations,
        })
        .await?;
    out.emit(&hops, |h| match format {
        GraphFormat::Text => render_graph(h),
        GraphFormat::Mermaid => mermaid(h),
        GraphFormat::Dot => dot(h),
    })
}

pub(crate) async fn export(app: &Application, namespace: Option<String>) -> CliResult<()> {
    let exported = app
        .export_vault
        .execute(ExportVaultCommand { namespace })
        .await?;
    let mut stdout = io::stdout().lock();
    for entity in &exported {
        let line = serde_json::to_string(entity)
            .map_err(|e| CliError::config(format!("encoding the export: {e}")))?;
        writeln!(stdout, "{line}")?;
    }
    Ok(())
}

pub(crate) async fn why(app: &Application, entity: String, out: &Output) -> CliResult<()> {
    let story = app
        .explain_entity
        .execute(ExplainEntityCommand { entity })
        .await?;
    out.emit(&story, |s| {
        let mut lines = vec![out.bold(&s.entity)];
        if s.history.is_empty() {
            lines.push("  no recorded history".to_owned());
        }
        for event in &s.history {
            lines.push(format!(
                "  {}  {:<26} {}",
                event.recorded_at.format("%Y-%m-%d %H:%M:%S"),
                event.topic,
                event.actor.as_deref().unwrap_or("?")
            ));
        }
        for (heading, links, outward) in [
            ("links from it", &s.links_out, true),
            ("links to it", &s.links_in, false),
        ] {
            if links.is_empty() {
                continue;
            }
            lines.push(String::new());
            lines.push(out.bold(heading));
            lines.extend(links.iter().map(|e| {
                let other = if outward { &e.to } else { &e.from };
                format!("  {:<16} {other}", e.relation)
            }));
        }
        lines.join("\n")
    })
}

fn mermaid(hops: &[TraversalHopDto]) -> String {
    let mut lines = vec!["graph LR".to_owned()];
    lines.extend(hops.iter().map(|hop| {
        format!(
            "  {}[\"{}\"] -->|{}| {}[\"{}\"]",
            node_id(&hop.edge.from),
            hop.edge.from,
            hop.edge.relation,
            node_id(&hop.edge.to),
            hop.edge.to
        )
    }));
    lines.join("\n")
}

fn dot(hops: &[TraversalHopDto]) -> String {
    let mut lines = vec!["digraph orchy {".to_owned()];
    lines.extend(hops.iter().map(|hop| {
        format!(
            "  \"{}\" -> \"{}\" [label=\"{}\"];",
            hop.edge.from, hop.edge.to, hop.edge.relation
        )
    }));
    lines.push("}".to_owned());
    lines.join("\n")
}

fn node_id(entity: &str) -> String {
    entity.replace(':', "_")
}

fn render_graph(hops: &[TraversalHopDto]) -> String {
    if hops.is_empty() {
        return "no links".to_owned();
    }
    hops.iter()
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
}

pub(crate) async fn supersede(
    app: &Application,
    old: String,
    by: String,
    if_match: Option<String>,
    out: &Output,
) -> CliResult<()> {
    let old_id = resolve::document(app, &old).await?;
    let new_id = resolve::document(app, &by).await?;
    let document = app
        .supersede_document
        .execute(SupersedeDocumentCommand {
            old_id,
            new_id,
            if_match,
        })
        .await?;
    out.emit(&document, |d| format!("{}  superseded", short(&d.id)))
}

pub(crate) async fn consolidate(
    app: &Application,
    sources: Vec<String>,
    into: String,
    out: &Output,
) -> CliResult<()> {
    let into = resolve::document(app, &into).await?;
    let mut resolved = Vec::new();
    for source in &sources {
        resolved.push(resolve::document(app, source).await?);
    }
    let response = app
        .consolidate_documents
        .execute(ConsolidateDocumentsCommand {
            sources: resolved,
            into,
        })
        .await?;
    out.emit(&response, |r| {
        format!(
            "{} consolidated into {}",
            r.superseded.len(),
            short(&r.into.id)
        )
    })
}

pub(crate) async fn update(
    app: &Application,
    target: String,
    mut command: UpdateDocumentCommand,
    out: &Output,
) -> CliResult<()> {
    command.document_id = resolve::document(app, &target).await?;
    let document = app.update_document.execute(command).await?;
    out.emit(&document, |d| detail(d, out))
}

pub(crate) async fn reject(
    app: &Application,
    target: String,
    reason: Option<String>,
    if_match: Option<String>,
    out: &Output,
) -> CliResult<()> {
    let document_id = resolve::document(app, &target).await?;
    let document = app
        .reject_document
        .execute(RejectDocumentCommand {
            document_id,
            reason,
            if_match,
        })
        .await?;
    out.emit(&document, |d| format!("{}  rejected", short(&d.id)))
}

pub(crate) async fn set_status(
    app: &Application,
    target: String,
    status: &str,
    if_match: Option<String>,
    out: &Output,
) -> CliResult<()> {
    let document_id = resolve::document(app, &target).await?;
    let document = app
        .update_document
        .execute(UpdateDocumentCommand {
            document_id,
            status: Some(status.to_owned()),
            if_match,
            ..Default::default()
        })
        .await?;
    out.emit(&document, |d| {
        format!("{}  {}", short(&d.id), d.status.as_deref().unwrap_or(""))
    })
}

pub(crate) async fn promote(
    app: &Application,
    mut command: PromoteDocumentCommand,
    out: &Output,
) -> CliResult<()> {
    command.document_id = resolve::document(app, &command.document_id).await?;
    let promoted = app.promote_document.execute(command).await?;
    out.emit(&promoted, |p| match &p.skill {
        Some(skill) => format!(
            "{}  promoted to skill `{}` in {}",
            short(&p.document.id),
            skill.name,
            skill.namespace
        ),
        None => format!(
            "{}  promoted to {}",
            short(&p.document.id),
            p.document.namespace
        ),
    })
}

fn render_hits(hits: &[HitDto], out: &Output) -> String {
    if hits.is_empty() {
        return "nothing found".to_owned();
    }
    hits.iter()
        .map(|h| {
            let heading = h.heading.as_deref().unwrap_or("(body)");
            let shown = match &h.text {
                Some(text) => text.clone(),
                None => h.excerpt.lines().next().unwrap_or("").to_owned(),
            };
            format!(
                "{}  {}  {}\n  {}",
                out.dim(short(&h.id)),
                out.dim(&h.kind),
                out.bold(heading),
                shown.replace('\n', "\n  ")
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
    lines.push(format!("  hash       {}", document.content_hash));
    lines.push(String::new());
    lines.push(document.body.clone());
    lines.join("\n")
}
