use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::error::{CliError, CliResult};

const START: &str = "<!-- orchy:start -->";
const END: &str = "<!-- orchy:end -->";
const SESSION_START: &str = "SessionStart";
const EVERY_SESSION: &str = "startup|resume|clear|compact";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Agent {
    ClaudeCode,
    Codex,
    Opencode,
    Gemini,
}

pub(crate) struct Change {
    pub path: PathBuf,
    pub contents: String,
}

pub(crate) fn plan(agent: Agent, repo: &Path, announce: &str) -> CliResult<Change> {
    match agent {
        Agent::ClaudeCode => claude_settings(&repo.join(".claude/settings.json"), announce),
        Agent::Codex | Agent::Opencode => instructions(&repo.join("AGENTS.md"), announce),
        Agent::Gemini => instructions(&repo.join("GEMINI.md"), announce),
    }
}

pub(crate) fn announce_command(namespace: Option<&str>, roles: &[String]) -> String {
    let mut command = "orchy announce".to_owned();
    if let Some(namespace) = namespace {
        command.push_str(&format!(" --namespace {namespace}"));
    }
    for role in roles {
        command.push_str(&format!(" --roles {role}"));
    }
    command
}

/// The hook's stdout, the briefing, is what Claude Code adds to the session's context.
fn claude_settings(path: &Path, announce: &str) -> CliResult<Change> {
    let mut settings: Value = match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| CliError::config(format!("{}: {e}", path.display())))?,
        Err(_) => json!({}),
    };
    let blocks = settings
        .as_object_mut()
        .ok_or_else(|| CliError::config(format!("{} is not a JSON object", path.display())))?
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| CliError::config("`hooks` is not an object"))?
        .entry(SESSION_START)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| CliError::config("`hooks.SessionStart` is not an array"))?;

    for block in blocks.iter_mut() {
        if let Some(hooks) = block.get_mut("hooks").and_then(Value::as_array_mut) {
            hooks.retain(|h| {
                !h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.starts_with("orchy announce"))
            });
        }
    }
    blocks.retain(|block| {
        block
            .get("hooks")
            .and_then(Value::as_array)
            .is_none_or(|hooks| !hooks.is_empty())
    });
    blocks.push(json!({
        "matcher": EVERY_SESSION,
        "hooks": [{ "type": "command", "command": announce }],
    }));

    let contents = serde_json::to_string_pretty(&settings)
        .map_err(|e| CliError::config(format!("writing settings: {e}")))?;
    Ok(Change {
        path: path.to_owned(),
        contents: format!("{contents}\n"),
    })
}

fn instructions(path: &Path, announce: &str) -> CliResult<Change> {
    let block = format!(
        "{START}\n## orchy\n\nThis project coordinates its agents through orchy. Before starting any work, run\n`{announce}` and follow the briefing it prints: the skills in force, the task waiting for\nyou, and the handoff from the last session. It also gives you a session token\n(`ses_...`): run every later orchy command with `ORCHY_SESSION=<token>` set, or with\n`--session <token>`, so orchy knows it is you. Before stopping, record what is left with\n`orchy new context handoff --body ...`.\n{END}\n"
    );
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let contents = match (existing.find(START), existing.find(END)) {
        (Some(start), Some(end)) if start < end => {
            format!(
                "{}{block}{}",
                &existing[..start],
                existing[end + END.len()..].trim_start_matches('\n')
            )
        }
        _ if existing.trim().is_empty() => block,
        _ => format!("{}\n\n{block}", existing.trim_end()),
    };
    Ok(Change {
        path: path.to_owned(),
        contents,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_settings_keep_what_is_there_and_hold_one_orchy_hook() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".claude/settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"model":"opus","hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"orchy announce --namespace /old"}]}],"Stop":[]}}"#,
        )
        .unwrap();

        let change = claude_settings(&path, "orchy announce --namespace /backend").unwrap();
        let settings: Value = serde_json::from_str(&change.contents).unwrap();
        assert_eq!(settings["model"], "opus");
        assert!(settings["hooks"]["Stop"].is_array());
        let starts = settings["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(
            starts.len(),
            1,
            "the old orchy hook was replaced, not duplicated"
        );
        assert_eq!(
            starts[0]["hooks"][0]["command"],
            "orchy announce --namespace /backend"
        );
    }

    #[test]
    fn instructions_are_one_block_updated_in_place() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("AGENTS.md");
        std::fs::write(&path, "# Project\n\nOur rules.\n").unwrap();

        let first = instructions(&path, "orchy announce").unwrap();
        std::fs::write(&path, &first.contents).unwrap();
        let second = instructions(&path, "orchy announce --roles dev").unwrap();

        assert!(second.contents.starts_with("# Project\n\nOur rules.\n"));
        assert_eq!(second.contents.matches(START).count(), 1);
        assert!(second.contents.contains("orchy announce --roles dev"));
        assert!(second.contents.contains("ORCHY_SESSION"));
    }
}
