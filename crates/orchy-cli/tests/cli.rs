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
