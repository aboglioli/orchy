use std::collections::BTreeSet;
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

fn seeded_for_recall() -> tempfile::TempDir {
    let temp = vault();
    ok(
        temp.path(),
        &[
            "new",
            "decision",
            "Never edit an applied migration",
            "--body",
            "A long migration holds the lock and blocks every deploy.",
        ],
    );
    ok(
        temp.path(),
        &[
            "new",
            "note",
            "Frontend tokens",
            "--body",
            "Design tokens are generated at build time.",
        ],
    );
    temp
}

#[test]
fn recall_finds_a_word_through_its_inflections() {
    let temp = seeded_for_recall();

    for query in ["migration", "migrations", "migrate", "MIGRATING"] {
        let hits = json(temp.path(), &["recall", query]);
        assert_eq!(
            hits.as_array().unwrap().len(),
            1,
            "`{query}` should reach the same document"
        );
    }
}

#[test]
fn recall_matches_words_that_are_not_next_to_each_other() {
    let temp = seeded_for_recall();

    for query in [
        vec!["recall", "deploy", "lock"],
        vec!["recall", "lock", "deploy"],
        vec!["recall", "blocks", "migration", "deploy"],
    ] {
        let hits = json(temp.path(), &query);
        assert_eq!(
            hits.as_array().unwrap().len(),
            1,
            "{query:?} are all words in the document, in some order"
        );
    }
}

#[test]
fn recall_leaves_out_what_carries_none_of_the_query() {
    let temp = seeded_for_recall();
    let hits = json(temp.path(), &["recall", "migration"]);

    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert!(
        !hits[0]["excerpt"]
            .as_str()
            .unwrap()
            .contains("Design tokens"),
        "an unrelated document is not a weak match, it is not a match"
    );
}

#[test]
fn a_hit_carries_how_relevant_it_was_so_a_caller_can_judge() {
    let temp = seeded_for_recall();
    let hits = json(temp.path(), &["recall", "migration"]);

    assert!(
        hits[0]["relevance"].as_f64().unwrap() > 0.0,
        "relevance is reported, not just an order: {hits}"
    );
}

#[test]
fn the_document_a_query_is_most_about_comes_first() {
    let temp = vault();
    ok(
        temp.path(),
        &[
            "new",
            "note",
            "Migration safety",
            "--body",
            "Never edit an applied migration.",
        ],
    );
    ok(
        temp.path(),
        &[
            "new",
            "note",
            "Weekly notes",
            "--body",
            "We talked about the migration in passing.",
        ],
    );

    let hits = json(temp.path(), &["recall", "migration"]);
    assert!(
        hits[0]["excerpt"]
            .as_str()
            .unwrap()
            .contains("Never edit an applied"),
        "the document titled for the subject outranks one that mentions it: {hits}"
    );
}

fn as_actor(vault: &Path, actor: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_orchy"))
        .args(args)
        .env("ORCHY_VAULT", vault)
        .env("XDG_CONFIG_HOME", vault.join(".config"))
        .env("ORCHY_ACTOR", actor)
        .env("NO_COLOR", "1")
        .output()
        .expect("orchy binary runs")
}

#[test]
fn a_skill_is_filed_by_name_where_a_human_would_look_for_it() {
    let temp = vault();
    ok(
        temp.path(),
        &[
            "skill",
            "write",
            "migrations",
            "--summary",
            "never edit an applied migration",
            "--namespace",
            "/backend",
            "--body",
            "add a new one instead",
        ],
    );

    assert!(
        temp.path().join("skills/backend/migrations.md").is_file(),
        "a skill is addressed by name, so it is filed under one"
    );
    let shown = ok(temp.path(), &["skill", "show", "migrations"]);
    assert!(shown.contains("add a new one instead"));
}

#[test]
fn writing_a_skill_twice_revises_it_rather_than_duplicating_it() {
    let temp = vault();
    ok(
        temp.path(),
        &["skill", "write", "review", "--summary", "first attempt"],
    );
    ok(
        temp.path(),
        &[
            "skill",
            "write",
            "review",
            "--summary",
            "what we settled on",
        ],
    );

    let listed = json(temp.path(), &["skill", "list"]);
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["summary"], "what we settled on");
}

#[test]
fn a_new_skill_without_a_summary_is_refused() {
    let temp = vault();
    let refused = orchy(temp.path(), &["skill", "write", "nameless", "--body", "x"]);
    assert_eq!(refused.status.code(), Some(6));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("summary"),
        "the refusal says what is missing"
    );
}

#[test]
fn skills_are_inherited_downward_and_the_nearest_one_wins() {
    let temp = vault();
    ok(
        temp.path(),
        &["skill", "write", "review", "--summary", "the house rule"],
    );
    ok(
        temp.path(),
        &[
            "skill",
            "write",
            "review",
            "--summary",
            "what backend does instead",
            "--namespace",
            "/backend",
        ],
    );
    ok(
        temp.path(),
        &[
            "skill",
            "write",
            "migrations",
            "--summary",
            "backend only",
            "--namespace",
            "/backend",
        ],
    );

    let at_root = json(temp.path(), &["skill", "list"]);
    assert_eq!(
        at_root.as_array().unwrap().len(),
        1,
        "root inherits nothing"
    );
    assert_eq!(at_root[0]["summary"], "the house rule");

    let in_backend = json(temp.path(), &["skill", "list", "--namespace", "/backend"]);
    let summaries: Vec<&str> = in_backend
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["summary"].as_str().unwrap())
        .collect();
    assert_eq!(summaries, vec!["backend only", "what backend does instead"]);
}

#[test]
fn a_retired_skill_stays_readable_but_teaches_nobody() {
    let temp = vault();
    ok(
        temp.path(),
        &["skill", "write", "old-way", "--summary", "how we used to"],
    );
    ok(temp.path(), &["skill", "retire", "old-way"]);

    assert!(
        json(temp.path(), &["skill", "list"])
            .as_array()
            .unwrap()
            .is_empty(),
        "a retired skill is in force nowhere"
    );
    assert_eq!(
        json(temp.path(), &["skill", "list", "--retired"])
            .as_array()
            .unwrap()
            .len(),
        1,
        "but it is still there when asked for"
    );

    ok(temp.path(), &["skill", "restore", "old-way"]);
    assert_eq!(
        json(temp.path(), &["skill", "list"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn announcing_returns_the_briefing_an_agent_needs_to_start() {
    let temp = vault();
    ok(
        temp.path(),
        &[
            "skill",
            "write",
            "code-review",
            "--summary",
            "two approvals, always",
        ],
    );
    ok(temp.path(), &["task", "new", "wire up auth"]);
    ok(temp.path(), &["announce"]);
    as_actor(temp.path(), "codex", &["announce"]);
    ok(
        temp.path(),
        &["msg", "send", "@codex", "--body", "heads up"],
    );

    let briefing =
        String::from_utf8_lossy(&as_actor(temp.path(), "codex", &["announce"]).stdout).into_owned();

    assert!(briefing.contains("ORCHY IN ONE MINUTE"), "{briefing}");
    assert!(
        briefing.contains("two approvals, always"),
        "the skills in force are summarised, not just counted: {briefing}"
    );
    assert!(briefing.contains("1 unread"), "{briefing}");
    assert!(briefing.contains("wire up auth"), "{briefing}");
}

#[test]
fn the_briefing_carries_a_summary_per_skill_rather_than_the_skills_themselves() {
    let temp = vault();
    for n in 0..30 {
        ok(
            temp.path(),
            &[
                "skill",
                "write",
                &format!("convention-{n}"),
                "--summary",
                &format!("the {n}th thing to know"),
                "--body",
                "a long body nobody wants inlined thirty times over",
            ],
        );
    }
    ok(temp.path(), &["announce"]);

    let briefing = ok(temp.path(), &["announce"]);
    assert!(briefing.contains("SKILLS IN FORCE HERE (30)"));
    assert!(briefing.contains("the 29th thing to know"));
    assert!(
        !briefing.contains("a long body nobody wants inlined"),
        "hundreds of skills have to stay scannable: the body is behind `skill show`"
    );
}

#[test]
fn the_guide_explains_orchy_without_joining_the_roster() {
    let temp = vault();
    let guide = ok(temp.path(), &["guide"]);

    assert!(guide.contains("ORCHY IN ONE MINUTE"));
    assert!(
        json(temp.path(), &["agents"])
            .as_array()
            .unwrap()
            .is_empty(),
        "reading the manual is not announcing yourself"
    );
}

#[test]
fn a_team_puts_its_own_frontmatter_on_a_skill_and_orchy_keeps_it() {
    let temp = vault();
    ok(
        temp.path(),
        &[
            "skill",
            "write",
            "migrations",
            "--summary",
            "never edit one",
        ],
    );
    ok(
        temp.path(),
        &[
            "skill",
            "set",
            "migrations",
            "owner=platform-team",
            "review_by=2027-01-01",
            "risk=3",
        ],
    );

    let shown = json(temp.path(), &["skill", "show", "migrations"]);
    assert_eq!(shown["frontmatter"]["owner"], "platform-team");
    assert_eq!(shown["frontmatter"]["risk"], 3, "a number stays a number");

    ok(
        temp.path(),
        &["skill", "write", "migrations", "--summary", "revised"],
    );
    let revised = json(temp.path(), &["skill", "show", "migrations"]);
    assert_eq!(
        revised["frontmatter"]["owner"], "platform-team",
        "revising the skill does not discard what the team wrote on it"
    );

    ok(
        temp.path(),
        &["skill", "set", "migrations", "--remove", "risk"],
    );
    assert!(
        json(temp.path(), &["skill", "show", "migrations"])["frontmatter"]
            .get("risk")
            .is_none()
    );
}

#[test]
fn the_fields_orchy_maintains_are_refused_by_name() {
    let temp = vault();
    ok(temp.path(), &["skill", "write", "review", "--summary", "x"]);

    for (field, hint) in [
        ("status=retired", "orchy skill retire"),
        ("name=other", "under the new name"),
        ("summary=sneaky", "--summary"),
        ("id=01ARZ3NDEKTSV4RRFFQ69G5FAV", "immutable"),
    ] {
        let refused = orchy(temp.path(), &["skill", "set", "review", field]);
        assert_eq!(
            refused.status.code(),
            Some(5),
            "`{field}` should be refused"
        );
        let message = String::from_utf8_lossy(&refused.stderr);
        assert!(
            message.contains(hint),
            "the refusal names the command that does change it: {message}"
        );
    }
}

#[test]
fn skills_carry_tags_and_can_be_listed_by_them() {
    let temp = vault();
    ok(
        temp.path(),
        &[
            "skill",
            "write",
            "migrations",
            "--summary",
            "one",
            "--tag",
            "database",
            "--tag",
            "safety",
        ],
    );
    ok(
        temp.path(),
        &[
            "skill",
            "write",
            "style",
            "--summary",
            "two",
            "--tag",
            "formatting",
        ],
    );

    let tagged = json(temp.path(), &["skill", "list", "--tag", "database"]);
    assert_eq!(tagged.as_array().unwrap().len(), 1);
    assert_eq!(tagged[0]["name"], "migrations");

    ok(
        temp.path(),
        &["skill", "set", "migrations", "--untag", "safety"],
    );
    let left = json(temp.path(), &["skill", "show", "migrations"]);
    assert_eq!(left["tags"], serde_json::json!(["database"]));
}

#[test]
fn an_agent_that_runs_orchy_with_no_arguments_is_told_where_to_start() {
    let temp = vault();
    let bare = orchy(temp.path(), &[]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&bare.stdout),
        String::from_utf8_lossy(&bare.stderr)
    );

    assert!(
        text.contains("orchy announce"),
        "the one command an agent must run has to be in the first thing it reads: {text}"
    );
    assert!(
        text.contains("EXIT CODES"),
        "so an agent can branch: {text}"
    );
    assert!(text.contains("WHERE THINGS LIVE"), "{text}");
}

#[test]
fn orchy_help_says_the_same_thing_as_running_it_bare() {
    let temp = vault();
    let helped = String::from_utf8_lossy(&orchy(temp.path(), &["help"]).stdout).into_owned();

    assert!(helped.contains("orchy announce"));
    assert!(helped.contains("skills"), "the pillars are named: {helped}");
}

fn seeded_for_search() -> tempfile::TempDir {
    let temp = vault();
    ok(
        temp.path(),
        &[
            "skill",
            "write",
            "migrations",
            "--namespace",
            "/backend",
            "--summary",
            "never edit an applied migration; add a new one",
            "--body",
            "Rolling back in place corrupts every environment that already ran it.",
        ],
    );
    ok(
        temp.path(),
        &[
            "skill",
            "write",
            "db-pooling",
            "--namespace",
            "/backend",
            "--summary",
            "one pool per process, never per request",
            "--body",
            "Connection churn is the usual cause of migration timeouts.",
        ],
    );
    ok(
        temp.path(),
        &[
            "new",
            "discovery",
            "migration lock timeout",
            "--body",
            "A long migration holds the lock and blocks deploys.",
        ],
    );
    temp
}

#[test]
fn a_skill_is_found_by_text_in_its_name_summary_or_body() {
    let temp = seeded_for_search();
    let found = json(temp.path(), &["skill", "find", "migration"]);

    let names: Vec<&str> = found
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["heading"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec!["migrations", "db-pooling"],
        "the skill that matches in its name and summary outranks one that only mentions it \
         in passing: {names:?}"
    );
    assert_eq!(
        found[0]["excerpt"], "never edit an applied migration; add a new one",
        "a hit carries the line that tells an agent whether to open it"
    );
}

#[test]
fn finding_a_skill_never_returns_one_that_is_out_of_force() {
    let temp = seeded_for_search();
    ok(
        temp.path(),
        &[
            "skill",
            "write",
            "old-way",
            "--summary",
            "how we used to run migrations",
        ],
    );
    ok(temp.path(), &["skill", "retire", "old-way"]);

    let found = ok(temp.path(), &["skill", "find", "migrations"]);
    assert!(
        !found.contains("old-way"),
        "a retired skill must not be offered as something to follow: {found}"
    );
    assert!(
        ok(temp.path(), &["skill", "find", "migrations", "--retired"]).contains("old-way"),
        "but it is still searchable when asked for"
    );
}

#[test]
fn recall_searches_documents_and_skills_together_and_says_which_is_which() {
    let temp = seeded_for_search();
    let hits = json(temp.path(), &["recall", "migration"]);

    let kinds: BTreeSet<&str> = hits
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        BTreeSet::from(["document", "skill"]),
        "one search covers what the team knows and how it works: {kinds:?}"
    );
    for hit in hits.as_array().unwrap() {
        assert!(
            hit["entity"].as_str().unwrap().contains(':'),
            "a hit is addressable as kind:id so it can be read back"
        );
    }
}

#[test]
fn recall_can_be_narrowed_to_one_kind_of_entity() {
    let temp = seeded_for_search();

    let only_skills = json(temp.path(), &["recall", "migration", "--entity", "skill"]);
    assert!(
        only_skills
            .as_array()
            .unwrap()
            .iter()
            .all(|h| h["kind"] == "skill"),
        "{only_skills}"
    );

    let only_docs = json(
        temp.path(),
        &["recall", "migration", "--entity", "document"],
    );
    assert!(
        only_docs
            .as_array()
            .unwrap()
            .iter()
            .all(|h| h["kind"] == "document"),
        "{only_docs}"
    );
    assert!(!only_docs.as_array().unwrap().is_empty());
}

#[test]
fn a_skill_declared_where_the_agent_works_is_ranked_first() {
    let temp = vault();
    for (name, namespace) in [("deploy-checks", "/frontend"), ("deploy-steps", "/backend")] {
        ok(
            temp.path(),
            &[
                "skill",
                "write",
                name,
                "--namespace",
                namespace,
                "--summary",
                "how deploys work here",
            ],
        );
    }

    let found = json(
        temp.path(),
        &["skill", "find", "deploy", "--namespace", "/backend"],
    );
    assert_eq!(
        found[0]["heading"], "deploy-steps",
        "equal matches break towards the namespace the agent is standing in: {found}"
    );
}

#[test]
fn a_lock_is_renewed_by_its_holder_and_by_nobody_else() {
    let temp = vault();
    ok(temp.path(), &["lock", "acquire", "build", "--ttl", "60"]);
    let taken = json(temp.path(), &["lock", "check", "build"]);

    let renewed = json(temp.path(), &["lock", "renew", "build", "--ttl", "600"]);
    assert_eq!(
        renewed["generation"], taken["generation"],
        "renewing is not re-taking, so the fencing token stands"
    );
    assert!(renewed["expires_at"].as_str() > taken["expires_at"].as_str());

    let stolen = Command::new(env!("CARGO_BIN_EXE_orchy"))
        .args(["lock", "renew", "build"])
        .env("ORCHY_VAULT", temp.path())
        .env("XDG_CONFIG_HOME", temp.path().join(".config"))
        .env("ORCHY_ACTOR", "codex")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert_eq!(stolen.status.code(), Some(5));
}

#[test]
fn resources_that_sanitise_alike_are_still_two_locks() {
    let temp = vault();
    ok(temp.path(), &["lock", "acquire", "deploy/prod"]);

    let other = Command::new(env!("CARGO_BIN_EXE_orchy"))
        .args(["lock", "acquire", "deploy-prod"])
        .env("ORCHY_VAULT", temp.path())
        .env("XDG_CONFIG_HOME", temp.path().join(".config"))
        .env("ORCHY_ACTOR", "codex")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(
        other.status.success(),
        "`deploy-prod` is not `deploy/prod`: {}",
        String::from_utf8_lossy(&other.stderr)
    );
}

#[test]
fn list_shows_what_is_held_and_drops_what_has_lapsed() {
    let temp = vault();
    ok(temp.path(), &["lock", "acquire", "alpha", "--ttl", "600"]);
    ok(temp.path(), &["lock", "acquire", "brief", "--ttl", "1"]);
    std::thread::sleep(std::time::Duration::from_millis(1200));

    let held = json(temp.path(), &["lock", "list"]);
    let names: Vec<&str> = held
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["resource"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["alpha"]);
}

#[test]
fn with_holds_a_resource_for_one_command_and_gives_it_back_either_way() {
    let temp = vault();

    ok(temp.path(), &["lock", "with", "deploy", "--", "true"]);
    assert!(
        json(temp.path(), &["lock", "check", "deploy"]).is_null(),
        "a lease taken for a command does not outlive it"
    );

    let failed = orchy(temp.path(), &["lock", "with", "deploy", "--", "false"]);
    assert_eq!(
        failed.status.code(),
        Some(1),
        "the command's own exit code reaches the caller"
    );
    assert!(
        json(temp.path(), &["lock", "check", "deploy"]).is_null(),
        "and a command that fails still gives the resource back"
    );

    let unstartable = orchy(
        temp.path(),
        &["lock", "with", "deploy", "--", "/nope/nothing"],
    );
    assert!(!unstartable.status.success());
    assert!(
        json(temp.path(), &["lock", "check", "deploy"]).is_null(),
        "so does one that never started"
    );
}

#[test]
fn a_lease_that_has_already_lapsed_is_refused_where_it_is_typed() {
    let temp = vault();
    let refused = orchy(temp.path(), &["lock", "acquire", "build", "--ttl=0"]);
    assert_eq!(refused.status.code(), Some(6), "bad input, not contention");
    assert!(json(temp.path(), &["lock", "check", "build"]).is_null());
}

const BROKEN_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

fn with_a_broken_file(contents: &str) -> tempfile::TempDir {
    let temp = vault();
    ok(temp.path(), &["announce", "--roles", "dev"]);
    ok(
        temp.path(),
        &["new", "note", "healthy", "--body", "still findable"],
    );
    std::fs::write(temp.path().join("docs/broken.md"), contents).unwrap();
    temp
}

fn every_listing_survives(temp: &tempfile::TempDir) {
    for args in [
        vec!["announce"],
        vec!["recall", "findable"],
        vec!["task", "list"],
        vec!["skill", "list"],
        vec!["msg", "inbox"],
        vec!["agents"],
    ] {
        ok(temp.path(), &args);
    }
}

#[test]
fn a_file_with_invalid_yaml_does_not_take_the_vault_down() {
    let temp = with_a_broken_file(&format!(
        "---\nid: {BROKEN_ID}\ntype: note\ntitle: [broken\n---\n"
    ));
    every_listing_survives(&temp);
    let hits = json(temp.path(), &["recall", "findable"]);
    assert_eq!(
        hits.as_array().unwrap().len(),
        1,
        "the healthy note is still found"
    );
}

#[test]
fn a_file_with_an_unknown_type_does_not_take_the_vault_down() {
    let temp = with_a_broken_file(&format!(
        "---\nid: {BROKEN_ID}\ntype: brainstorm\ntitle: x\n---\n"
    ));
    every_listing_survives(&temp);
}

#[test]
fn a_file_left_with_merge_conflict_markers_does_not_take_the_vault_down() {
    let temp = with_a_broken_file(&format!(
        "---\nid: {BROKEN_ID}\ntype: note\n<<<<<<< HEAD\ntitle: ours\n=======\ntitle: theirs\n>>>>>>> other\n---\n"
    ));
    every_listing_survives(&temp);
}

#[test]
fn reading_an_unreadable_entity_names_its_file() {
    let temp = with_a_broken_file(&format!(
        "---\nid: {BROKEN_ID}\ntype: note\nstatus: nonsense\ntitle: x\n---\n"
    ));
    let out = orchy(temp.path(), &["read", BROKEN_ID]);
    assert_eq!(out.status.code(), Some(6));
    let message = String::from_utf8_lossy(&out.stderr);
    assert!(message.contains("docs/broken.md"), "{message}");
}

#[test]
fn the_briefing_counts_the_files_it_had_to_skip() {
    let temp = with_a_broken_file(&format!(
        "---\nid: {BROKEN_ID}\ntype: note\ntitle: [broken\n---\n"
    ));
    let briefing = json(temp.path(), &["announce"]);
    assert_eq!(briefing["unreadable"], 1);
    assert!(ok(temp.path(), &["announce"]).contains("orchy doctor"));
}

#[test]
fn skill_is_refused_as_a_document_type_with_a_pointer_to_orchy_skill() {
    let temp = vault();
    let out = orchy(temp.path(), &["new", "skill", "x", "--body", "y"]);
    assert_eq!(out.status.code(), Some(6));
    assert!(String::from_utf8_lossy(&out.stderr).contains("orchy skill write"));
}

#[test]
fn a_candidate_promoted_into_a_skill_becomes_a_real_skill_and_stays_as_the_record() {
    let temp = vault();
    let candidate = json(
        temp.path(),
        &[
            "new",
            "candidate",
            "Prefer jose",
            "--body",
            "jose supports RS256",
        ],
    );
    let candidate_id = candidate["id"].as_str().unwrap();

    let promoted = json(
        temp.path(),
        &["promote", candidate_id, "--as", "skill", "--name", "jose"],
    );
    assert_eq!(promoted["skill"]["name"], "jose");
    assert_eq!(promoted["skill"]["summary"], "Prefer jose");
    let skill_id = promoted["skill"]["id"].as_str().unwrap();

    let skill_file = std::fs::read_to_string(temp.path().join("skills/jose.md")).unwrap();
    assert!(skill_file.contains("jose supports RS256"), "{skill_file}");

    let record =
        std::fs::read_to_string(temp.path().join(format!("docs/{candidate_id}.md"))).unwrap();
    assert!(record.contains("status: promoted"), "{record}");

    let graph = ok(temp.path(), &["graph", &format!("skill:{skill_id}")]);
    assert!(graph.contains("derived_from"), "{graph}");

    ok(temp.path(), &["announce"]);
    assert_eq!(
        json(temp.path(), &["skill", "list"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn promoting_into_a_skill_name_already_in_use_is_refused() {
    let temp = vault();
    ok(
        temp.path(),
        &["skill", "write", "jose", "--summary", "already here"],
    );
    let candidate = json(
        temp.path(),
        &["new", "candidate", "Prefer jose", "--body", "x"],
    );
    let out = orchy(
        temp.path(),
        &[
            "promote",
            candidate["id"].as_str().unwrap(),
            "--as",
            "skill",
            "--name",
            "jose",
        ],
    );
    assert_eq!(out.status.code(), Some(5));
}

#[test]
fn a_hand_written_skill_document_without_a_name_does_not_stop_the_briefing() {
    let temp = vault();
    std::fs::write(
        temp.path().join("docs/old.md"),
        format!("---\nid: {BROKEN_ID}\ntype: skill\ntitle: old style\n---\n\nrule\n"),
    )
    .unwrap();
    ok(temp.path(), &["announce"]);
    ok(temp.path(), &["skill", "list", "--everywhere"]);
}

#[test]
fn document_commands_refuse_a_skill_and_leave_its_file_untouched() {
    let temp = vault();
    let skill = json(
        temp.path(),
        &["skill", "write", "commits", "--summary", "one line"],
    );
    let id = skill["id"].as_str().unwrap();
    let path = temp.path().join("skills/commits.md");
    let before = std::fs::read(&path).unwrap();

    for args in [
        vec!["set", id, "reviewer=alan"],
        vec!["archive", id],
        vec!["edit", id, "--content", "x"],
        vec!["read", id],
    ] {
        let out = orchy(temp.path(), &args);
        assert_eq!(out.status.code(), Some(4), "`orchy {}`", args.join(" "));
        assert!(String::from_utf8_lossy(&out.stderr).contains("orchy skill"));
    }
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "the skill file is byte-identical"
    );
    assert!(!temp.path().join(format!("docs/{id}.md")).exists());
}

#[test]
fn supersede_records_the_edge_on_the_replacement_pointing_at_what_it_replaced() {
    let temp = vault();
    let old = json(temp.path(), &["new", "decision", "Old way", "--body", "x"]);
    let new = json(temp.path(), &["new", "decision", "New way", "--body", "y"]);
    let (old_id, new_id) = (old["id"].as_str().unwrap(), new["id"].as_str().unwrap());

    ok(temp.path(), &["supersede", old_id, "--by", new_id]);

    let new_file = std::fs::read_to_string(temp.path().join(format!("docs/{new_id}.md"))).unwrap();
    assert!(
        new_file.contains(&format!("document:{old_id}")),
        "{new_file}"
    );
    let old_file = std::fs::read_to_string(temp.path().join(format!("docs/{old_id}.md"))).unwrap();
    assert!(!old_file.contains("\nsupersedes:"), "{old_file}");
    assert!(old_file.contains("status: superseded"), "{old_file}");

    let graph = json(temp.path(), &["graph", &format!("document:{new_id}")]);
    let edge = &graph.as_array().unwrap()[0]["edge"];
    assert_eq!(edge["relation"], "supersedes");
    assert!(edge["from"].as_str().unwrap().ends_with(new_id));
    assert!(edge["to"].as_str().unwrap().ends_with(old_id));
}

#[test]
fn recall_leaves_out_superseded_and_archived_knowledge_unless_asked() {
    let temp = vault();
    let old = json(
        temp.path(),
        &[
            "new",
            "decision",
            "Use HS256 tokens",
            "--body",
            "tokens signed with a shared key",
        ],
    );
    let new = json(
        temp.path(),
        &[
            "new",
            "decision",
            "Use RS256 tokens",
            "--body",
            "tokens signed with a key pair",
        ],
    );
    let archived = json(
        temp.path(),
        &[
            "new",
            "note",
            "Token notes",
            "--body",
            "tokens were discussed",
        ],
    );
    let plain = json(
        temp.path(),
        &[
            "new",
            "note",
            "Token glossary",
            "--body",
            "what tokens mean",
        ],
    );
    ok(
        temp.path(),
        &[
            "supersede",
            old["id"].as_str().unwrap(),
            "--by",
            new["id"].as_str().unwrap(),
        ],
    );
    ok(temp.path(), &["archive", archived["id"].as_str().unwrap()]);

    let ids = |hits: serde_json::Value| -> Vec<String> {
        hits.as_array()
            .unwrap()
            .iter()
            .map(|h| h["id"].as_str().unwrap().to_owned())
            .collect()
    };
    let found = ids(json(temp.path(), &["recall", "tokens"]));
    assert!(found.contains(&new["id"].as_str().unwrap().to_owned()));
    assert!(
        found.contains(&plain["id"].as_str().unwrap().to_owned()),
        "no status always passes"
    );
    assert!(
        !found.contains(&old["id"].as_str().unwrap().to_owned()),
        "superseded is hidden"
    );
    assert!(
        !found.contains(&archived["id"].as_str().unwrap().to_owned()),
        "archived is hidden"
    );

    let history = ids(json(
        temp.path(),
        &["recall", "tokens", "--status", "superseded"],
    ));
    assert_eq!(history, vec![old["id"].as_str().unwrap().to_owned()]);
}

#[test]
fn a_task_keeps_its_completion_note_and_failure_reason_on_disk() {
    let temp = vault();
    for (title, finish) in [
        ("finished", vec!["done", "--note", "shipped in abc123"]),
        ("failed", vec!["fail", "the lib lacks RS256"]),
        ("cancelled", vec!["cancel", "duplicate of another"]),
    ] {
        let task = json(temp.path(), &["task", "new", title]);
        let id = task["id"].as_str().unwrap().to_owned();
        ok(temp.path(), &["task", "claim", &id]);
        let mut args = vec!["task", finish[0], id.as_str()];
        args.extend_from_slice(&finish[1..]);
        ok(temp.path(), &args);

        let expected = finish.last().unwrap();
        let reread = json(temp.path(), &["task", "get", &id]);
        assert_eq!(reread["task"]["note"], *expected, "{title}: {reread}");
        let file =
            std::fs::read_to_string(temp.path().join(format!("tasks/done/{id}.md"))).unwrap();
        assert!(
            file.contains("## Outcome") && file.contains(expected),
            "{file}"
        );
    }
}

#[test]
fn an_edited_document_remembers_when_it_was_last_changed() {
    let temp = vault();
    let doc = json(
        temp.path(),
        &["new", "note", "Timestamps", "--body", "first"],
    );
    let id = doc["id"].as_str().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    ok(temp.path(), &["edit", id, "--content", "second"]);

    let read = json(temp.path(), &["read", id]);
    let created = read["document"]["created_at"].as_str().unwrap().to_owned();
    let updated = read["document"]["updated_at"].as_str().unwrap().to_owned();
    assert!(updated > created, "created {created}, updated {updated}");
    let file = std::fs::read_to_string(temp.path().join(format!("docs/{id}.md"))).unwrap();
    assert!(file.contains("\nupdated: "), "{file}");
}

#[test]
fn the_guide_works_where_there_is_no_vault_yet() {
    let temp = tempfile::tempdir().unwrap();
    let out = orchy(&temp.path().join("nowhere"), &["guide"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("orchy task next"));
}

fn task_id(temp: &tempfile::TempDir, args: &[&str]) -> String {
    let mut full = vec!["task", "new"];
    full.extend_from_slice(args);
    json(temp.path(), &full)["id"].as_str().unwrap().to_owned()
}

fn peek(temp: &tempfile::TempDir) -> Option<String> {
    let next = json(temp.path(), &["task", "next", "--peek"]);
    next["id"].as_str().map(str::to_owned)
}

#[test]
fn a_dependent_task_is_handed_out_once_its_dependency_completes_with_no_extra_step() {
    let temp = vault();
    let b = task_id(&temp, &["first", "--priority", "low"]);
    let a = task_id(&temp, &["second", "--priority", "high", "--depends-on", &b]);

    assert_eq!(peek(&temp), Some(b.clone()), "A waits while B is pending");
    ok(temp.path(), &["task", "claim", &b]);
    ok(temp.path(), &["task", "done", &b]);
    assert_eq!(
        peek(&temp),
        Some(a.clone()),
        "and is handed out once B completes"
    );
    assert_eq!(
        json(temp.path(), &["task", "get", &a])["readiness"],
        "satisfied"
    );
}

#[test]
fn a_task_whose_dependency_failed_is_never_handed_out_and_says_why() {
    let temp = vault();
    let b = task_id(&temp, &["first"]);
    let a = task_id(&temp, &["second", "--depends-on", &b]);
    ok(temp.path(), &["task", "claim", &b]);
    ok(temp.path(), &["task", "fail", &b, "no luck"]);

    assert_eq!(peek(&temp), None);
    let got = json(temp.path(), &["task", "get", &a]);
    assert_eq!(got["readiness"], "doomed");
    assert_eq!(got["dependencies"][0]["outcome"], "doomed");
    assert!(ok(temp.path(), &["task", "get", &a]).contains("failed or cancelled"));
}

#[test]
fn a_superseded_dependency_is_satisfied_once_its_replacements_complete() {
    let temp = vault();
    let b = task_id(&temp, &["original"]);
    let a = task_id(&temp, &["dependent", "--depends-on", &b]);
    let replaced = json(
        temp.path(),
        &["task", "replace", &b, "part one", "part two"],
    );
    let parts: Vec<String> = replaced["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_owned())
        .collect();

    ok(temp.path(), &["task", "claim", &parts[0]]);
    ok(temp.path(), &["task", "done", &parts[0]]);
    assert_eq!(
        json(temp.path(), &["task", "get", &a])["readiness"],
        "pending"
    );

    ok(temp.path(), &["task", "claim", &parts[1]]);
    ok(temp.path(), &["task", "done", &parts[1]]);
    assert_eq!(
        json(temp.path(), &["task", "get", &a])["readiness"],
        "satisfied"
    );
    assert_eq!(peek(&temp), Some(a));
}

#[test]
fn the_briefing_names_the_same_task_that_task_next_hands_out() {
    let temp = vault();
    ok(temp.path(), &["announce"]);
    for n in 0..20 {
        task_id(&temp, &[&format!("filler {n}")]);
    }
    let blocker = task_id(&temp, &["blocker", "--priority", "low"]);
    task_id(
        &temp,
        &["blocked", "--priority", "urgent", "--depends-on", &blocker],
    );
    let urgent = task_id(&temp, &["urgent one", "--priority", "high"]);

    let briefing = json(temp.path(), &["announce"]);
    assert_eq!(briefing["next"]["id"], urgent.as_str());
    assert_eq!(peek(&temp), Some(urgent));
}

#[test]
fn the_briefing_hands_over_the_latest_handoff() {
    let temp = vault();
    for n in 1..=21 {
        ok(
            temp.path(),
            &["new", "context", &format!("handoff {n}"), "--body", "state"],
        );
    }
    let briefing = json(temp.path(), &["announce"]);
    assert_eq!(briefing["handoff"]["title"], "handoff 21");
}

#[test]
fn a_parent_finished_by_rollup_gives_its_lease_back() {
    let temp = vault();
    let goal = task_id(&temp, &["goal"]);
    ok(temp.path(), &["task", "claim", &goal]);
    let split = json(temp.path(), &["task", "split", &goal, "part a", "part b"]);
    for child in split["created"].as_array().unwrap() {
        let id = child["id"].as_str().unwrap();
        ok(temp.path(), &["task", "claim", id]);
        ok(temp.path(), &["task", "done", id]);
    }
    assert_eq!(
        json(temp.path(), &["task", "get", &goal])["task"]["status"],
        "completed"
    );
    let held = ok(temp.path(), &["lock", "list"]);
    assert!(
        !held.contains(&goal),
        "the finished goal is still held:\n{held}"
    );
}

#[test]
fn detaching_the_last_open_child_lets_the_parent_roll_up() {
    let temp = vault();
    let goal = task_id(&temp, &["goal"]);
    let split = json(temp.path(), &["task", "split", &goal, "a", "b"]);
    let children: Vec<String> = split["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect();
    ok(temp.path(), &["task", "claim", &children[0]]);
    ok(temp.path(), &["task", "done", &children[0]]);
    assert_ne!(
        json(temp.path(), &["task", "get", &goal])["task"]["status"],
        "completed"
    );

    ok(temp.path(), &["task", "update", &children[1], "--detach"]);
    assert_eq!(
        json(temp.path(), &["task", "get", &goal])["task"]["status"],
        "completed"
    );
}

#[test]
fn announcing_again_keeps_the_namespace_an_agent_works_in() {
    let temp = vault();
    ok(temp.path(), &["announce", "--namespace", "/backend"]);
    let again = json(temp.path(), &["announce"]);
    assert_eq!(again["actor"]["namespace"], "/backend");
}

#[test]
fn writes_land_where_the_agent_works_unless_told_otherwise() {
    let temp = vault();
    ok(temp.path(), &["announce", "--namespace", "/backend"]);

    let note = json(temp.path(), &["new", "note", "here", "--body", "x"]);
    assert!(
        temp.path()
            .join(format!("docs/backend/{}.md", note["id"].as_str().unwrap()))
            .exists()
    );
    assert_eq!(
        json(temp.path(), &["task", "new", "t"])["namespace"],
        "/backend"
    );

    let rooted = json(
        temp.path(),
        &["new", "note", "root", "--namespace", "/", "--body", "x"],
    );
    assert_eq!(rooted["namespace"], "/", "an explicit namespace wins");

    let out = Command::new(env!("CARGO_BIN_EXE_orchy"))
        .args(["--json", "new", "note", "web", "--body", "x"])
        .env("ORCHY_VAULT", temp.path())
        .env("XDG_CONFIG_HOME", temp.path().join(".config"))
        .env("ORCHY_ACTOR", "claude")
        .env("ORCHY_NAMESPACE", "/web")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let web: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        web["namespace"], "/web",
        "ORCHY_NAMESPACE overrides the roster"
    );
}

fn piped(vault: &Path, args: &[&str], input: &str) -> Output {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_orchy"))
        .args(args)
        .env("ORCHY_VAULT", vault)
        .env("XDG_CONFIG_HOME", vault.join(".config"))
        .env("ORCHY_ACTOR", "claude")
        .env("NO_COLOR", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn a_new_document_takes_its_body_from_a_pipe() {
    let temp = vault();
    let out = piped(
        temp.path(),
        &["--json", "new", "note", "piped"],
        "## Context\n\nfrom a heredoc\n",
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        doc["body"].as_str().unwrap().contains("from a heredoc"),
        "{doc}"
    );

    let dash = piped(
        temp.path(),
        &["--json", "new", "note", "dash", "--body", "-"],
        "explicit",
    );
    let doc: serde_json::Value = serde_json::from_slice(&dash.stdout).unwrap();
    assert_eq!(doc["body"], "explicit");
}

#[test]
fn a_document_can_be_retitled_retyped_retagged_and_moved_and_each_is_recorded() {
    let temp = vault();
    let doc = json(
        temp.path(),
        &["new", "note", "Draft idea", "--tag", "old", "--body", "x"],
    );
    let id = doc["id"].as_str().unwrap();

    ok(temp.path(), &["retitle", id, "Settled idea"]);
    ok(temp.path(), &["retype", id, "decision"]);
    ok(temp.path(), &["tag", id, "+auth", "-old", "crypto"]);
    ok(temp.path(), &["ns", "move", id, "/web"]);

    let read = json(temp.path(), &["read", id])["document"].clone();
    assert_eq!(read["title"], "Settled idea");
    assert_eq!(read["kind"], "decision");
    assert_eq!(read["namespace"], "/web");
    let tags: Vec<&str> = read["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    assert_eq!(tags, vec!["auth", "crypto"]);

    let files: Vec<_> = walk(temp.path(), "docs");
    assert_eq!(
        files,
        vec![format!("docs/web/{id}.md")],
        "exactly one file, under docs/web/"
    );

    let topics: Vec<String> = json(temp.path(), &["events", "--key", id])
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["topic"].as_str().unwrap().to_owned())
        .collect();
    for topic in [
        "document.retitled",
        "document.retyped",
        "document.tagged",
        "document.moved",
    ] {
        assert!(
            topics.contains(&topic.to_owned()),
            "{topic} missing from {topics:?}"
        );
    }
}

#[test]
fn setting_a_managed_field_names_the_command_that_exists_for_it() {
    let temp = vault();
    let doc = json(temp.path(), &["new", "note", "x", "--body", "y"]);
    let out = orchy(
        temp.path(),
        &["set", doc["id"].as_str().unwrap(), "title=renamed"],
    );
    assert_eq!(out.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&out.stderr).contains("orchy retitle"));
}

fn walk(root: &Path, dir: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.join(dir)];
    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(&path).unwrap() {
            let entry = entry.unwrap().path();
            if entry.is_dir() {
                stack.push(entry);
            } else {
                found.push(
                    entry
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    found.sort();
    found
}

#[test]
fn an_edit_to_a_heading_used_twice_is_refused_until_one_is_picked() {
    let temp = vault();
    let doc = json(
        temp.path(),
        &[
            "new",
            "note",
            "Log",
            "--body",
            "## Notes\nfirst\n\n## Notes\nsecond\n\n## End\nend",
        ],
    );
    let id = doc["id"].as_str().unwrap();
    let before = json(temp.path(), &["read", id])["document"]["body"].clone();

    let out = orchy(
        temp.path(),
        &["edit", id, "--section", "Notes", "--content", "x"],
    );
    assert_eq!(out.status.code(), Some(7));
    assert_eq!(
        json(temp.path(), &["read", id])["document"]["body"],
        before,
        "nothing changed"
    );

    ok(
        temp.path(),
        &[
            "edit",
            id,
            "--section",
            "Notes",
            "--nth",
            "2",
            "--content",
            "changed",
        ],
    );
    let body = json(temp.path(), &["read", id])["document"]["body"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(body, "## Notes\nfirst\n\n## Notes\nchanged\n\n## End\nend");
    assert_eq!(
        ok(
            temp.path(),
            &["read", id, "--section", "Notes", "--nth", "1"]
        )
        .trim(),
        "first"
    );
}

#[test]
fn the_text_before_the_first_heading_and_the_headings_themselves_are_searchable() {
    let temp = vault();
    let doc = json(
        temp.path(),
        &[
            "new",
            "note",
            "Rules",
            "--body",
            "The zanzibar rule applies everywhere.\n\n## Details\nsee the handbook",
        ],
    );
    let id = doc["id"].as_str().unwrap();
    for query in ["zanzibar", "details"] {
        let hits = json(temp.path(), &["recall", query]);
        assert!(
            hits.as_array().unwrap().iter().any(|h| h["id"] == id),
            "`{query}` misses the document: {hits}"
        );
    }
}

#[test]
fn an_identifier_is_found_by_its_parts_and_by_its_whole_name() {
    let temp = vault();
    let doc = json(
        temp.path(),
        &[
            "new",
            "note",
            "Persistence",
            "--body",
            "The UserRepository caches reads.",
        ],
    );
    let id = doc["id"].as_str().unwrap();
    for query in ["repository", "UserRepository"] {
        let hits = json(temp.path(), &["recall", query]);
        assert!(
            hits.as_array().unwrap().iter().any(|h| h["id"] == id),
            "`{query}`: {hits}"
        );
    }
}

#[test]
fn a_message_is_addressed_by_the_short_id_the_inbox_prints() {
    let temp = vault();
    ok(temp.path(), &["announce"]);
    let sent = json(
        temp.path(),
        &[
            "--actor", "codex", "msg", "send", "@claude", "--body", "hello",
        ],
    );
    let id = sent["id"].as_str().unwrap();
    let short = &id[id.len() - 6..];
    assert!(ok(temp.path(), &["msg", "inbox"]).contains(short));

    let read = json(temp.path(), &["msg", "read", short]);
    assert_eq!(read["id"], id);
    ok(
        temp.path(),
        &[
            "msg",
            "send",
            "@claude",
            "--reply-to",
            short,
            "--body",
            "again",
        ],
    );
    ok(temp.path(), &["msg", "thread", short]);

    let out = orchy(temp.path(), &["msg", "read", &id[..2]]);
    assert_eq!(
        out.status.code(),
        Some(7),
        "a prefix every message shares is ambiguous"
    );
}

#[test]
fn events_with_a_limit_are_the_most_recent_ones() {
    let temp = vault();
    let ids: Vec<String> = (1..=5)
        .map(|n| task_id(&temp, &[&format!("t{n}")]))
        .collect();
    let latest = json(
        temp.path(),
        &["events", "--topic", "task.created", "--limit", "2"],
    );
    let keys: Vec<&str> = latest
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec![ids[3].as_str(), ids[4].as_str()]);
}

fn topics(temp: &tempfile::TempDir, args: &[&str]) -> Vec<String> {
    let mut full = vec!["events"];
    full.extend_from_slice(args);
    json(temp.path(), &full)
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["topic"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn links_roster_changes_locks_and_skill_edits_all_reach_the_event_log() {
    let temp = vault();
    ok(temp.path(), &["announce", "--roles", "dev"]);
    ok(temp.path(), &["announce", "--namespace", "/web"]);
    let a = json(temp.path(), &["new", "note", "a", "--body", "x"]);
    let b = json(temp.path(), &["new", "note", "b", "--body", "y"]);
    let (a, b) = (a["id"].as_str().unwrap(), b["id"].as_str().unwrap());
    ok(
        temp.path(),
        &[
            "link",
            &format!("document:{a}"),
            &format!("document:{b}"),
            "--rel",
            "related_to",
        ],
    );
    ok(
        temp.path(),
        &[
            "unlink",
            &format!("document:{a}"),
            &format!("document:{b}"),
            "--rel",
            "related_to",
        ],
    );
    ok(temp.path(), &["lock", "acquire", "deploy"]);
    ok(temp.path(), &["lock", "renew", "deploy"]);
    ok(temp.path(), &["lock", "release", "deploy"]);
    ok(
        temp.path(),
        &["skill", "write", "commits", "--summary", "one line"],
    );
    ok(
        temp.path(),
        &["skill", "set", "commits", "owner=alan", "--tag", "git"],
    );

    assert_eq!(
        topics(&temp, &["--key", a, "--topic", "edge."]),
        vec!["edge.added", "edge.removed"]
    );
    assert_eq!(
        topics(&temp, &["--topic", "actor."]),
        vec!["actor.announced", "actor.updated"]
    );
    assert_eq!(
        topics(&temp, &["--topic", "lock."]),
        vec!["lock.acquired", "lock.renewed", "lock.released"]
    );
    let skill = topics(&temp, &["--topic", "skill."]);
    assert!(
        skill.contains(&"skill.tagged".to_owned()) && skill.contains(&"skill.field_set".to_owned()),
        "{skill:?}"
    );
}

const ID_1: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA1";
const ID_2: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA2";
const ID_3: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA3";
const ID_4: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA4";
const ID_5: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA5";
const ID_6: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA6";
const ID_7: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA7";
const ID_8: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA8";

fn seed(temp: &tempfile::TempDir, key: &str, text: &str) {
    let path = temp.path().join(key);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn doctor_kinds(temp: &tempfile::TempDir, args: &[&str]) -> (Option<i32>, Vec<(String, String)>) {
    let mut full = vec!["--json", "doctor"];
    full.extend_from_slice(args);
    let out = orchy(temp.path(), &full);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let mut kinds: Vec<(String, String)> = report["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["kind"].as_str().unwrap().to_owned(),
                p["fixable"].to_string(),
            )
        })
        .collect();
    kinds.sort();
    (out.status.code(), kinds)
}

#[test]
fn doctor_finds_every_kind_of_problem_and_fixes_the_mechanical_ones() {
    let temp = vault();
    seed(
        &temp,
        "docs/broken.md",
        &format!("---\nid: {ID_1}\ntype: note\ntitle: [x\n---\n"),
    );
    seed(
        &temp,
        "docs/odd.md",
        &format!("---\nid: {ID_2}\ntype: brainstorm\ntitle: x\n---\n"),
    );
    seed(
        &temp,
        "tasks/done/misplaced.md",
        &format!("---\nid: {ID_3}\ntype: task\ntitle: t\nstatus: pending\n---\n"),
    );
    seed(
        &temp,
        "docs/hand/named.md",
        &format!("---\nid: {ID_4}\ntype: note\ntitle: n\nrelated_to:\n  - document:{ID_8}\n---\n"),
    );
    seed(
        &temp,
        &format!("tasks/open/{ID_5}.md"),
        &format!("---\nid: {ID_5}\ntype: task\ntitle: goal\nstatus: pending\n---\n"),
    );
    seed(
        &temp,
        &format!("tasks/done/{ID_6}.md"),
        &format!(
            "---\nid: {ID_6}\ntype: task\ntitle: child\nstatus: completed\nparent: task:{ID_5}\n---\n"
        ),
    );
    seed(
        &temp,
        &format!("docs/{ID_7}.md"),
        &format!(
            "---\nid: {ID_7}\ntype: decision\ntitle: old\nstatus: superseded\nsupersedes:\n  - document:{ID_4}\n---\n"
        ),
    );

    let (code, before) = doctor_kinds(&temp, &[]);
    assert_eq!(code, Some(6));
    let names: Vec<&str> = before.iter().map(|(k, _)| k.as_str()).collect();
    for kind in [
        "dangling_edge",
        "inverted_supersedes",
        "misnamed_file",
        "misplaced",
        "stale_rollup",
        "unknown_type",
        "unreadable",
    ] {
        assert!(names.contains(&kind), "{kind} missing from {before:?}");
    }

    let (code, after) = doctor_kinds(&temp, &["--fix"]);
    assert_eq!(code, Some(6), "manual problems remain");
    assert!(
        after.iter().all(|(_, fixable)| fixable == "false"),
        "{after:?}"
    );
    assert!(temp.path().join(format!("tasks/open/{ID_3}.md")).exists());
    assert!(temp.path().join(format!("docs/hand/{ID_4}.md")).exists());
    assert_eq!(
        json(temp.path(), &["task", "get", ID_5])["task"]["status"],
        "completed"
    );
    let new_home =
        std::fs::read_to_string(temp.path().join(format!("docs/hand/{ID_4}.md"))).unwrap();
    assert!(new_home.contains(&format!("document:{ID_7}")), "{new_home}");

    let (_, again) = doctor_kinds(&temp, &["--fix"]);
    assert_eq!(again, after, "a second --fix is a no-op");
}

#[test]
fn doctor_on_a_healthy_vault_exits_zero() {
    let temp = vault();
    ok(temp.path(), &["new", "note", "fine", "--body", "x"]);
    assert!(ok(temp.path(), &["doctor"]).contains("healthy"));
}

#[test]
fn doctor_reports_a_parent_cycle_once() {
    let temp = vault();
    seed(
        &temp,
        &format!("tasks/open/{ID_1}.md"),
        &format!(
            "---\nid: {ID_1}\ntype: task\ntitle: a\nstatus: pending\nparent: task:{ID_2}\n---\n"
        ),
    );
    seed(
        &temp,
        &format!("tasks/open/{ID_2}.md"),
        &format!(
            "---\nid: {ID_2}\ntype: task\ntitle: b\nstatus: pending\nparent: task:{ID_1}\n---\n"
        ),
    );
    let (code, kinds) = doctor_kinds(&temp, &[]);
    assert_eq!(code, Some(6));
    assert_eq!(kinds, vec![("parent_cycle".to_owned(), "false".to_owned())]);
}

#[test]
fn any_command_keeps_an_announced_agent_live_without_touching_the_roster_file() {
    let temp = vault();
    ok(temp.path(), &["announce"]);
    let roster: Vec<_> = std::fs::read_dir(temp.path().join("agents"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let before = std::fs::read(&roster[0]).unwrap();
    let presence: Vec<_> = std::fs::read_dir(temp.path().join(".orchy/presence"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let stamp = std::fs::read(&presence[0]).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10));
    ok(temp.path(), &["task", "list"]);

    assert_eq!(
        std::fs::read(&roster[0]).unwrap(),
        before,
        "the committed roster file is untouched"
    );
    assert_ne!(
        std::fs::read(&presence[0]).unwrap(),
        stamp,
        "presence was refreshed"
    );
    assert_eq!(
        json(temp.path(), &["agents", "--live"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn ready_and_blocked_together_are_exactly_the_open_board() {
    let temp = vault();
    let first = task_id(&temp, &["first"]);
    let waits = task_id(&temp, &["waits", "--depends-on", &first]);
    let parked = task_id(&temp, &["parked"]);
    ok(
        temp.path(),
        &["task", "block", &parked, "--reason", "vendor"],
    );
    let free = task_id(&temp, &["free", "--priority", "high"]);

    let ids = |value: serde_json::Value, path: &str| -> Vec<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.pointer(path).unwrap().as_str().unwrap().to_owned())
            .collect()
    };
    let ready = ids(json(temp.path(), &["task", "ready"]), "/id");
    assert_eq!(
        ready,
        vec![free.clone(), first.clone()],
        "in the order task next draws"
    );

    let blocked = json(temp.path(), &["task", "list", "--blocked"]);
    assert_eq!(ids(blocked.clone(), "/task/id").len(), 2);
    let waiting_on = blocked
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["task"]["id"] == waits.as_str())
        .unwrap()["waiting_on"][0]["id"]
        .clone();
    assert_eq!(waiting_on, first.as_str());

    let mut union: Vec<String> = ready.into_iter().chain(ids(blocked, "/task/id")).collect();
    union.sort();
    let board = json(
        temp.path(),
        &["task", "list", "--status", "pending", "--status", "blocked"],
    );
    let mut open: Vec<String> = board["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_owned())
        .collect();
    open.sort();
    assert_eq!(union, open);
}

#[test]
fn a_task_carries_its_definition_of_done_and_the_roles_that_may_take_it() {
    let temp = vault();
    let id = task_id(&temp, &["ship", "--acceptance", "tests pass"]);
    let got = json(temp.path(), &["task", "get", &id]);
    assert_eq!(got["task"]["acceptance_criteria"], "tests pass");

    ok(
        temp.path(),
        &[
            "task",
            "update",
            &id,
            "--acceptance",
            "tests and docs pass",
            "--role",
            "reviewer",
        ],
    );
    let got = json(temp.path(), &["task", "get", &id]);
    assert_eq!(got["task"]["acceptance_criteria"], "tests and docs pass");
    assert_eq!(
        got["task"]["assigned_roles"],
        serde_json::json!(["reviewer"])
    );
    assert!(ok(temp.path(), &["task", "get", &id]).contains("tests and docs pass"));
}

#[test]
fn a_rejected_candidate_stays_but_leaves_recall() {
    let temp = vault();
    let proposal = json(
        temp.path(),
        &["new", "candidate", "Use tabs", "--body", "tabs everywhere"],
    );
    let id = proposal["id"].as_str().unwrap();
    assert_eq!(proposal["status"], "proposed");
    assert_eq!(
        json(temp.path(), &["new", "note", "n", "--body", "x"])["status"],
        "active"
    );

    ok(temp.path(), &["reject", id, "--reason", "we use spaces"]);
    let read = json(temp.path(), &["read", id])["document"].clone();
    assert_eq!(read["status"], "rejected");
    assert_eq!(read["frontmatter"]["rejected_because"], "we use spaces");
    assert!(
        json(temp.path(), &["recall", "tabs"])
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_document_written_for_a_task_is_linked_to_it() {
    let temp = vault();
    let task = task_id(&temp, &["pick an algorithm"]);
    let doc = json(
        temp.path(),
        &[
            "new", "decision", "RS256", "--task", "pick an", "--body", "x",
        ],
    );
    let doc = doc["id"].as_str().unwrap();
    let graph = json(temp.path(), &["graph", &format!("task:{task}")]);
    let edge = &graph.as_array().unwrap()[0]["edge"];
    assert_eq!(edge["relation"], "produces");
    assert!(edge["to"].as_str().unwrap().ends_with(doc));
}

#[test]
fn a_budget_returns_whole_sections_up_to_roughly_that_many_tokens() {
    let temp = vault();
    for n in 0..6 {
        let body = format!("## Deploy {n}\n{}", "deploy steps in detail. ".repeat(40));
        ok(
            temp.path(),
            &["new", "note", &format!("runbook {n}"), "--body", &body],
        );
    }
    let hits = json(temp.path(), &["recall", "deploy", "--budget", "500"]);
    let texts: Vec<usize> = hits
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["text"].as_str().unwrap().chars().count())
        .collect();
    assert!(!texts.is_empty());
    let longest = *texts.iter().max().unwrap();
    assert!(texts.iter().sum::<usize>() <= 2000 + longest, "{texts:?}");
    assert!(texts.len() < 6, "the budget cut the list: {texts:?}");
}

#[test]
fn the_man_pages_cover_every_command_without_a_vault() {
    let temp = tempfile::tempdir().unwrap();
    let pages = temp.path().join("man");
    let out = orchy(
        &temp.path().join("nowhere"),
        &["man", "--out", pages.to_str().unwrap()],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for page in [
        "orchy.1",
        "orchy-task.1",
        "orchy-task-next.1",
        "orchy-skill-write.1",
        "orchy-doctor.1",
    ] {
        assert!(pages.join(page).exists(), "{page} missing");
    }
}

#[test]
fn events_since_a_moment_leave_out_what_came_before() {
    let temp = vault();
    task_id(&temp, &["old"]);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let cut = chrono::Utc::now().to_rfc3339();
    let recent = task_id(&temp, &["recent"]);
    let keys: Vec<String> = json(
        temp.path(),
        &["events", "--topic", "task.created", "--since", &cut],
    )
    .as_array()
    .unwrap()
    .iter()
    .map(|e| e["key"].as_str().unwrap().to_owned())
    .collect();
    assert_eq!(keys, vec![recent]);
    assert!(
        json(temp.path(), &["events", "--since", "1h"])
            .as_array()
            .unwrap()
            .len()
            > 1
    );
}

#[test]
fn integrating_claude_code_adds_one_session_hook_and_keeps_the_rest_in_order() {
    let temp = tempfile::tempdir().unwrap();
    let settings = temp.path().join(".claude/settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(
        &settings,
        "{\n  \"permissions\": {},\n  \"model\": \"opus\"\n}\n",
    )
    .unwrap();

    for _ in 0..2 {
        ok(
            temp.path(),
            &[
                "integrate",
                "claude-code",
                "--dir",
                temp.path().to_str().unwrap(),
                "--namespace",
                "/web",
            ],
        );
    }
    let text = std::fs::read_to_string(&settings).unwrap();
    assert!(
        text.find("permissions").unwrap() < text.find("model").unwrap(),
        "{text}"
    );
    assert_eq!(
        text.matches("orchy announce --namespace /web").count(),
        1,
        "{text}"
    );
}

#[test]
fn an_abandoned_claim_can_be_taken_back_once_its_lease_expires() {
    let temp = vault();
    let id = task_id(&temp, &["abandoned"]);
    ok(
        temp.path(),
        &["--actor", "crashed", "task", "claim", &id, "--ttl", "1"],
    );

    let early = orchy(
        temp.path(),
        &["task", "release", &id, "--force", "--reason", "gone"],
    );
    assert_eq!(early.status.code(), Some(5), "the lease is still live");

    std::thread::sleep(std::time::Duration::from_millis(1200));
    ok(
        temp.path(),
        &[
            "task",
            "release",
            &id,
            "--force",
            "--reason",
            "crashed agent",
        ],
    );
    assert_eq!(
        json(temp.path(), &["task", "get", &id])["task"]["status"],
        "pending"
    );

    let released = json(
        temp.path(),
        &["events", "--key", &id, "--topic", "task.released"],
    );
    assert_eq!(released[0]["payload"]["forced"], true);
    assert_eq!(released[0]["payload"]["reason"], "crashed agent");
}

#[test]
fn a_returning_agent_is_told_what_others_did_while_it_was_away() {
    let temp = vault();
    ok(temp.path(), &["announce"]);

    ok(
        temp.path(),
        &["--actor", "codex", "new", "decision", "d", "--body", "x"],
    );
    let t = task_id(&temp, &["done by codex"]);
    ok(temp.path(), &["--actor", "codex", "task", "claim", &t]);
    ok(temp.path(), &["--actor", "codex", "task", "done", &t]);
    ok(
        temp.path(),
        &["new", "note", "mine", "--body", "own work is not news"],
    );

    let text = ok(temp.path(), &["announce"]);
    assert!(text.contains("SINCE YOU WERE LAST HERE"), "{text}");
    assert!(
        text.contains("1 document written") && text.contains("1 task completed"),
        "{text}"
    );

    ok(
        temp.path(),
        &["--actor", "codex", "new", "note", "later", "--body", "y"],
    );
    let briefing = json(temp.path(), &["announce"]);
    assert_eq!(
        briefing["since_last"]["documents_created"], 1,
        "counted from the previous announce"
    );
    assert_eq!(briefing["since_last"]["tasks_completed"], 0);
}

fn file_of(temp: &tempfile::TempDir, id: &str) -> String {
    let found = walk(temp.path(), "docs")
        .into_iter()
        .chain(walk(temp.path(), "tasks"))
        .find(|key| key.ends_with(&format!("{id}.md")))
        .unwrap();
    std::fs::read_to_string(temp.path().join(found)).unwrap()
}

#[test]
fn a_target_file_shows_who_replaced_produced_or_subdivided_it() {
    let temp = vault();
    let old = json(temp.path(), &["new", "decision", "old way", "--body", "x"])["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let new = json(temp.path(), &["new", "decision", "new way", "--body", "y"])["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(temp.path(), &["supersede", &old, "--by", &new]);
    assert!(
        file_of(&temp, &old).contains(&format!("superseded_by:\n  - document:{new}")),
        "{}",
        file_of(&temp, &old)
    );

    let task = task_id(&temp, &["research"]);
    let note = json(
        temp.path(),
        &["new", "note", "findings", "--task", &task, "--body", "z"],
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        file_of(&temp, &note).contains(&format!("produced_by:\n  - task:{task}")),
        "{}",
        file_of(&temp, &note)
    );

    let goal = task_id(&temp, &["goal"]);
    let split = json(temp.path(), &["task", "split", &goal, "a", "b"]);
    let children: Vec<String> = split["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect();
    let goal_file = file_of(&temp, &goal);
    assert!(
        children
            .iter()
            .all(|c| goal_file.contains(&format!("task:{c}"))),
        "{goal_file}"
    );

    ok(temp.path(), &["task", "update", &children[1], "--detach"]);
    let goal_file = file_of(&temp, &goal);
    assert!(
        !goal_file.contains(&children[1]),
        "a detached child leaves the list: {goal_file}"
    );

    ok(
        temp.path(),
        &[
            "unlink",
            &format!("task:{task}"),
            &format!("document:{note}"),
            "--rel",
            "produces",
        ],
    );
    assert!(
        !file_of(&temp, &note).contains("produced_by"),
        "{}",
        file_of(&temp, &note)
    );
}

#[test]
fn the_inbox_can_keep_to_one_conversation() {
    let temp = vault();
    ok(temp.path(), &["announce"]);
    let send = |actor: &str, args: &[&str]| -> String {
        let mut full = vec!["--actor", actor, "msg", "send"];
        full.extend_from_slice(args);
        json(temp.path(), &full)["id"].as_str().unwrap().to_owned()
    };
    let first = send("codex", &["@claude", "--body", "about keys"]);
    send("codex", &["@claude", "--body", "about lunch"]);
    let answer = send(
        "claude",
        &["@codex", "--reply-to", &first, "--body", "which keys?"],
    );
    send(
        "codex",
        &["@claude", "--reply-to", &answer, "--body", "more on keys"],
    );

    let thread = json(temp.path(), &["msg", "inbox", "--thread", &first]);
    let bodies: Vec<&str> = thread
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["body"].as_str().unwrap())
        .collect();
    assert_eq!(bodies, vec!["about keys", "more on keys"]);
}

#[test]
fn a_listing_cut_short_says_how_much_is_left() {
    let temp = vault();
    for n in 0..3 {
        task_id(&temp, &[&format!("t{n}")]);
    }
    let text = ok(temp.path(), &["task", "list", "--limit", "2"]);
    assert!(text.contains("showing 2 of 3"), "{text}");
    assert!(!ok(temp.path(), &["task", "list"]).contains("showing"));
}

#[test]
fn a_graph_can_follow_one_relation_and_render_as_a_diagram() {
    let temp = vault();
    let doc = |title: &str| {
        json(temp.path(), &["new", "note", title, "--body", "x"])["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let (a, b, c) = (doc("a"), doc("b"), doc("c"));
    ok(
        temp.path(),
        &[
            "link",
            &format!("document:{a}"),
            &format!("document:{b}"),
            "--rel",
            "derived_from",
        ],
    );
    ok(
        temp.path(),
        &[
            "link",
            &format!("document:{a}"),
            &format!("document:{c}"),
            "--rel",
            "related_to",
        ],
    );

    let only = json(
        temp.path(),
        &["graph", &format!("document:{a}"), "--rel", "derived_from"],
    );
    assert_eq!(only.as_array().unwrap().len(), 1);
    assert_eq!(only[0]["edge"]["relation"], "derived_from");

    let mermaid = ok(
        temp.path(),
        &["graph", &format!("document:{a}"), "--format", "mermaid"],
    );
    assert!(mermaid.starts_with("graph LR"), "{mermaid}");
    assert!(mermaid.contains("-->|related_to|"), "{mermaid}");
    let dot = ok(
        temp.path(),
        &["graph", &format!("document:{a}"), "--format", "dot"],
    );
    assert!(
        dot.starts_with("digraph orchy {") && dot.contains("[label=\"derived_from\"]"),
        "{dot}"
    );
}

#[test]
fn why_tells_what_happened_to_an_entity_and_what_it_is_linked_to() {
    let temp = vault();
    let old = json(temp.path(), &["new", "decision", "old", "--body", "x"])["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let new = json(
        temp.path(),
        &["--actor", "codex", "new", "decision", "new", "--body", "y"],
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        temp.path(),
        &["--actor", "codex", "supersede", &old, "--by", &new],
    );

    let story = json(temp.path(), &["why", &format!("document:{old}")]);
    let topics: Vec<&str> = story["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["topic"].as_str().unwrap())
        .collect();
    assert!(
        topics.contains(&"document.created") && topics.contains(&"document.superseded"),
        "{topics:?}"
    );
    assert_eq!(story["links_in"][0]["relation"], "supersedes");
    let text = ok(temp.path(), &["why", &format!("document:{old}")]);
    assert!(
        text.contains("codex@") && text.contains("links to it"),
        "{text}"
    );
}

#[test]
fn recall_since_leaves_out_what_did_not_change_in_the_window() {
    let temp = vault();
    ok(
        temp.path(),
        &["new", "decision", "rotate keys", "--body", "rotate weekly"],
    );

    let recent = json(temp.path(), &["recall", "rotate", "--since", "1h"]);
    assert_eq!(recent.as_array().unwrap().len(), 1);
    let future = json(
        temp.path(),
        &["recall", "rotate", "--since", "2999-01-01T00:00:00Z"],
    );
    assert!(future.as_array().unwrap().is_empty(), "{future}");
}

#[test]
fn recall_graph_adds_what_the_hits_link_to_at_lower_relevance() {
    let temp = vault();
    let hit = json(
        temp.path(),
        &["new", "decision", "rotate keys", "--body", "rotate weekly"],
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let linked = json(
        temp.path(),
        &["new", "note", "incident", "--body", "the outage"],
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        temp.path(),
        &[
            "link",
            &format!("document:{linked}"),
            &format!("document:{hit}"),
            "--rel",
            "related_to",
        ],
    );

    let plain = json(temp.path(), &["recall", "rotate"]);
    assert_eq!(plain.as_array().unwrap().len(), 1);
    let expanded = json(temp.path(), &["recall", "rotate", "--graph", "1"]);
    let hits = expanded.as_array().unwrap();
    assert_eq!(hits.len(), 2, "{expanded}");
    assert_eq!(hits[1]["id"], linked.as_str());
    assert!(hits[1]["relevance"].as_f64() < hits[0]["relevance"].as_f64());
}

#[test]
fn task_next_hands_out_first_the_task_others_depend_on() {
    let temp = vault();
    ok(temp.path(), &["task", "new", "older"]);
    let blocker = json(temp.path(), &["task", "new", "blocker"])["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        temp.path(),
        &["task", "new", "waits", "--depends-on", &blocker],
    );

    let next = json(temp.path(), &["task", "next", "--peek"]);
    assert_eq!(next["title"], "blocker", "{next}");
}

#[test]
fn task_merge_folds_duplicates_into_the_kept_task() {
    let temp = vault();
    let keep = json(temp.path(), &["task", "new", "ship login", "--tag", "auth"])["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let other = json(temp.path(), &["task", "new", "login flow", "--tag", "web"])["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let child = json(temp.path(), &["task", "new", "form", "--parent", &other])["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let dependent = json(
        temp.path(),
        &["task", "new", "announce", "--depends-on", &other],
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let merged = json(temp.path(), &["task", "merge", &keep, &other]);
    assert_eq!(merged["merged"][0]["status"], "superseded");
    assert_eq!(merged["moved"][0]["id"], child.as_str());

    let kept = json(temp.path(), &["task", "get", &keep]);
    let tags: Vec<&str> = kept["task"]["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    assert_eq!(tags, vec!["auth", "web"]);
    assert_eq!(
        json(temp.path(), &["task", "get", &child])["task"]["parent"],
        keep.as_str()
    );

    ok(temp.path(), &["task", "claim", &child]);
    ok(temp.path(), &["task", "done", &child]);
    assert_eq!(
        json(temp.path(), &["task", "get", &keep])["task"]["status"],
        "completed",
        "rollup holds"
    );
    let links = ok(
        temp.path(),
        &["graph", &format!("task:{keep}"), "--rel", "merged_from"],
    );
    assert!(links.contains("merged_from"), "{links}");
    let waiting = json(temp.path(), &["task", "get", &dependent]);
    assert_eq!(waiting["readiness"], "satisfied", "{waiting}");
}

#[test]
fn consolidate_supersedes_the_sources_and_hides_them_from_recall() {
    let temp = vault();
    let first = json(
        temp.path(),
        &[
            "new",
            "note",
            "deploy steps",
            "--body",
            "rollout order",
            "--tag",
            "ops",
        ],
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let second = json(
        temp.path(),
        &["new", "note", "deploy notes", "--body", "rollout checks"],
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let into = json(
        temp.path(),
        &[
            "new",
            "note",
            "deploy guide",
            "--body",
            "rollout order and checks",
        ],
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let done = json(
        temp.path(),
        &["consolidate", &first, &second, "--into", &into],
    );
    assert_eq!(done["superseded"].as_array().unwrap().len(), 2);
    assert_eq!(done["into"]["tags"][0], "ops");

    let hits = json(temp.path(), &["recall", "rollout"]);
    let ids: Vec<&str> = hits
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![into.as_str()]);
    let graph = ok(
        temp.path(),
        &["graph", &format!("document:{into}"), "--rel", "merged_from"],
    );
    assert_eq!(graph.matches("merged_from").count(), 2, "{graph}");
}

#[test]
fn a_stale_hash_refuses_every_document_and_skill_change() {
    let temp = vault();
    let created = json(
        temp.path(),
        &["new", "decision", "keys", "--body", "rotate"],
    );
    let id = created["id"].as_str().unwrap().to_owned();
    let hash = created["content_hash"].as_str().unwrap().to_owned();
    assert!(
        ok(temp.path(), &["read", &id]).contains(&hash),
        "the hash is in the text header"
    );
    ok(temp.path(), &["edit", &id, "--content", "later"]);

    for args in [
        vec!["retitle", &id, "other"],
        vec!["retype", &id, "note"],
        vec!["tag", "--if-match", &hash, &id, "+x"],
        vec!["set", &id, "reviewer=alan"],
        vec!["archive", &id],
        vec!["ns", "move", &id, "/web"],
    ] {
        let mut args = args.clone();
        if !args.contains(&"--if-match") {
            args.extend(["--if-match", &hash]);
        }
        let refused = orchy(temp.path(), &args);
        assert_eq!(refused.status.code(), Some(5), "{args:?}");
    }
    let misplaced = orchy(temp.path(), &["tag", &id, "+x", "--if-match", &hash]);
    assert_eq!(
        misplaced.status.code(),
        Some(6),
        "a trailing option is refused, not taken as a tag"
    );
    let unchanged = json(temp.path(), &["read", &id])["document"].clone();
    assert!(unchanged["tags"].as_array().is_none_or(|t| t.is_empty()));
    assert_eq!(unchanged["title"], "keys");
    assert_eq!(unchanged["status"], "active");

    let fresh = unchanged["content_hash"].as_str().unwrap().to_owned();
    ok(temp.path(), &["archive", &id, "--if-match", &fresh]);

    let skill = json(
        temp.path(),
        &[
            "skill",
            "write",
            "commits",
            "--summary",
            "one line",
            "--body",
            "x",
        ],
    );
    let skill_hash = skill["content_hash"].as_str().unwrap().to_owned();
    ok(temp.path(), &["skill", "write", "commits", "--body", "y"]);
    let refused = orchy(
        temp.path(),
        &[
            "skill",
            "write",
            "commits",
            "--body",
            "z",
            "--if-match",
            &skill_hash,
        ],
    );
    assert_eq!(refused.status.code(), Some(5));
    let refused = orchy(
        temp.path(),
        &[
            "skill",
            "set",
            "commits",
            "--tag",
            "git",
            "--if-match",
            &skill_hash,
        ],
    );
    assert_eq!(refused.status.code(), Some(5));
}
