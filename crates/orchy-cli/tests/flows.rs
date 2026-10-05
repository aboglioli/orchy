//! Whole stories told through the binary: several agents, several commands, one vault. Each
//! test follows a piece of work from start to finish and checks what a later agent, or a
//! human with an editor, would find.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::Duration;

use serde_json::Value;

struct Vault {
    root: tempfile::TempDir,
    machine_a: tempfile::TempDir,
    machine_b: tempfile::TempDir,
}

impl Vault {
    fn new() -> Self {
        let vault = Self {
            root: tempfile::tempdir().unwrap(),
            machine_a: tempfile::tempdir().unwrap(),
            machine_b: tempfile::tempdir().unwrap(),
        };
        vault.ok("human", &["init"]);
        vault
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn run_on(&self, machine: &Path, actor: &str, args: &[&str], stdin: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_orchy"))
            .args(args)
            .env("ORCHY_VAULT", self.path())
            .env("XDG_CONFIG_HOME", machine)
            .env("ORCHY_ACTOR", actor)
            .env("NO_COLOR", "1")
            .env_remove("ORCHY_NAMESPACE")
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("orchy binary runs");
        if let Some(input) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
        }
        child.wait_with_output().unwrap()
    }

    fn run(&self, actor: &str, args: &[&str]) -> Output {
        self.run_on(self.machine_a.path(), actor, args, None)
    }

    fn ok(&self, actor: &str, args: &[&str]) -> String {
        let out = self.run(actor, args);
        assert!(
            out.status.success(),
            "`orchy {}` as {actor} failed ({:?}): {}",
            args.join(" "),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn json(&self, actor: &str, args: &[&str]) -> Value {
        let mut full = vec!["--json"];
        full.extend_from_slice(args);
        serde_json::from_str(&self.ok(actor, &full)).expect("valid json")
    }

    fn json_on_b(&self, actor: &str, args: &[&str]) -> Value {
        let mut full = vec!["--json"];
        full.extend_from_slice(args);
        let out = self.run_on(self.machine_b.path(), actor, &full, None);
        assert!(
            out.status.success(),
            "machine b: `orchy {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("valid json")
    }

    fn piped(&self, actor: &str, args: &[&str], input: &str) -> Value {
        let mut full = vec!["--json"];
        full.extend_from_slice(args);
        let out = self.run_on(self.machine_a.path(), actor, &full, Some(input));
        assert!(
            out.status.success(),
            "`orchy {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("valid json")
    }

    fn refused(&self, actor: &str, args: &[&str]) -> (i32, String) {
        let out = self.run(actor, args);
        assert!(
            !out.status.success(),
            "`orchy {}` should have been refused",
            args.join(" ")
        );
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    fn id(&self, actor: &str, args: &[&str]) -> String {
        self.json(actor, args)["id"].as_str().unwrap().to_owned()
    }

    fn file(&self, id: &str) -> PathBuf {
        walk(self.path())
            .into_iter()
            .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(&format!("{id}.md")))
            .unwrap_or_else(|| panic!("no file for {id}"))
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found
}

fn items(value: &Value) -> &Vec<Value> {
    value
        .get("items")
        .unwrap_or(value)
        .as_array()
        .expect("a list")
}

fn ids(value: &Value) -> Vec<String> {
    items(value)
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_team_ships_a_feature_from_plan_to_handoff() {
    let vault = Vault::new();
    vault.ok("architect", &["announce", "--roles", "architect"]);
    vault.ok(
        "coder-1",
        &["announce", "--roles", "developer", "--namespace", "/auth"],
    );
    vault.ok(
        "coder-2",
        &["announce", "--roles", "developer", "--namespace", "/auth"],
    );
    vault.ok("reviewer", &["announce", "--roles", "reviewer"]);
    vault.ok(
        "architect",
        &[
            "skill",
            "write",
            "tokens",
            "--summary",
            "Tokens are RS256",
            "--namespace",
            "/auth",
            "--body",
            "- rotate weekly",
        ],
    );

    let decision = vault.id(
        "architect",
        &[
            "new",
            "decision",
            "Single sign-on through OIDC",
            "--namespace",
            "/auth",
            "--body",
            "## Context\nCustomers ask for SSO.\n\n## Decision\nOIDC first.",
        ],
    );
    let goal = vault.id(
        "architect",
        &[
            "task",
            "new",
            "Ship SSO",
            "--namespace",
            "/auth",
            "--priority",
            "high",
            "--description",
            "- OIDC login\n- account linking",
            "--acceptance",
            "- a customer logs in with Okta\n- docs updated",
        ],
    );
    let login = vault.id(
        "architect",
        &[
            "task",
            "new",
            "OIDC login",
            "--parent",
            &goal,
            "--role",
            "developer",
        ],
    );
    let linking = vault.id(
        "architect",
        &[
            "task",
            "new",
            "Account linking",
            "--parent",
            &goal,
            "--role",
            "developer",
            "--depends-on",
            &login,
        ],
    );
    let (code, why) = vault.refused("coder-2", &["task", "claim", &linking]);
    assert_eq!(
        code, 5,
        "work that waits on a dependency cannot be claimed: {why}"
    );

    let first = vault.json("coder-1", &["task", "next", "--role", "developer"]);
    assert_eq!(
        first["id"],
        login.as_str(),
        "only the free task is handed out"
    );
    assert_eq!(
        vault.json("coder-2", &["task", "next", "--peek"])["id"],
        Value::Null,
        "nothing else is ready"
    );

    vault.ok(
        "coder-1",
        &["lock", "acquire", "file:src/oidc.rs", "--ttl", "120"],
    );
    let (code, _) = vault.refused("coder-2", &["lock", "acquire", "file:src/oidc.rs"]);
    assert_eq!(code, 5);
    vault.ok("coder-1", &["task", "start", &login]);
    let sent = vault.json(
        "coder-1",
        &[
            "msg",
            "send",
            "role:reviewer",
            "--subject",
            "review OIDC",
            "--body",
            "- PR #7 is up",
        ],
    );
    let inbox = vault.json("reviewer", &["msg", "inbox"]);
    assert!(ids(&inbox).contains(&sent["id"].as_str().unwrap().to_owned()));
    vault.ok("reviewer", &["msg", "read", sent["id"].as_str().unwrap()]);
    vault.ok(
        "reviewer",
        &[
            "msg",
            "send",
            "@coder-1",
            "--reply-to",
            sent["id"].as_str().unwrap(),
            "--body",
            "approved",
        ],
    );
    let thread = vault.json("coder-1", &["msg", "thread", sent["id"].as_str().unwrap()]);
    assert_eq!(items(&thread).len(), 2, "the reply joins the thread");
    let mine = vault.json("coder-1", &["msg", "sent"]);
    assert!(ids(&mine).contains(&sent["id"].as_str().unwrap().to_owned()));
    vault.ok("coder-1", &["msg", "resolve", sent["id"].as_str().unwrap()]);

    let notes = vault.id(
        "coder-1",
        &[
            "new",
            "report",
            "OIDC login notes",
            "--task",
            &login,
            "--body",
            "Discovery endpoint cached for 1h.",
        ],
    );
    vault.ok(
        "coder-1",
        &["task", "done", &login, "--note", "- merged #7\n- see notes"],
    );
    vault.ok("coder-1", &["lock", "release", "file:src/oidc.rs"]);
    assert_eq!(
        vault.json("coder-2", &["task", "get", &linking])["readiness"],
        "satisfied"
    );
    vault.ok("coder-2", &["task", "claim", &linking, "--start"]);
    assert_eq!(
        vault.json("coder-2", &["task", "list", "--mine"])["items"][0]["id"],
        linking.as_str()
    );
    vault.ok("coder-2", &["task", "done", &linking]);

    let finished = vault.json("architect", &["task", "get", &goal]);
    assert_eq!(finished["task"]["status"], "completed", "the goal rolls up");
    let goal_file = fs::read_to_string(vault.file(&goal)).unwrap();
    assert!(
        vault
            .file(&goal)
            .starts_with(vault.path().join("tasks/done"))
    );
    assert!(goal_file.contains("a customer logs in with Okta"));
    assert!(
        fs::read_to_string(vault.file(&login))
            .unwrap()
            .contains("- merged #7")
    );
    assert!(
        fs::read_to_string(vault.file(&notes))
            .unwrap()
            .contains(&format!("task:{login}")),
        "the report says which task produced it"
    );
    assert!(
        vault
            .json("architect", &["lock", "list"])
            .as_array()
            .unwrap()
            .is_empty(),
        "no lease outlives the work"
    );

    vault.ok(
        "coder-2",
        &[
            "new",
            "context",
            "handoff",
            "--body",
            "- SSO shipped\n- next: SCIM",
        ],
    );
    let briefing = vault.json("coder-3", &["announce", "--namespace", "/auth"]);
    assert!(
        briefing["handoff"]["body"]
            .as_str()
            .unwrap()
            .contains("next: SCIM")
    );
    assert!(
        briefing["skills"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "tokens")
    );

    let by_coder = vault.json("human", &["events", "--by", "coder-1", "--limit", "100"]);
    assert!(
        items(&by_coder)
            .iter()
            .any(|e| e["topic"] == "task.completed" || e["topic"] == "task.finished"),
        "{by_coder}"
    );
    let story = vault.json("human", &["why", &format!("task:{goal}")]);
    assert!(!story["history"].as_array().unwrap().is_empty());
    assert!(
        vault
            .json("human", &["recall", "OIDC", "--anchor", "/auth"])
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["id"] == decision.as_str())
    );
}

#[test]
fn knowledge_evolves_without_losing_its_history() {
    let vault = Vault::new();
    let old = vault.id(
        "ops",
        &[
            "new",
            "decision",
            "Deploy on Fridays",
            "--namespace",
            "/infra/ci",
            "--body",
            "Intro.\n\n## Rule\nDeploy any day.\n\n## Why\nSpeed.",
        ],
    );
    vault.ok(
        "ops",
        &[
            "edit",
            &old,
            "--replace-in",
            "Intro.",
            "--content",
            "- context first",
        ],
    );
    vault.ok(
        "ops",
        &[
            "edit",
            &old,
            "--section",
            "Rule",
            "--content",
            "Deploy Mon-Thu.",
        ],
    );
    vault.ok("ops", &["edit", &old, "--content", "Appended."]);
    let body = vault.json("ops", &["read", &old])["document"]["body"].clone();
    assert!(body.as_str().unwrap().starts_with("- context first"));
    assert!(body.as_str().unwrap().contains("Deploy Mon-Thu."));
    vault.ok(
        "ops",
        &[
            "edit",
            &old,
            "--replace",
            "--content",
            "## Rule\nNo Friday deploys.",
        ],
    );
    assert_eq!(
        vault.json("ops", &["read", &old, "--section", "Rule"])["section"]
            .as_str()
            .unwrap()
            .trim(),
        "No Friday deploys."
    );

    vault.ok("ops", &["ns", "move", &old, "/infra"]);
    assert_eq!(
        vault.file(&old),
        vault.path().join(format!("docs/infra/{old}.md")),
        "moving to a parent namespace moves the file up"
    );
    let canonical = vault.path().join(format!("docs/infra/{old}.md"));
    let dragged = vault.path().join("docs/infra/runbooks");
    fs::create_dir_all(&dragged).unwrap();
    fs::rename(&canonical, dragged.join(format!("{old}.md"))).unwrap();
    let (code, _) = vault.refused("human", &["doctor"]);
    assert_eq!(
        code, 6,
        "a folder that disagrees with the namespace is a problem"
    );
    assert_eq!(
        vault.json("ops", &["read", &old])["document"]["namespace"],
        "/infra",
        "the folder never changes the namespace"
    );
    vault.ok("ops", &["set", &old, "owner=ops"]);
    assert_eq!(
        vault.file(&old),
        canonical,
        "the next save puts the file back"
    );

    let file = fs::read_to_string(&canonical).unwrap();
    fs::write(
        &canonical,
        file.replace("namespace: /infra", "namespace: /infra/ci"),
    )
    .unwrap();
    vault.ok("human", &["doctor", "--fix"]);
    assert_eq!(
        vault.file(&old),
        vault.path().join(format!("docs/infra/ci/{old}.md")),
        "editing the namespace by hand moves the file where it now belongs"
    );
    vault.ok("ops", &["ns", "move", &old, "/infra"]);

    let new = vault.id(
        "ops",
        &[
            "new",
            "decision",
            "Deploy freeze on Fridays",
            "--namespace",
            "/infra",
            "--body",
            "Freeze from Friday 12:00.",
        ],
    );
    vault.ok("ops", &["supersede", &old, "--by", &new]);
    for command in [["archive", old.as_str()], ["unarchive", old.as_str()]] {
        let (code, _) = vault.refused("ops", &command);
        assert_eq!(
            code, 5,
            "{command:?}: what replaced a document cannot be undone"
        );
    }
    let (code, _) = vault.refused("ops", &["supersede", &old, "--by", &new]);
    assert_eq!(code, 5, "a document is superseded once");

    let draft = vault.id("ops", &["new", "note", "Old pager rota", "--body", "x"]);
    vault.ok("ops", &["archive", &draft]);
    vault.ok("ops", &["archive", &draft]);
    assert!(
        vault
            .json("ops", &["recall", "pager rota"])
            .as_array()
            .unwrap()
            .is_empty()
    );
    vault.ok("ops", &["unarchive", &draft]);
    assert_eq!(
        vault.json("ops", &["recall", "pager rota"])[0]["id"],
        draft.as_str()
    );

    let proposal = vault.id(
        "coder",
        &[
            "new",
            "candidate",
            "Blue-green deploys",
            "--body",
            "Two stacks.",
        ],
    );
    vault.ok("ops", &["reject", &proposal, "--reason", "- too costly"]);
    let (code, _) = vault.refused("ops", &["promote", &proposal, "--as", "decision"]);
    assert_eq!(code, 5, "a rejected proposal stays rejected");
    let accepted = vault.id(
        "coder",
        &["new", "candidate", "Canary deploys", "--body", "5% first."],
    );
    vault.ok("ops", &["promote", &accepted, "--as", "decision"]);

    let dup = vault.id(
        "coder",
        &[
            "new",
            "note",
            "Freeze rules",
            "--body",
            "No Friday deploys.",
            "--tag",
            "ops",
        ],
    );
    let merged = vault.json("ops", &["consolidate", &dup, "--into", &new]);
    assert_eq!(merged["into"]["tags"][0], "ops");

    let hits: Vec<String> = vault
        .json("ops", &["recall", "Friday deploys"])
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["id"].as_str().unwrap().to_owned())
        .collect();
    assert!(hits.contains(&new) && !hits.contains(&old) && !hits.contains(&dup));
    let graph = vault.json(
        "ops",
        &["graph", &format!("document:{new}"), "--depth", "2"],
    );
    let relations: Vec<&str> = items(&graph)
        .iter()
        .map(|h| h["edge"]["relation"].as_str().unwrap())
        .collect();
    assert!(relations.contains(&"supersedes") && relations.contains(&"merged_from"));
    let history = vault.json("ops", &["why", &format!("document:{old}")]);
    let topics: Vec<&str> = history["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["topic"].as_str().unwrap())
        .collect();
    assert!(topics.contains(&"document.created") && topics.contains(&"document.superseded"));

    let missing = "01BX5ZZKBKACTAV9WEVGEMMVRZ";
    let (code, _) = vault.refused(
        "ops",
        &[
            "link",
            &format!("document:{new}"),
            &format!("document:{missing}"),
            "--rel",
            "related_to",
        ],
    );
    assert_eq!(code, 4, "a link must point at something that exists");
    let file = vault.file(&new);
    let text = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        text.replacen(
            "---\n",
            &format!("---\nrelated_to: [document:{missing}]\n"),
            1,
        ),
    )
    .unwrap();
    vault.ok(
        "ops",
        &[
            "unlink",
            &format!("document:{new}"),
            &format!("document:{missing}"),
            "--rel",
            "related_to",
        ],
    );
    assert!(
        !fs::read_to_string(&file).unwrap().contains(missing),
        "a dangling link can always be removed"
    );

    let imported = vault.piped(
        "ops",
        &[
            "import",
            "-",
            "--kind",
            "reference",
            "--title",
            "Pager duty",
        ],
        "---\ntitle: Ignored because --title wins\nteam: sre\n---\n\nCall the owner.\n",
    );
    assert_eq!(imported["title"], "Pager duty");
    assert_eq!(imported["frontmatter"]["team"], "sre");
    let exported = vault.ok("ops", &["export", "--namespace", "/infra"]);
    assert!(
        exported
            .lines()
            .all(|l| serde_json::from_str::<Value>(l).is_ok())
    );
    assert!(exported.contains(&new));
}

#[test]
fn a_plan_changes_while_the_work_is_under_way() {
    let vault = Vault::new();
    let types = vault.json("human", &["types"]);
    assert!(types["kinds"].as_array().unwrap().len() >= 14);
    assert!(types["relations"].as_array().unwrap().len() >= 16);

    let goal = vault.id("lead", &["task", "new", "Migrate billing"]);
    let split = vault.json(
        "lead",
        &["task", "split", &goal, "schema", "backfill", "cutover"],
    );
    let children: Vec<String> = split["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_owned())
        .collect();
    let (schema, backfill, cutover) = (&children[0], &children[1], &children[2]);

    vault.ok(
        "lead",
        &[
            "task",
            "block",
            cutover,
            "--on",
            backfill,
            "--reason",
            "- needs data",
        ],
    );
    let waiting = vault.json("lead", &["task", "list", "--blocked"]);
    assert!(
        items(&waiting)
            .iter()
            .any(|w| w["task"]["id"] == cutover.as_str())
    );
    vault.ok("lead", &["task", "unblock", cutover]);
    assert_eq!(
        vault.json("lead", &["task", "get", cutover])["readiness"],
        "pending",
        "unblocking keeps the dependency --on recorded"
    );

    vault.ok(
        "lead",
        &[
            "task",
            "update",
            schema,
            "--title",
            "Schema v2",
            "--description",
            "- add invoices table",
            "--priority",
            "urgent",
        ],
    );
    let updated = vault.json("lead", &["task", "get", schema])["task"].clone();
    assert_eq!(updated["title"], "Schema v2");
    assert_eq!(updated["description"], "- add invoices table");

    vault.ok("dev", &["task", "claim", schema]);
    vault.ok(
        "dev",
        &["task", "fail", schema, "- migration locks the table"],
    );
    assert_eq!(
        vault.json("lead", &["task", "get", backfill])["task"]["status"],
        "pending"
    );
    let replaced = vault.json(
        "lead",
        &[
            "task",
            "replace",
            backfill,
            "Backfill in batches",
            "--reason",
            "- avoid long locks",
        ],
    );
    let batches = replaced["created"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        vault.json("lead", &["task", "get", &batches])["task"]["parent"],
        goal.as_str(),
        "a replacement stays under the same goal"
    );
    assert_eq!(
        vault.json("lead", &["task", "get", cutover])["readiness"],
        "pending",
        "the cutover now waits on the replacement"
    );

    let extra = vault.id("dev", &["task", "new", "Backfill batches"]);
    let tune = vault.id(
        "dev",
        &["task", "new", "Tune batch size", "--parent", &extra],
    );
    let merged = vault.json("lead", &["task", "merge", &batches, &extra]);
    assert_eq!(merged["moved"].as_array().unwrap().len(), 1);

    let (code, why) = vault.refused("dev", &["task", "claim", &batches]);
    assert_eq!(code, 5, "the merged subtask now belongs to it: {why}");
    vault.ok("dev", &["task", "update", &batches, "--detach"]);
    vault.ok("dev", &["task", "update", &batches, "--parent", &goal]);
    let (code, why) = vault.refused(
        "lead",
        &["task", "cancel", &batches, "- not needed after all"],
    );
    assert_eq!(
        code, 5,
        "a goal with open work beneath it is not over: {why}"
    );
    vault.ok("lead", &["task", "cancel", &tune, "- not needed after all"]);
    assert_eq!(
        vault.json("lead", &["task", "get", &batches])["task"]["status"],
        "cancelled",
        "its last open subtask cancelled, the goal follows"
    );
    assert_eq!(
        vault.json("lead", &["task", "get", cutover])["readiness"],
        "doomed",
        "a cancelled dependency dooms what waits on it"
    );
    vault.ok("lead", &["task", "dep", cutover, "--remove", backfill]);
    vault.ok("dev", &["task", "claim", cutover]);
    vault.ok("dev", &["task", "done", cutover]);
    assert_eq!(
        vault.json("lead", &["task", "get", &goal])["task"]["status"],
        "failed",
        "a goal with a failed step fails"
    );
    let (code, _) = vault.refused("dev", &["task", "claim", &goal]);
    assert_eq!(code, 5);
}

#[test]
fn two_machines_share_one_vault_and_keep_their_own_logs() {
    let vault = Vault::new();
    vault.json("coder", &["announce", "--roles", "developer"]);
    vault.json_on_b("coder", &["announce", "--roles", "developer"]);
    let roster = vault.json("human", &["agents"]);
    assert_eq!(
        roster.as_array().unwrap().len(),
        2,
        "one alias, two machines"
    );

    let task = vault.id("coder", &["task", "new", "on machine a"]);
    let seen = vault.json_on_b("coder", &["task", "get", &task]);
    assert_eq!(seen["task"]["title"], "on machine a");
    vault.json_on_b("coder", &["task", "claim", &task]);
    let (code, _) = vault.refused("coder", &["task", "done", &task]);
    assert_eq!(
        code, 5,
        "the same alias on another machine is another actor"
    );

    let roots = fs::read_dir(vault.path().join("events")).unwrap().count();
    assert_eq!(roots, 2, "each machine appends to its own log");
    let by_alias = vault.json("human", &["events", "--by", "coder", "--limit", "100"]);
    let machines: Vec<&str> = items(&by_alias)
        .iter()
        .filter_map(|e| e["actor"].as_str())
        .collect();
    assert!(
        machines.iter().any(|a| a.starts_with("coder@"))
            && machines
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                >= 2,
        "a bare alias finds that alias on every machine: {machines:?}"
    );
}

#[test]
fn skills_resolve_where_the_agent_works() {
    let vault = Vault::new();
    vault.ok(
        "lead",
        &[
            "skill",
            "write",
            "commits",
            "--summary",
            "Conventional commits",
        ],
    );
    vault.ok(
        "lead",
        &[
            "skill",
            "write",
            "commits",
            "--summary",
            "Reference the ticket",
            "--namespace",
            "/web",
        ],
    );
    vault.ok("web-dev", &["announce", "--namespace", "/web/app"]);

    let shown = vault.json("web-dev", &["skill", "show", "commits"]);
    assert_eq!(
        shown["namespace"], "/web",
        "the nearest skill, as in the briefing"
    );
    assert_eq!(
        vault.json("stranger", &["skill", "show", "commits"])["namespace"],
        "/"
    );
    vault.ok("web-dev", &["skill", "set", "commits", "owner=web"]);
    let web = vault.json("lead", &["skill", "show", "commits", "--namespace", "/web"]);
    assert_eq!(web["frontmatter"]["owner"], "web");
    let root = vault.json("lead", &["skill", "show", "commits", "--namespace", "/"]);
    assert!(root["frontmatter"].get("owner").is_none());
    let listed = vault.json("web-dev", &["skill", "list"]);
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["namespace"], "/web");
}

#[test]
fn free_text_may_start_with_a_dash() {
    let vault = Vault::new();
    let task = vault.id(
        "dev",
        &[
            "task",
            "new",
            "t",
            "--description",
            "-x flag is broken",
            "--acceptance",
            "- one\n- two",
        ],
    );
    vault.ok("dev", &["task", "claim", &task]);
    vault.ok("dev", &["task", "fail", &task, "- it broke"]);
    let got = vault.json("dev", &["task", "get", &task])["task"].clone();
    assert_eq!(got["description"], "-x flag is broken");
    assert_eq!(got["acceptance_criteria"], "- one\n- two");
    assert_eq!(got["note"], "- it broke");
    let doc = vault.json("dev", &["new", "note", "n", "--body", "- a list"]);
    assert_eq!(doc["body"], "- a list");
    let sent = vault.json("dev", &["msg", "send", "broadcast", "--body", "- heads up"]);
    assert_eq!(sent["body"], "- heads up");
}

#[test]
fn a_reader_that_stops_early_is_not_an_error() {
    let vault = Vault::new();
    for n in 0..300 {
        vault.ok("dev", &["new", "note", &format!("note {n}"), "--body", "x"]);
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_orchy"))
        .args(["export"])
        .env("ORCHY_VAULT", vault.path())
        .env("XDG_CONFIG_HOME", vault.machine_a.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = [0u8; 16];
    child.stdout.take().unwrap().read_exact(&mut first).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{:?}", out.status);
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn every_integration_prints_the_same_announce_command() {
    let vault = Vault::new();
    let repo = tempfile::tempdir().unwrap();
    for agent in ["claude-code", "codex", "opencode", "gemini"] {
        let printed = vault.ok(
            "human",
            &[
                "integrate",
                agent,
                "--dir",
                repo.path().to_str().unwrap(),
                "--namespace",
                "/web",
                "--role",
                "developer",
                "--print",
            ],
        );
        assert!(
            printed.contains("orchy announce --namespace /web --roles developer"),
            "{agent}: {printed}"
        );
    }
    assert!(
        fs::read_dir(repo.path()).unwrap().next().is_none(),
        "--print writes nothing"
    );
}

#[test]
fn a_message_becomes_a_titled_task_and_the_thread_closes() {
    let vault = Vault::new();
    vault.ok("ops", &["announce", "--roles", "ops"]);
    let sent = vault.json(
        "dev",
        &[
            "msg",
            "send",
            "role:ops",
            "--body",
            "disk is at 95% on db-1",
        ],
    );
    let id = sent["id"].as_str().unwrap();
    let promoted = vault.json(
        "ops",
        &[
            "msg",
            "promote",
            id,
            "--title",
            "Grow db-1 disk",
            "--role",
            "ops",
        ],
    );
    assert_eq!(promoted["task"]["title"], "Grow db-1 disk");
    assert_eq!(promoted["message"]["status"], "resolved");
    let task = promoted["task"]["id"].as_str().unwrap();
    let links = vault.json("ops", &["graph", &format!("task:{task}")]);
    assert!(
        items(&links)
            .iter()
            .any(|h| h["edge"]["relation"] == "spawned_by")
    );
    let (code, _) = vault.refused("ops", &["msg", "resolve", id]);
    assert_eq!(code, 5, "a thread is resolved once");
}

#[test]
fn a_url_import_names_the_document_after_the_page_when_asked() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/runbook.md", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request).unwrap();
        let body = "restart the worker\n";
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let vault = Vault::new();
    let imported = vault.json(
        "ops",
        &[
            "import",
            &url,
            "--kind",
            "reference",
            "--title",
            "Worker runbook",
        ],
    );
    server.join().unwrap();
    assert_eq!(imported["title"], "Worker runbook");
}

#[test]
fn a_skill_written_without_a_namespace_lands_where_the_agent_works() {
    let vault = Vault::new();
    vault.ok("web-dev", &["announce", "--namespace", "/web"]);
    let written = vault.json(
        "web-dev",
        &[
            "skill",
            "write",
            "css",
            "--summary",
            "Use tokens",
            "--tag",
            "ui",
        ],
    );
    assert_eq!(written["namespace"], "/web");
    assert_eq!(written["tags"][0], "ui");
    assert!(vault.path().join("skills/web/css.md").exists());
    let root = vault.json(
        "lead",
        &[
            "skill",
            "write",
            "commits",
            "--summary",
            "Conventional commits",
        ],
    );
    assert_eq!(
        root["namespace"], "/",
        "an agent off the roster works at the root"
    );
    let guide = vault.ok("web-dev", &["guide"]);
    assert!(guide.contains("orchy announce"), "{guide}");
}

fn assert_healthy(vault: &Vault) {
    let report = vault.json("human", &["doctor"]);
    assert_eq!(
        report["problems"].as_array().map(Vec::len),
        Some(0),
        "every refusal must leave the vault healthy: {report}"
    );
}

#[test]
fn links_are_only_ever_made_to_things_that_exist_and_fit() {
    let vault = Vault::new();
    let doc = vault.id("dev", &["new", "note", "Notes", "--body", "x"]);
    let task = vault.id("dev", &["task", "new", "Work"]);
    let ghost = "01ZZZZZZZZZZZZZZZZZZZZZZZZ";

    for args in [
        vec!["task", "new", "w", "--parent", ghost],
        vec!["task", "new", "w", "--parent", &doc],
        vec!["task", "new", "w", "--depends-on", ghost],
        vec!["task", "dep", &task, "--add", ghost],
        vec!["task", "block", &task, "--on", &doc],
    ] {
        let (code, why) = vault.refused("dev", &args);
        assert_eq!(code, 4, "`{}`: {why}", args.join(" "));
    }

    let (code, why) = vault.refused("dev", &["set", &doc, &format!("supersedes=document:{doc}")]);
    assert_eq!(code, 5, "{why}");
    assert!(why.contains("orchy supersede"), "{why}");
    let (code, why) = vault.refused("dev", &["set", &doc, "subtasks=x"]);
    assert_eq!(code, 5, "{why}");
    vault.ok(
        "dev",
        &["skill", "write", "style", "--summary", "s", "--body", "b"],
    );
    let (code, _) = vault.refused("dev", &["skill", "set", "style", "derives=x"]);
    assert_eq!(code, 5);
    let (code, _) = vault.refused("dev", &["skill", "set", "style", "--remove", "related_to"]);
    assert_eq!(code, 5);

    let machine = vault.json("dev", &["status"])["machine"]
        .as_str()
        .unwrap()
        .to_owned();
    vault.json("dev", &["announce"]);
    vault.ok(
        "dev",
        &[
            "link",
            &format!("document:{doc}"),
            &format!("actor:dev@{machine}"),
            "--rel",
            "owned_by",
        ],
    );
    let (code, _) = vault.refused(
        "dev",
        &[
            "link",
            &format!("document:{doc}"),
            &format!("actor:ghost@{machine}"),
            "--rel",
            "owned_by",
        ],
    );
    assert_eq!(
        code, 4,
        "an actor that never announced is not there to own anything"
    );
    assert_healthy(&vault);
}

#[test]
fn no_change_may_make_work_wait_on_itself() {
    let vault = Vault::new();
    let a = vault.id("lead", &["task", "new", "A"]);
    let b = vault.id("lead", &["task", "new", "B", "--depends-on", &a]);
    let (code, why) = vault.refused("lead", &["task", "dep", &a, "--add", &b]);
    assert_eq!(code, 6, "{why}");
    assert!(why.contains("wait on itself"), "{why}");
    let (code, _) = vault.refused("lead", &["task", "block", &a, "--on", &b]);
    assert_eq!(code, 6);

    let goal = vault.id("lead", &["task", "new", "Goal"]);
    let step = vault.id("lead", &["task", "new", "Step", "--parent", &goal]);
    let (code, _) = vault.refused("lead", &["task", "dep", &step, "--add", &goal]);
    assert_eq!(code, 6, "a subtask waiting on its own goal can never start");
    let (code, _) = vault.refused("lead", &["task", "update", &goal, "--parent", &step]);
    assert_eq!(code, 6);

    let below = vault.id("lead", &["task", "new", "Below", "--parent", &step]);
    let (code, _) = vault.refused("lead", &["task", "merge", &below, &goal]);
    assert_eq!(code, 6, "merging a goal into its own grandchild would loop");
    assert_healthy(&vault);
}

#[test]
fn finished_work_never_has_open_work_beneath_it() {
    let vault = Vault::new();
    let goal = vault.id("lead", &["task", "new", "Goal"]);
    let step = vault.id("lead", &["task", "new", "Step", "--parent", &goal]);
    let (code, _) = vault.refused("lead", &["task", "cancel", &goal, "dropped"]);
    assert_eq!(code, 5);
    let (code, _) = vault.refused("lead", &["task", "replace", &goal, "Other"]);
    assert_eq!(code, 5);

    let done = vault.id("lead", &["task", "new", "Done"]);
    vault.ok("lead", &["task", "claim", &done, "--start"]);
    vault.ok("lead", &["task", "done", &done]);
    let (code, _) = vault.refused("lead", &["task", "split", &done, "More"]);
    assert_eq!(code, 5);
    let (code, _) = vault.refused("lead", &["task", "new", "Late", "--parent", &done]);
    assert_eq!(code, 5);
    let (code, _) = vault.refused("lead", &["task", "update", &step, "--parent", &done]);
    assert_eq!(code, 5);
    let (code, _) = vault.refused("lead", &["task", "merge", &done, &step]);
    assert_eq!(code, 5, "nothing is merged into finished work");
    assert_healthy(&vault);
}

#[test]
fn replaced_knowledge_always_names_something_current() {
    let vault = Vault::new();
    let old = vault.id("ops", &["new", "note", "Old", "--body", "x"]);
    let new = vault.id("ops", &["new", "note", "New", "--body", "y"]);
    let other = vault.id("ops", &["new", "note", "Other", "--body", "z"]);
    vault.ok("ops", &["supersede", &old, "--by", &new]);

    let (code, why) = vault.refused("ops", &["supersede", &other, "--by", &old]);
    assert_eq!(code, 5, "{why}");
    let (code, _) = vault.refused("ops", &["consolidate", &other, "--into", &old]);
    assert_eq!(code, 5);
    let (code, _) = vault.refused("ops", &["retype", &old, "candidate"]);
    assert_eq!(code, 5, "crossing into proposals would wipe `superseded`");
    let proposal = vault.id("ops", &["new", "candidate", "Maybe", "--body", "m"]);
    let (code, _) = vault.refused("ops", &["supersede", &other, "--by", &proposal]);
    assert_eq!(code, 5);
    assert_eq!(
        vault.json("ops", &["read", &old])["document"]["status"],
        "superseded",
        "nothing brought it back"
    );
    assert_healthy(&vault);
}

#[test]
fn nothing_orchy_writes_is_unreadable_afterwards() {
    let vault = Vault::new();
    let doc = vault.id("dev", &["new", "note", "Notes", "--body", "x"]);
    for field in ["=v", " =v", "#c=v", "? q=1", "a:b=1"] {
        let (code, why) = vault.refused("dev", &["set", &doc, field]);
        assert_eq!(code, 6, "`{field}`: {why}");
    }
    let (code, _) = vault.refused(
        "dev",
        &[
            "new",
            "note",
            "Merge",
            "--body",
            "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x",
        ],
    );
    assert_eq!(code, 6);
    let (code, _) = vault.refused(
        "dev",
        &["task", "new", "T", "--description", "a\n## Outcome\nb"],
    );
    assert_eq!(code, 6);
    vault.ok(
        "dev",
        &[
            "new",
            "note",
            "About conflicts",
            "--body",
            "```\n<<<<<<< HEAD\n=======\n>>>>>>> x\n```",
        ],
    );
    assert_healthy(&vault);
}

#[test]
fn a_thread_keeps_its_links_when_it_is_resolved() {
    let vault = Vault::new();
    let doc = vault.id("dev", &["new", "note", "Spec", "--body", "x"]);
    let message = vault.id(
        "dev",
        &["msg", "send", "broadcast", "--body", "see the spec"],
    );
    vault.ok(
        "dev",
        &[
            "link",
            &format!("message:{message}"),
            &format!("document:{doc}"),
            "--rel",
            "related_to",
        ],
    );
    vault.ok(
        "dev",
        &[
            "link",
            &format!("document:{doc}"),
            &format!("message:{message}"),
            "--rel",
            "derived_from",
        ],
    );
    vault.ok("dev", &["msg", "resolve", &message]);
    let file = fs::read_to_string(vault.file(&message)).unwrap();
    assert!(file.contains("related_to"), "{file}");
    assert!(file.contains("derives"), "{file}");
    assert_healthy(&vault);
}

#[test]
fn merging_respects_holders_and_never_strands_the_kept_task() {
    let vault = Vault::new();
    let held = vault.id("a2", &["task", "new", "Held"]);
    vault.ok("a2", &["task", "claim", &held]);
    let keep = vault.id("a1", &["task", "new", "Keep"]);
    let (code, _) = vault.refused("a1", &["task", "merge", &keep, &held]);
    assert_eq!(
        code, 5,
        "another agent's claimed work is not retired under it"
    );
    let (code, _) = vault.refused("a1", &["task", "replace", &held, "Instead"]);
    assert_eq!(code, 5);

    let dup = vault.id("a1", &["task", "new", "Dup"]);
    let waits = vault.id("a1", &["task", "new", "Waits", "--depends-on", &dup]);
    vault.ok("a1", &["task", "merge", &waits, &dup]);
    assert_eq!(
        vault.json("a1", &["task", "get", &waits])["readiness"],
        "satisfied",
        "waiting on a duplicate merged into you is waiting on nothing"
    );
    vault.ok("a1", &["task", "claim", &waits]);
    assert_healthy(&vault);
}

#[test]
fn errors_are_json_when_json_was_asked_for() {
    let vault = Vault::new();
    let out = vault.run(
        "dev",
        &["task", "get", "01ZZZZZZZZZZZZZZZZZZZZZZZZ", "--json"],
    );
    assert_eq!(out.status.code(), Some(4));
    let error: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(error["error"]["kind"], "not_found");
    assert_eq!(error["error"]["exit"], 4);
}
