use orchy_application::Application;
use orchy_application::announce_actor::AnnounceActorCommand;
use orchy_application::brief::BriefCommand;
use orchy_application::dto::BriefingDto;

use crate::cmd::skill::summarise;
use crate::error::CliResult;
use crate::output::{Output, short};

const WHAT_ORCHY_IS: &str = "\
ORCHY IN ONE MINUTE
  A shared memory for agents, kept as markdown files you can read, edit and commit.
  Frontmatter is the only source of truth; a file's directory is a projection of it,
  so never infer state from a path. Every entity has a stable id: move files freely.

  docs/      what the team knows      tasks/     work with owners and state
  skills/    how this team works      messages/  the board agents post to

WHAT TO RUN
  orchy task next                     claim the highest-ranked task
  orchy task done <id> --note ...     finish it, saying what you did
  orchy recall <query>                search what the team already knows
  orchy new decision <title>          write down a choice and why
  orchy skill show <name>             read a convention in full
  orchy msg send broadcast --body     tell everyone
  orchy lock with <res> -- <cmd>      hold something for one command, no matter how it ends

BEFORE YOU STOP
  orchy new context handoff --body    what you did, what is left, what to watch out for";

pub(crate) async fn announce(
    app: &Application,
    actor: &str,
    roles: Vec<String>,
    namespace: Option<String>,
    name: Option<String>,
    out: &Output,
) -> CliResult<()> {
    app.announce_actor
        .execute(AnnounceActorCommand {
            actor: actor.to_owned(),
            roles,
            namespace,
            display_name: name,
        })
        .await?;

    let briefing = app
        .brief
        .execute(BriefCommand {
            actor: actor.to_owned(),
        })
        .await?;
    out.emit(&briefing, render)
}

pub(crate) fn guide(out: &Output) -> CliResult<()> {
    out.emit(&WHAT_ORCHY_IS, |text| (*text).to_owned())
}

fn render(briefing: &BriefingDto) -> String {
    let mut lines = vec![
        format!(
            "You are {} in {}",
            briefing.actor.id, briefing.actor.namespace
        ),
        String::new(),
        WHAT_ORCHY_IS.to_owned(),
        String::new(),
    ];

    lines.push(skills(briefing));
    lines.push(String::new());
    lines.push(waiting(briefing));

    if let Some(handoff) = &briefing.handoff {
        lines.push(String::new());
        lines.push(format!(
            "HANDOFF FROM THE LAST SESSION\n  {}  {}\n  orchy read {}",
            short(&handoff.id),
            handoff.title,
            short(&handoff.id)
        ));
    }
    lines.join("\n")
}

fn skills(briefing: &BriefingDto) -> String {
    if briefing.skills.is_empty() {
        return "SKILLS\n  none yet. `orchy skill write <name> --summary ...` records how this \
                team works."
            .to_owned();
    }
    let mut out = vec![format!(
        "SKILLS IN FORCE HERE ({}) — follow these; `orchy skill show <name>` for the detail",
        briefing.skills.len()
    )];
    out.extend(briefing.skills.iter().map(summarise));
    out.join("\n")
}

fn waiting(briefing: &BriefingDto) -> String {
    let mut out = vec!["WAITING FOR YOU".to_owned()];
    out.push(match briefing.unread {
        0 => "  no unread messages".to_owned(),
        n => format!("  {n} unread — orchy msg inbox"),
    });

    if briefing.claimed.is_empty() {
        out.push("  nothing claimed".to_owned());
    } else {
        out.push(format!("  {} already yours:", briefing.claimed.len()));
        out.extend(
            briefing
                .claimed
                .iter()
                .map(|t| format!("    {}  {}  ({})", short(&t.id), t.title, t.status)),
        );
    }

    match &briefing.next {
        Some(task) => out.push(format!(
            "  next up: {}  {} — orchy task next",
            short(&task.id),
            task.title
        )),
        None => out.push("  no unclaimed work here".to_owned()),
    }
    out.join("\n")
}
