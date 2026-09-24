use std::path::Path;
use std::process::{Command, Output};

fn orchy(vault: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_orchy"))
        .args(args)
        .env("ORCHY_VAULT", vault)
        .env("XDG_CONFIG_HOME", vault.join(".config"))
        .env("ORCHY_ACTOR", "claude")
        .env("NO_COLOR", "1")
        .output()
        .expect("orchy binary runs")
}

fn ok(vault: &Path, args: &[&str]) -> String {
    let out = orchy(vault, args);
    assert!(
        out.status.success(),
        "`orchy {}` failed ({:?}): {}",
        args.join(" "),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn json(vault: &Path, args: &[&str]) -> serde_json::Value {
    let mut full = vec!["--json"];
    full.extend_from_slice(args);
    serde_json::from_str(&ok(vault, &full)).expect("valid json")
}

fn vault() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    temp
}

#[test]
fn a_command_against_an_uninitialised_directory_says_what_to_run() {
    let temp = tempfile::tempdir().unwrap();
    let out = orchy(temp.path(), &["task", "list"]);

    assert_eq!(out.status.code(), Some(4));
    let message = String::from_utf8_lossy(&out.stderr);
    assert!(message.contains("orchy init"), "{message}");
}

#[test]
fn init_is_idempotent_and_status_reports_the_resolved_vault() {
    let temp = vault();
    ok(temp.path(), &["init"]);

    let status = json(temp.path(), &["status"]);
    assert_eq!(status["initialised"], true);
    assert_eq!(
        status["vault"].as_str().unwrap(),
        temp.path().to_string_lossy()
    );
    assert!(
        status["actor"].as_str().unwrap().starts_with("claude@"),
        "a bare alias is completed with this machine's id"
    );
}

#[test]
fn a_task_moves_between_open_and_done_as_its_status_changes() {
    let temp = vault();
    let task = json(temp.path(), &["task", "new", "ship it"]);
    let id = task["id"].as_str().unwrap();

    assert!(temp.path().join(format!("tasks/open/{id}.md")).exists());

    ok(temp.path(), &["task", "claim", id]);
    ok(temp.path(), &["task", "done", id]);

    assert!(!temp.path().join(format!("tasks/open/{id}.md")).exists());
    let text = std::fs::read_to_string(temp.path().join(format!("tasks/done/{id}.md"))).unwrap();
    assert!(text.contains("status: completed"));
}

#[test]
fn completing_the_last_subtask_rolls_the_parent_up_on_disk() {
    let temp = vault();
    let parent = json(temp.path(), &["task", "new", "epic"]);
    let parent_id = parent["id"].as_str().unwrap().to_owned();

    let split = json(
        temp.path(),
        &["task", "split", &parent_id, "first", "second"],
    );
    let children: Vec<String> = split["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(children.len(), 2);

    for child in &children {
        ok(temp.path(), &["task", "claim", child]);
        ok(temp.path(), &["task", "done", child]);
    }

    let parent_after = json(temp.path(), &["task", "get", &parent_id]);
    assert_eq!(parent_after["task"]["status"], "completed");
    assert!(
        temp.path()
            .join(format!("tasks/done/{parent_id}.md"))
            .exists(),
        "the parent file follows its derived status"
    );
}

#[test]
fn a_refused_transition_exits_five_and_a_missing_entity_exits_four() {
    let temp = vault();
    let task = json(temp.path(), &["task", "new", "unclaimed"]);
    let id = task["id"].as_str().unwrap();

    let refused = orchy(temp.path(), &["task", "done", id]);
    assert_eq!(
        refused.status.code(),
        Some(5),
        "finishing unclaimed work is a domain refusal"
    );

    let missing = orchy(temp.path(), &["task", "get", "01BX5ZZKBKACTAV9WEVGEMMVRZ"]);
    assert_eq!(missing.status.code(), Some(4));
}

#[test]
fn an_ambiguous_address_is_refused_rather_than_guessed() {
    let temp = vault();
    ok(temp.path(), &["task", "new", "rotate the keys"]);
    ok(temp.path(), &["task", "new", "rotate the certs"]);

    let out = orchy(temp.path(), &["task", "get", "rotate"]);
    assert_eq!(out.status.code(), Some(7));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("matches 2"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_task_can_be_addressed_by_a_title_fragment_when_it_is_unique() {
    let temp = vault();
    let created = json(temp.path(), &["task", "new", "rotate the keys"]);
    let found = json(temp.path(), &["task", "get", "rotate"]);
    assert_eq!(found["task"]["id"], created["id"]);
}

#[test]
fn a_document_is_written_as_readable_markdown_and_keeps_author_fields() {
    let temp = vault();
    let document = json(
        temp.path(),
        &[
            "new",
            "decision",
            "Use RS256",
            "--namespace",
            "/backend",
            "--body",
            "# Decision\n\nMove to RS256.",
        ],
    );
    let id = document["id"].as_str().unwrap();
    let path = temp.path().join(format!("docs/backend/{id}.md"));

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("---\n"));
    assert!(text.contains("type: decision"));

    ok(temp.path(), &["set", id, "reviewer=alan"]);
    ok(
        temp.path(),
        &[
            "edit",
            id,
            "--section",
            "Decision",
            "--content",
            "Move to EdDSA.",
        ],
    );

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("reviewer: alan"), "{after}");
    assert!(after.contains("Move to EdDSA."));
    assert!(!after.contains("Move to RS256."));
}

#[test]
fn recall_finds_a_section_by_its_text() {
    let temp = vault();
    ok(
        temp.path(),
        &[
            "new",
            "decision",
            "Key rotation",
            "--body",
            "# Context\n\nWe rotate signing keys quarterly.",
        ],
    );
    let hits = json(temp.path(), &["recall", "quarterly"]);
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["heading"], "Context");
}

#[test]
fn a_broadcast_reaches_an_announced_agent_and_promotes_into_a_task() {
    let temp = vault();
    ok(temp.path(), &["announce", "--roles", "developer"]);

    let sent = Command::new(env!("CARGO_BIN_EXE_orchy"))
        .args([
            "--json",
            "msg",
            "send",
            "broadcast",
            "--subject",
            "build red",
            "--body",
            "master is failing",
        ])
        .env("ORCHY_VAULT", temp.path())
        .env("XDG_CONFIG_HOME", temp.path().join(".config"))
        .env("ORCHY_ACTOR", "codex")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(
        sent.status.success(),
        "{}",
        String::from_utf8_lossy(&sent.stderr)
    );
    let message: serde_json::Value = serde_json::from_slice(&sent.stdout).unwrap();
    let id = message["id"].as_str().unwrap();

    let inbox = json(temp.path(), &["msg", "inbox"]);
    assert_eq!(inbox.as_array().unwrap().len(), 1);

    let promoted = json(temp.path(), &["msg", "promote", id]);
    assert_eq!(promoted["task"]["title"], "build red");
    assert_eq!(promoted["message"]["status"], "resolved");
}

#[test]
fn reading_a_message_advances_the_watermark_so_the_inbox_empties() {
    let temp = vault();
    ok(temp.path(), &["announce"]);

    let sent = Command::new(env!("CARGO_BIN_EXE_orchy"))
        .args(["--json", "msg", "send", "@claude", "--body", "ping"])
        .env("ORCHY_VAULT", temp.path())
        .env("XDG_CONFIG_HOME", temp.path().join(".config"))
        .env("ORCHY_ACTOR", "codex")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    let message: serde_json::Value = serde_json::from_slice(&sent.stdout).unwrap();

    assert_eq!(
        json(temp.path(), &["msg", "inbox"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ok(
        temp.path(),
        &["msg", "read", message["id"].as_str().unwrap()],
    );
    assert!(
        json(temp.path(), &["msg", "inbox"])
            .as_array()
            .unwrap()
            .is_empty(),
        "the watermark is per actor and machine-local"
    );
    assert!(
        !json(temp.path(), &["msg", "inbox", "--all"])
            .as_array()
            .unwrap()
            .is_empty(),
        "--all ignores the watermark"
    );
}

#[test]
fn a_lock_is_refused_to_a_second_holder_and_released_by_the_first() {
    let temp = vault();
    ok(temp.path(), &["lock", "acquire", "build"]);

    let contended = Command::new(env!("CARGO_BIN_EXE_orchy"))
        .args(["lock", "acquire", "build"])
        .env("ORCHY_VAULT", temp.path())
        .env("XDG_CONFIG_HOME", temp.path().join(".config"))
        .env("ORCHY_ACTOR", "codex")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert_eq!(contended.status.code(), Some(5));

    ok(temp.path(), &["lock", "release", "build"]);
    assert!(json(temp.path(), &["lock", "check", "build"]).is_null());
}

#[test]
fn every_write_lands_in_the_event_log() {
    let temp = vault();
    let task = json(temp.path(), &["task", "new", "logged"]);
    ok(
        temp.path(),
        &["task", "claim", task["id"].as_str().unwrap()],
    );

    let events = json(temp.path(), &["events"]);
    let topics: Vec<&str> = events
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["topic"].as_str().unwrap())
        .collect();
    assert!(topics.contains(&"task.created"), "{topics:?}");
    assert!(topics.contains(&"task.claimed"), "{topics:?}");
}

#[test]
fn set_refuses_a_semantic_field_and_names_the_command_to_use() {
    let temp = vault();
    let document = json(temp.path(), &["new", "note", "n", "--body", "x"]);
    let out = orchy(
        temp.path(),
        &["set", document["id"].as_str().unwrap(), "status=archived"],
    );

    assert_eq!(out.status.code(), Some(5));
    let message = String::from_utf8_lossy(&out.stderr);
    assert!(message.contains("orchy supersede"), "{message}");
}

#[test]
fn completions_are_generated_without_a_vault() {
    let temp = tempfile::tempdir().unwrap();
    let script = ok(temp.path(), &["completions", "fish"]);
    assert!(script.contains("orchy"), "a fish completion script");
}
