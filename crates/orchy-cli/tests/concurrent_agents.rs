//! Several agents driving the real binary at once.
//!
//! Sequential tests miss the failures that matter for a tool whose premise is a team: a queue
//! that hands two agents the same task, a lock held for a whole process, a write lost because
//! two of them landed together. Everything here spawns real processes in parallel.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;

struct Vault {
    dir: tempfile::TempDir,
}

impl Vault {
    fn new() -> Self {
        let vault = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        vault.run("alan", &["init"]).expect("init");
        vault
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn command(&self, actor: &str, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_orchy"));
        command
            .args(args)
            .env("ORCHY_VAULT", self.path())
            .env("XDG_CONFIG_HOME", self.path().join(".config"))
            .env("ORCHY_ACTOR", actor)
            .env("NO_COLOR", "1");
        command
    }

    fn raw(&self, actor: &str, args: &[&str]) -> Output {
        self.command(actor, args).output().expect("orchy runs")
    }

    fn run(&self, actor: &str, args: &[&str]) -> Result<String, String> {
        let out = self.raw(actor, args);
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }

    fn ok(&self, actor: &str, args: &[&str]) -> String {
        self.run(actor, args)
            .unwrap_or_else(|e| panic!("`orchy {}` as {actor}: {e}", args.join(" ")))
    }

    fn json(&self, actor: &str, args: &[&str]) -> serde_json::Value {
        let mut full = vec!["--json"];
        full.extend_from_slice(args);
        serde_json::from_str(&self.ok(actor, &full)).expect("valid json")
    }

    fn new_task(&self, title: &str) -> String {
        self.json("alan", &["task", "new", title])["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn status_of(&self, id: &str) -> String {
        self.json("alan", &["task", "get", id])["task"]["status"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn task_ids(&self) -> Vec<String> {
        self.json("alan", &["task", "list", "--limit", "200"])["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_owned())
            .collect()
    }

    fn event_topics(&self) -> Vec<String> {
        self.json("alan", &["events"])
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["topic"].as_str().unwrap().to_owned())
            .collect()
    }
}

/// Run one closure per agent, all at once, and collect what each got back.
fn in_parallel<T, F>(agents: &[&str], body: F) -> Vec<T>
where
    T: Send,
    F: Fn(&str) -> T + Sync,
{
    thread::scope(|scope| {
        let handles: Vec<_> = agents
            .iter()
            .map(|agent| scope.spawn(|| body(agent)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

const TEAM: [&str; 5] = ["alan", "claude", "codex", "gemini", "pi"];

#[test]
fn a_whole_team_announces_at_once_and_every_seat_appears() {
    let vault = Vault::new();

    let results = in_parallel(&TEAM, |agent| {
        vault.run(agent, &["announce", "--roles", "developer"])
    });
    assert!(
        results.iter().all(|r| r.is_ok()),
        "announcing is not a contended operation: {results:?}"
    );

    let roster = vault.json("alan", &["agents"]);
    let aliases: BTreeSet<String> = roster
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["alias"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(aliases.len(), TEAM.len(), "every seat is on the roster");
}

#[test]
fn concurrent_next_hands_every_agent_a_different_task() {
    let vault = Vault::new();
    for n in 0..4 {
        vault.new_task(&format!("task {n}"));
    }

    let handed = in_parallel(&["claude", "codex", "gemini", "pi"], |agent| {
        vault.run(agent, &["--json", "task", "next"])
    });

    let mut ids = Vec::new();
    for result in &handed {
        let out = result
            .as_ref()
            .unwrap_or_else(|e| panic!("losing a race must not be an error: {e}"));
        let task: serde_json::Value = serde_json::from_str(out).unwrap();
        ids.push(task["id"].as_str().unwrap().to_owned());
    }

    let distinct: BTreeSet<&String> = ids.iter().collect();
    assert_eq!(
        distinct.len(),
        4,
        "four agents, four tasks, no two the same: {ids:?}"
    );
}

#[test]
fn next_runs_out_quietly_when_there_is_less_work_than_agents() {
    let vault = Vault::new();
    vault.new_task("the only one");

    let handed = in_parallel(&["claude", "codex", "gemini"], |agent| {
        vault.run(agent, &["--json", "task", "next"])
    });

    let claimed = handed
        .iter()
        .filter(|r| r.as_ref().is_ok_and(|out| out.trim() != "null"))
        .count();
    assert_eq!(claimed, 1, "exactly one agent can have the only task");
    assert!(
        handed.iter().all(|r| r.is_ok()),
        "an empty queue is nothing to do, not an error"
    );
}

#[test]
fn exactly_one_agent_wins_a_race_for_a_named_task() {
    let vault = Vault::new();
    let id = vault.new_task("contended");

    let results = in_parallel(&["claude", "codex", "gemini", "pi"], |agent| {
        vault.run(agent, &["task", "claim", &id])
    });

    let winners = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(winners, 1, "a lease admits one holder: {results:?}");
    assert_eq!(vault.status_of(&id), "claimed");
}

#[test]
fn another_agent_cannot_finish_work_it_does_not_hold() {
    let vault = Vault::new();
    let id = vault.new_task("mine");
    vault.ok("claude", &["task", "claim", &id]);

    let refused = vault.raw("codex", &["task", "done", &id]);
    assert_eq!(
        refused.status.code(),
        Some(5),
        "the domain refuses, and says so with an exit code an agent can branch on"
    );
    assert_eq!(vault.status_of(&id), "claimed");
}

#[test]
fn parallel_writes_to_different_entities_all_survive() {
    let vault = Vault::new();

    let results = in_parallel(&TEAM, |agent| {
        vault.run("alan", &["task", "new", &format!("from {agent}")])
    });
    assert!(results.iter().all(|r| r.is_ok()), "{results:?}");

    assert_eq!(
        vault.task_ids().len(),
        TEAM.len(),
        "a write lost to lock contention would show up as a missing task"
    );
}

#[test]
fn parallel_document_writes_all_land_and_are_searchable() {
    let vault = Vault::new();

    in_parallel(&TEAM, |agent| {
        vault
            .ok(
                agent,
                &[
                    "new",
                    "discovery",
                    &format!("finding by {agent}"),
                    "--body",
                    "a shared keyword: telemetry",
                ],
            )
            .len()
    });

    let hits = vault.json("alan", &["recall", "telemetry"]);
    assert_eq!(
        hits.as_array().unwrap().len(),
        TEAM.len(),
        "every parallel document is present and indexed"
    );
}

#[test]
fn every_concurrent_write_reaches_the_event_log() {
    let vault = Vault::new();

    in_parallel(&TEAM, |agent| {
        vault.ok("alan", &["task", "new", &format!("logged by {agent}")])
    });

    let created = vault
        .event_topics()
        .iter()
        .filter(|t| *t == "task.created")
        .count();
    assert_eq!(
        created,
        TEAM.len(),
        "the log is the audit trail; a dropped append is a silent hole in it"
    );
}

#[test]
fn subtasks_finished_in_parallel_still_roll_the_goal_up_once() {
    let vault = Vault::new();
    let goal = vault.new_task("the goal");
    let split = vault.json(
        "alan",
        &["task", "split", &goal, "first", "second", "third", "fourth"],
    );
    let children: Vec<String> = split["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect();

    let agents = ["claude", "codex", "gemini", "pi"];
    let vault = &vault;
    let finished = thread::scope(|scope| {
        agents
            .iter()
            .zip(&children)
            .map(|(agent, child)| {
                scope.spawn(move || {
                    vault.run(agent, &["task", "claim", child])?;
                    vault.run(agent, &["task", "done", child])
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(finished.iter().all(|r| r.is_ok()), "{finished:?}");

    assert_eq!(
        vault.status_of(&goal),
        "completed",
        "the goal is derived from its children however they finish"
    );

    let rolled = vault
        .event_topics()
        .iter()
        .filter(|t| *t == "task.rolled_up")
        .count();
    assert_eq!(
        rolled, 1,
        "the goal moves once, no matter how many children reported at the same moment"
    );
}

#[test]
fn a_broadcast_written_once_reaches_every_reader_in_parallel() {
    let vault = Vault::new();
    let readers = ["claude", "codex", "gemini", "pi"];
    for agent in readers {
        vault.ok(agent, &["announce"]);
    }
    vault.ok(
        "alan",
        &[
            "msg",
            "send",
            "broadcast",
            "--subject",
            "heads up",
            "--body",
            "the build is red",
        ],
    );

    let inboxes = in_parallel(&readers, |agent| {
        vault
            .json(agent, &["msg", "inbox"])
            .as_array()
            .unwrap()
            .len()
    });
    assert!(
        inboxes.iter().all(|n| *n == 1),
        "one file, four readers, no coordination needed: {inboxes:?}"
    );

    let files = std::fs::read_dir(vault.path().join("messages"))
        .unwrap()
        .flatten()
        .count();
    assert_eq!(
        files, 1,
        "a broadcast is one message, not one per recipient"
    );
}

#[test]
fn read_watermarks_are_per_agent_and_do_not_interfere() {
    let vault = Vault::new();
    let readers = ["claude", "codex", "gemini"];
    for agent in readers {
        vault.ok(agent, &["announce"]);
    }
    let message = vault.json("alan", &["msg", "send", "broadcast", "--body", "ping"]);
    let id = message["id"].as_str().unwrap();

    vault.ok("claude", &["msg", "read", id]);

    let unread = in_parallel(&readers, |agent| {
        (
            agent.to_owned(),
            vault
                .json(agent, &["msg", "inbox"])
                .as_array()
                .unwrap()
                .len(),
        )
    });
    for (agent, count) in unread {
        let expected = if agent == "claude" { 0 } else { 1 };
        assert_eq!(
            count, expected,
            "{agent} sees its own read state and no one else's"
        );
    }
}

#[test]
fn a_lease_admits_one_holder_and_frees_the_resource_after_release() {
    let vault = Vault::new();

    let first = in_parallel(&TEAM, |agent| {
        vault.run(agent, &["lock", "acquire", "deploy/prod"])
    });
    let holders = first.iter().filter(|r| r.is_ok()).count();
    assert_eq!(holders, 1, "an advisory lock admits one: {first:?}");

    let holder = TEAM[first.iter().position(|r| r.is_ok()).unwrap()];
    vault.ok(holder, &["lock", "release", "deploy/prod"]);

    assert!(
        vault
            .run("gemini", &["lock", "acquire", "deploy/prod"])
            .is_ok(),
        "releasing hands the resource on"
    );
}

#[test]
fn the_vault_is_still_readable_by_everyone_while_writes_are_happening() {
    let vault = Vault::new();
    for n in 0..3 {
        vault.new_task(&format!("existing {n}"));
    }

    let writers = ["claude", "codex"];
    let readers = ["gemini", "pi", "alan"];

    let (written, read) = thread::scope(|scope| {
        let w = scope.spawn(|| {
            in_parallel(&writers, |agent| {
                vault.run("alan", &["task", "new", &format!("added by {agent}")])
            })
        });
        let r = scope.spawn(|| {
            in_parallel(&readers, |agent| {
                vault.run(agent, &["--json", "task", "list"])
            })
        });
        (w.join().unwrap(), r.join().unwrap())
    });

    assert!(written.iter().all(|r| r.is_ok()), "{written:?}");
    assert!(
        read.iter().all(|r| r.is_ok()),
        "a read must never be blocked by someone else's write: {read:?}"
    );
    assert_eq!(vault.task_ids().len(), 5);
}

#[test]
fn a_vault_survives_a_burst_from_one_agent_repeated_by_all_of_them() {
    let vault = Vault::new();

    let results = in_parallel(&TEAM, |agent| {
        (0..4)
            .map(|n| vault.run("alan", &["task", "new", &format!("{agent} {n}")]))
            .collect::<Vec<_>>()
    });

    let failures: Vec<_> = results
        .iter()
        .flatten()
        .filter_map(|r| r.as_ref().err())
        .collect();
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(vault.task_ids().len(), TEAM.len() * 4);
}

#[test]
fn the_event_log_keeps_every_append_from_a_burst() {
    let vault = Vault::new();

    in_parallel(&TEAM, |agent| {
        for n in 0..3 {
            vault.ok("alan", &["task", "new", &format!("{agent} {n}")]);
        }
    });

    let created = vault
        .event_topics()
        .iter()
        .filter(|t| *t == "task.created")
        .count();
    assert_eq!(created, TEAM.len() * 3);
}

#[test]
fn partitions_are_created_as_configured_and_spread_the_load() {
    let vault = Vault::new();
    in_parallel(&TEAM, |agent| {
        for n in 0..3 {
            vault.ok("alan", &["task", "new", &format!("{agent} {n}")]);
        }
    });

    let machine_root: PathBuf = std::fs::read_dir(vault.path().join("events"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .expect("a log root for this machine");

    let partitions = std::fs::read_dir(&machine_root)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .count();
    assert_eq!(partitions, 10, "the configured default");

    let used = std::fs::read_dir(&machine_root)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            std::fs::read_dir(e.path())
                .map(|d| {
                    d.flatten().any(|f| {
                        f.path().extension().is_some_and(|x| x == "log")
                            && f.metadata().map(|m| m.len() > 0).unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        })
        .count();
    assert!(
        used > 1,
        "routing on the event key should spread fifteen aggregates over more than one partition"
    );
}
