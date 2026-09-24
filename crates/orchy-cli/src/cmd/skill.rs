use orchy_application::Application;
use orchy_application::dto::SkillDto;
use orchy_application::list_skills::ListSkillsCommand;
use orchy_application::read_skill::ReadSkillCommand;
use orchy_application::recall::RecallCommand;
use orchy_application::retire_skill::RetireSkillCommand;
use orchy_application::set_skill_field::SetSkillFieldCommand;
use orchy_application::write_skill::WriteSkillCommand;

use crate::cli::SkillEdits;
use crate::error::{CliError, CliResult};
use crate::output::Output;
use crate::stdin;

pub(crate) async fn write(
    app: &Application,
    name: String,
    summary: Option<String>,
    namespace: Option<String>,
    body: Option<String>,
    tag: Vec<String>,
    out: &Output,
) -> CliResult<()> {
    let skill = app
        .write_skill
        .execute(WriteSkillCommand {
            name: name.clone(),
            summary,
            namespace: namespace.clone(),
            body: piped(body)?,
        })
        .await?;
    if tag.is_empty() {
        return out.emit(&skill, |s| format!("{}  {}", s.name, s.summary));
    }

    let tagged = app
        .set_skill_field
        .execute(SetSkillFieldCommand {
            target: name,
            namespace,
            tag,
            ..Default::default()
        })
        .await?;
    out.emit(&tagged, |s| format!("{}  {}", s.name, s.summary))
}

pub(crate) async fn set(
    app: &Application,
    target: String,
    namespace: Option<String>,
    edits: SkillEdits,
    out: &Output,
) -> CliResult<()> {
    let mut fields = Vec::new();
    for assignment in &edits.assignments {
        let (field, value) = assignment
            .split_once('=')
            .ok_or_else(|| CliError::config(format!("`{assignment}` is not field=value")))?;
        let parsed = serde_json::from_str(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.to_owned()));
        fields.push((field.to_owned(), parsed));
    }

    let skill = app
        .set_skill_field
        .execute(SetSkillFieldCommand {
            target,
            namespace,
            fields,
            remove: edits.remove,
            tag: edits.tag,
            untag: edits.untag,
        })
        .await?;
    out.emit(&skill, |s| format!("{}  updated", s.name))
}

pub(crate) async fn list(
    app: &Application,
    namespace: Option<String>,
    tag: Vec<String>,
    everywhere: bool,
    retired: bool,
    out: &Output,
) -> CliResult<()> {
    let skills = app
        .list_skills
        .execute(ListSkillsCommand {
            namespace,
            tags: tag,
            everywhere,
            retired,
        })
        .await?;
    out.emit(&skills, |list| {
        if list.is_empty() {
            return "no skills yet: `orchy skill write <name> --summary ...`".to_owned();
        }
        list.iter().map(summarise).collect::<Vec<_>>().join("\n")
    })
}

pub(crate) async fn find(
    app: &Application,
    query: Vec<String>,
    namespace: Option<String>,
    tag: Vec<String>,
    retired: bool,
    limit: Option<usize>,
    out: &Output,
) -> CliResult<()> {
    let hits = app
        .recall
        .execute(RecallCommand {
            text: query.join(" "),
            entities: vec!["skill".to_owned()],
            retired,
            tags: tag,
            anchor: namespace,
            limit,
            ..Default::default()
        })
        .await?;

    out.emit(&hits, |found| {
        if found.is_empty() {
            return "no skill matches that".to_owned();
        }
        found
            .iter()
            .map(|h| {
                format!(
                    "  {:<24}{}",
                    h.heading.as_deref().unwrap_or(""),
                    h.excerpt.lines().next().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

pub(crate) async fn show(
    app: &Application,
    target: String,
    namespace: Option<String>,
    out: &Output,
) -> CliResult<()> {
    let skill = app
        .read_skill
        .execute(ReadSkillCommand { target, namespace })
        .await?;
    out.emit(&skill, |s| {
        format!(
            "{}  {}\n  namespace {}\n  status    {}\n\n{}",
            s.name, s.summary, s.namespace, s.status, s.body
        )
    })
}

pub(crate) async fn retire(
    app: &Application,
    target: String,
    restore: bool,
    out: &Output,
) -> CliResult<()> {
    let id = resolve(app, &target).await?;
    let skill = app
        .retire_skill
        .execute(RetireSkillCommand {
            skill_id: id,
            restore,
        })
        .await?;
    out.emit(&skill, |s| format!("{} is {}", s.name, s.status))
}

async fn resolve(app: &Application, target: &str) -> CliResult<String> {
    Ok(app
        .read_skill
        .execute(ReadSkillCommand {
            target: target.to_owned(),
            namespace: None,
        })
        .await?
        .id)
}

fn piped(body: Option<String>) -> CliResult<Option<String>> {
    match body.as_deref() {
        Some("-") => stdin::or_read(None).map(Some),
        _ => Ok(body),
    }
}

pub(crate) fn summarise(skill: &SkillDto) -> String {
    format!("  {:<24}{}", skill.name, skill.summary)
}
