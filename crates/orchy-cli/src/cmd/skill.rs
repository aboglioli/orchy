use orchy_application::Application;
use orchy_application::dto::SkillDto;
use orchy_application::list_skills::ListSkillsCommand;
use orchy_application::read_skill::ReadSkillCommand;
use orchy_application::retire_skill::RetireSkillCommand;
use orchy_application::write_skill::WriteSkillCommand;

use crate::error::CliResult;
use crate::output::Output;
use crate::stdin;

pub(crate) async fn write(
    app: &Application,
    name: String,
    summary: Option<String>,
    namespace: Option<String>,
    body: Option<String>,
    out: &Output,
) -> CliResult<()> {
    let skill = app
        .write_skill
        .execute(WriteSkillCommand {
            name,
            summary,
            namespace,
            body: piped(body)?,
        })
        .await?;
    out.emit(&skill, |s| format!("{}  {}", s.name, s.summary))
}

pub(crate) async fn list(
    app: &Application,
    namespace: Option<String>,
    everywhere: bool,
    retired: bool,
    out: &Output,
) -> CliResult<()> {
    let skills = app
        .list_skills
        .execute(ListSkillsCommand {
            namespace,
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

/// Retiring names a particular skill rather than whichever one a namespace inherits, so the
/// name is resolved to its id first.
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

/// A revision that only changes the summary has no body at all, so unlike `orchy new` this
/// never reads stdin on its own: `--body -` is how a caller asks for it.
fn piped(body: Option<String>) -> CliResult<Option<String>> {
    match body.as_deref() {
        Some("-") => stdin::or_read(None).map(Some),
        _ => Ok(body),
    }
}

pub(crate) fn summarise(skill: &SkillDto) -> String {
    format!("  {:<24}{}", skill.name, skill.summary)
}
