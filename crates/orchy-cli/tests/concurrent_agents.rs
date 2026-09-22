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

    fn new_document(&self, title: &str) -> String {
        self.json("alan", &["new", "note", title, "--body", "start"])["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn document(&self, id: &str) -> serde_json::Value {
        self.json("alan", &["read", id])["document"].clone()
    }

    fn body_of(&self, id: &str) -> String {
        self.document(id)["body"].as_str().unwrap().to_owned()
    }

    fn edges_from(&self, entity: &str) -> Vec<String> {
        self.json("alan", &["graph", entity])
            .as_array()
            .unwrap()
            .iter()
            .map(|hop| hop["edge"]["to"].as_str().unwrap().to_owned())
            .collect()
    }

    /// Every file the vault owns, so a test can assert on what is actually on disk rather
    /// than on what the CLI is willing to tell it.
    fn files(&self) -> Vec<PathBuf> {
        fn walk(dir: &Path, into: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, into);
                } else {
                    into.push(path);
                }
            }
        }
        let mut found = Vec::new();
        walk(self.path(), &mut found);
        found
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

/// The invariant behind most of what follows: a command that reports success left its mark,
/// and a command that could not leave its mark said so. Anything in between is a lost update.
fn successes(results: &[Result<String, String>]) -> usize {
    results.iter().filter(|r| r.is_ok()).count()
}

#[test]
fn concurrent_appends_to_one_document_are_never_silently_dropped() {
    let vault = Vault::new();
    let id = vault.new_document("contended");

    let writers = ["ann", "bob", "cal", "dee", "eve", "fay", "gil", "hal"];
    let results = in_parallel(&writers, |agent| {
        vault.run(agent, &["edit", &id, "--content", &format!("line-{agent}")])
    });

    let landed = vault.body_of(&id).matches("line-").count();
    assert_eq!(
        successes(&results),
        landed,
        "every reported success must be on disk: {results:?}"
    );
    assert!(landed >= 1, "somebody has to win");
}

#[test]
fn an_edit_that_loses_a_race_is_refused_rather_than_dropped() {
    let vault = Vault::new();

    for round in 0..5 {
        let id = vault.new_document(&format!("round {round}"));
        let marker = format!("edit-{round}");

        let outcomes = in_parallel(&["claude", "codex"], |agent| {
            if agent == "claude" {
                ("archive", vault.raw(agent, &["archive", &id]))
            } else {
                (
                    "edit",
                    vault.raw(agent, &["edit", &id, "--content", &marker]),
                )
            }
        });

        let document = vault.document(&id);
        for (what, output) in &outcomes {
            let landed = match *what {
                "archive" => document["status"] == "archived",
                _ => document["body"].as_str().unwrap().contains(&marker),
            };
            if output.status.success() {
                assert!(
                    landed,
                    "`{what}` reported success in round {round} but its change is gone"
                );
            } else {
                assert_eq!(
                    output.status.code(),
                    Some(5),
                    "a lost race is a refusal the agent can branch on"
                );
                assert!(
                    !landed,
                    "`{what}` was refused in round {round} yet changed the document"
                );
            }
        }
    }
}

#[test]
fn a_digest_taken_before_someone_elses_edit_no_longer_matches() {
    let vault = Vault::new();
    let id = vault.new_document("guarded");
    let stale = vault.document(&id)["content_hash"]
        .as_str()
        .unwrap()
        .to_owned();

    vault.ok(
        "claude",
        &["edit", &id, "--content", "someone got here first"],
    );

    let refused = vault.raw(
        "codex",
        &["edit", &id, "--content", "mine", "--if-match", &stale],
    );
    assert_eq!(refused.status.code(), Some(5));
    assert!(
        !vault.body_of(&id).contains("mine"),
        "a refused edit writes nothing"
    );
}

#[test]
fn links_added_to_one_hub_at_once_all_land() {
    let vault = Vault::new();
    let hub = vault.new_task("hub");
    let targets: Vec<String> = (0..5)
        .map(|n| vault.new_document(&format!("output {n}")))
        .collect();

    let results = in_parallel(&TEAM, |agent| {
        let index = TEAM.iter().position(|a| a == &agent).unwrap();
        vault.run(
            agent,
            &[
                "link",
                &format!("task:{hub}"),
                &format!("document:{}", targets[index]),
                "--rel",
                "produces",
            ],
        )
    });

    assert_eq!(successes(&results), TEAM.len(), "{results:?}");
    assert_eq!(
        vault.edges_from(&format!("task:{hub}")).len(),
        TEAM.len(),
        "five writers to one frontmatter field, five edges"
    );
}

#[test]
fn links_removed_from_one_hub_at_once_all_disappear() {
    let vault = Vault::new();
    let hub = vault.new_task("hub");
    let targets: Vec<String> = (0..5)
        .map(|n| vault.new_document(&format!("output {n}")))
        .collect();
    for target in &targets {
        vault.ok(
            "alan",
            &[
                "link",
                &format!("task:{hub}"),
                &format!("document:{target}"),
                "--rel",
                "produces",
            ],
        );
    }

    let results = in_parallel(&TEAM, |agent| {
        let index = TEAM.iter().position(|a| a == &agent).unwrap();
        vault.run(
            agent,
            &[
                "unlink",
                &format!("task:{hub}"),
                &format!("document:{}", targets[index]),
                "--rel",
                "produces",
            ],
        )
    });

    assert_eq!(successes(&results), TEAM.len(), "{results:?}");
    assert!(
        vault.edges_from(&format!("task:{hub}")).is_empty(),
        "a removal that reports success has removed something"
    );
}

#[test]
fn fields_set_on_one_document_at_once_do_not_erase_each_other() {
    let vault = Vault::new();
    let id = vault.new_document("shared");
    let fields = ["owner", "severity", "area", "source"];

    let results = in_parallel(&fields, |field| {
        vault.run(field, &["set", &id, &format!("{field}=set-by-{field}")])
    });

    let present = vault.document(&id)["frontmatter"]
        .as_object()
        .unwrap()
        .len();
    assert_eq!(
        successes(&results),
        present,
        "a set that reported success is in the frontmatter: {results:?}"
    );
}

#[test]
fn a_task_can_only_be_started_once() {
    let vault = Vault::new();
    let id = vault.new_task("one start");
    vault.ok("claude", &["task", "claim", &id]);

    let codes = in_parallel(&["claude"; 3], |agent| {
        vault.raw(agent, &["task", "start", &id]).status.code()
    });

    assert_eq!(
        codes.iter().filter(|c| **c == Some(0)).count(),
        1,
        "the second start is not a no-op, it is a refusal: {codes:?}"
    );
    assert!(codes.iter().all(|c| *c == Some(0) || *c == Some(5)));
    assert_eq!(vault.status_of(&id), "in_progress");
}

#[test]
fn a_task_can_only_be_finished_once() {
    let vault = Vault::new();
    let id = vault.new_task("one finish");
    vault.ok("claude", &["task", "claim", &id]);

    let codes = in_parallel(&["claude"; 4], |agent| {
        vault.raw(agent, &["task", "done", &id]).status.code()
    });

    assert_eq!(
        codes.iter().filter(|c| **c == Some(0)).count(),
        1,
        "{codes:?}"
    );
    assert_eq!(vault.status_of(&id), "completed");
    assert_eq!(
        vault
            .event_topics()
            .iter()
            .filter(|t| *t == "task.finished")
            .count(),
        1,
        "one finish, one event: the log is what an auditor reads"
    );
}

#[test]
fn finishing_and_abandoning_at_once_leaves_exactly_one_terminal_status() {
    let vault = Vault::new();

    for round in 0..5 {
        let id = vault.new_task(&format!("contested {round}"));
        vault.ok("claude", &["task", "claim", &id]);

        let outcomes = in_parallel(&["claude", "claude"], |_| {
            vec![
                vault.raw("claude", &["task", "done", &id]),
                vault.raw("claude", &["task", "cancel", &id, "changed my mind"]),
            ]
        });
        let won = outcomes
            .iter()
            .flatten()
            .filter(|o| o.status.success())
            .count();

        let status = vault.status_of(&id);
        assert!(
            ["completed", "cancelled"].contains(&status.as_str()),
            "round {round} left `{status}`"
        );
        assert!(won >= 1, "somebody has to win round {round}");
    }
}

#[test]
fn a_contested_claim_records_one_holder_and_one_event() {
    let vault = Vault::new();
    let id = vault.new_task("one holder");

    let results = in_parallel(&TEAM, |agent| vault.run(agent, &["task", "claim", &id]));

    assert_eq!(successes(&results), 1, "{results:?}");
    let task = vault.json("alan", &["task", "get", &id])["task"].clone();
    assert!(task["claimed_by"].is_string(), "the winner is recorded");
    assert_eq!(
        vault
            .event_topics()
            .iter()
            .filter(|t| *t == "task.claimed")
            .count(),
        1,
        "a claim that was refused must not have logged itself"
    );
}

#[test]
fn a_wide_fan_of_children_finishing_together_rolls_the_parent_up_once() {
    let vault = Vault::new();
    let goal = vault.new_task("the goal");
    let names: Vec<String> = (0..8).map(|n| format!("part {n}")).collect();
    let mut split = vec!["task", "split", &goal];
    split.extend(names.iter().map(String::as_str));
    let created = vault.json("alan", &split);
    let children: Vec<String> = created["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect();

    let finished = in_parallel(
        &children.iter().map(String::as_str).collect::<Vec<_>>(),
        |child| {
            vault.run("claude", &["task", "claim", child])?;
            vault.run("claude", &["task", "done", child])
        },
    );
    assert_eq!(successes(&finished), children.len(), "{finished:?}");

    assert_eq!(vault.status_of(&goal), "completed");
    assert_eq!(
        vault
            .event_topics()
            .iter()
            .filter(|t| *t == "task.rolled_up")
            .count(),
        1,
        "eight children reporting at once still move the parent once"
    );
}

#[test]
fn a_parent_is_never_left_in_a_state_none_of_its_children_justify() {
    let vault = Vault::new();
    let goal = vault.new_task("mixed outcome");
    let split = vault.json(
        "alan",
        &["task", "split", &goal, "works", "fails", "dropped"],
    );
    let children: Vec<String> = split["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect();

    let outcomes = ["done", "fail", "cancel"];
    let finished = in_parallel(&["claude", "codex", "gemini"], |agent| {
        let index = ["claude", "codex", "gemini"]
            .iter()
            .position(|a| a == &agent)
            .unwrap();
        let child = &children[index];
        vault.run(agent, &["task", "claim", child])?;
        match outcomes[index] {
            "cancel" => vault.run(agent, &["task", "cancel", child, "not needed"]),
            "fail" => vault.run(agent, &["task", "fail", child, "broken"]),
            _ => vault.run(agent, &["task", "done", child]),
        }
    });
    assert_eq!(successes(&finished), 3, "{finished:?}");

    assert_eq!(
        vault.status_of(&goal),
        "failed",
        "one failed child makes the goal failed, whatever order they reported in"
    );
}

#[test]
fn repeated_acquire_and_release_never_admits_two_holders() {
    let vault = Vault::new();

    for round in 0..8 {
        let results = in_parallel(&TEAM, |agent| {
            vault.run(agent, &["lock", "acquire", "deploy/prod"])
        });
        let holders = successes(&results);
        assert_eq!(
            holders, 1,
            "round {round} admitted {holders} holders: {results:?}"
        );

        let holder = TEAM[results.iter().position(|r| r.is_ok()).unwrap()];
        vault.ok(holder, &["lock", "release", "deploy/prod"]);
    }
}

#[test]
fn agents_taking_two_locks_in_opposite_orders_all_terminate() {
    let vault = Vault::new();

    for _ in 0..6 {
        let outcomes = in_parallel(&["claude", "codex"], |agent| {
            let order = if agent == "claude" {
                ["schema", "migrations"]
            } else {
                ["migrations", "schema"]
            };
            let taken: Vec<bool> = order
                .iter()
                .map(|resource| vault.run(agent, &["lock", "acquire", resource]).is_ok())
                .collect();
            for (resource, held) in order.iter().zip(&taken) {
                if *held {
                    let _ = vault.run(agent, &["lock", "release", resource]);
                }
            }
            taken
        });

        assert_eq!(
            outcomes.len(),
            2,
            "a lock that queues instead of refusing would hang here"
        );
    }

    assert!(
        vault.run("gemini", &["lock", "acquire", "schema"]).is_ok(),
        "both resources are free again once everyone has let go"
    );
}

#[test]
fn one_agent_reading_two_messages_at_once_does_not_resurrect_either() {
    let vault = Vault::new();
    vault.ok("claude", &["announce"]);
    let ids: Vec<String> = (0..4)
        .map(|n| self::message(&vault, &format!("note {n}")))
        .collect();

    let results = in_parallel(&ids.iter().map(String::as_str).collect::<Vec<_>>(), |id| {
        vault.run("claude", &["msg", "read", id])
    });
    assert_eq!(successes(&results), ids.len(), "{results:?}");

    let unread = vault
        .json("claude", &["msg", "inbox"])
        .as_array()
        .unwrap()
        .len();
    assert_eq!(
        unread, 0,
        "an older watermark overwriting a newer one brings read messages back"
    );
}

fn message(vault: &Vault, body: &str) -> String {
    vault.json("alan", &["msg", "send", "broadcast", "--body", body])["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn concurrent_posts_all_reach_the_board() {
    let vault = Vault::new();
    vault.ok("claude", &["announce"]);

    let results = in_parallel(&TEAM, |agent| {
        vault.run(
            agent,
            &[
                "msg",
                "send",
                "broadcast",
                "--body",
                &format!("from {agent}"),
            ],
        )
    });
    assert_eq!(successes(&results), TEAM.len(), "{results:?}");

    let inbox = vault
        .json("claude", &["msg", "inbox"])
        .as_array()
        .unwrap()
        .len();
    assert_eq!(inbox, TEAM.len() - 1, "claude does not hear itself");
}

#[test]
fn a_storm_of_mixed_work_leaves_a_vault_that_still_reads_back() {
    let vault = Vault::new();
    for agent in TEAM {
        vault.ok(agent, &["announce"]);
    }
    let shared = vault.new_document("the shared page");
    let tasks: Vec<String> = (0..5)
        .map(|n| vault.new_task(&format!("job {n}")))
        .collect();

    in_parallel(&TEAM, |agent| {
        let index = TEAM.iter().position(|a| a == &agent).unwrap();
        let task = &tasks[index];
        let _ = vault.run(agent, &["task", "claim", task]);
        let _ = vault.run(
            agent,
            &["edit", &shared, "--content", &format!("from {agent}")],
        );
        let _ = vault.run(
            agent,
            &[
                "new",
                "discovery",
                &format!("found by {agent}"),
                "--body",
                "x",
            ],
        );
        let _ = vault.run(
            agent,
            &[
                "link",
                &format!("task:{task}"),
                &format!("document:{shared}"),
                "--rel",
                "produces",
            ],
        );
        let _ = vault.run(agent, &["msg", "send", "broadcast", "--body", "status"]);
        let _ = vault.run(agent, &["task", "done", task]);
    });

    for task in &tasks {
        assert_eq!(
            vault.status_of(task),
            "completed",
            "every claimed task finished under its own holder"
        );
    }
    assert!(
        vault.body_of(&shared).contains("from "),
        "the contended page survived"
    );

    let files = vault.files();
    let strays: Vec<_> = files
        .iter()
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".tmp"))
        })
        .collect();
    assert!(
        strays.is_empty(),
        "no half-written scratch files: {strays:?}"
    );

    let mut ids = Vec::new();
    for path in &files {
        let relative = path.strip_prefix(vault.path()).unwrap();
        if !relative.starts_with("docs") && !relative.starts_with("tasks") {
            continue;
        }
        let text = std::fs::read_to_string(path).unwrap();
        if let Some(line) = text.lines().find(|l| l.starts_with("id: ")) {
            ids.push(line.trim_start_matches("id: ").to_owned());
        }
    }
    let distinct: BTreeSet<&String> = ids.iter().collect();
    assert_eq!(
        distinct.len(),
        ids.len(),
        "a move that left the old copy behind would put one id in two files"
    );

    for id in &distinct {
        assert!(
            vault.run("alan", &["read", id]).is_ok()
                || vault.run("alan", &["task", "get", id]).is_ok(),
            "`{id}` is on disk but the index cannot reach it"
        );
    }
}

#[test]
fn identical_splits_of_one_goal_leave_one_subtask_per_title() {
    let vault = Vault::new();

    for round in 0..5 {
        let goal = vault.new_task(&format!("goal {round}"));
        let results = in_parallel(&["claude", "codex", "gemini"], |agent| {
            vault.run(agent, &["task", "split", &goal, "design", "build"])
        });
        assert!(results.iter().all(|r| r.is_ok()), "{results:?}");

        let subtasks = vault.json("alan", &["task", "get", &goal])["subtasks"]
            .as_array()
            .unwrap()
            .clone();
        let titles: Vec<&str> = subtasks
            .iter()
            .map(|s| s["title"].as_str().unwrap())
            .collect();
        assert_eq!(
            titles.len(),
            2,
            "round {round} split the same goal three times and kept {titles:?}"
        );
    }
}

#[test]
fn a_replace_that_loses_the_race_leaves_nothing_behind() {
    let vault = Vault::new();
    let original = vault.new_task("the old way");

    let results = in_parallel(&["claude", "codex", "gemini"], |agent| {
        vault.run(
            agent,
            &["task", "replace", &original, "first half", "second half"],
        )
    });
    assert_eq!(successes(&results), 1, "{results:?}");

    let titles: Vec<String> = vault.json("alan", &["task", "list", "--limit", "200"])["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        titles.iter().filter(|t| t.ends_with(" half")).count(),
        2,
        "a refused replace must not leave replacements standing in for a live task: {titles:?}"
    );
    assert_eq!(vault.status_of(&original), "superseded");
}

#[test]
fn promoting_and_archiving_one_document_at_once_settles_on_one_of_them() {
    let vault = Vault::new();

    for round in 0..5 {
        let id = vault.new_document(&format!("candidate {round}"));
        let outcomes = in_parallel(&["claude", "codex"], |agent| {
            if agent == "claude" {
                (
                    "promote",
                    vault.raw(agent, &["promote", &id, "--as", "decision"]),
                )
            } else {
                ("archive", vault.raw(agent, &["archive", &id]))
            }
        });

        let document = vault.document(&id);
        for (what, output) in &outcomes {
            let landed = match *what {
                "promote" => document["kind"] == "decision",
                _ => document["status"] == "archived",
            };
            if output.status.success() {
                assert!(
                    landed,
                    "`{what}` reported success in round {round} and did nothing"
                );
            } else {
                assert_eq!(output.status.code(), Some(5));
            }
        }

        let copies = vault
            .files()
            .iter()
            .filter(|p| {
                p.extension().is_some_and(|e| e == "md")
                    && std::fs::read_to_string(p)
                        .map(|t| t.contains(&id))
                        .unwrap_or(false)
            })
            .count();
        assert_eq!(
            copies, 1,
            "round {round} left the document in {copies} places"
        );
    }
}

#[test]
fn an_expired_lease_is_handed_to_exactly_one_waiting_agent() {
    let vault = Vault::new();
    vault.ok("claude", &["lock", "acquire", "deploy/prod", "--ttl", "1"]);
    thread::sleep(std::time::Duration::from_millis(1200));

    let results = in_parallel(&["codex", "gemini", "pi"], |agent| {
        vault.run(agent, &["lock", "acquire", "deploy/prod"])
    });
    assert_eq!(
        successes(&results),
        1,
        "an expiry frees the resource for one successor, not all of them: {results:?}"
    );
}

#[test]
fn a_listing_shows_the_work_another_process_created_after_this_one_started() {
    let vault = Vault::new();
    let goal = vault.new_task("the goal");

    // `split` is the one command that both writes subtasks and reads them back, so it is where
    // a listing served from a stale index would show a process only its own work
    vault.ok("claude", &["task", "split", &goal, "design"]);
    vault.ok("codex", &["task", "split", &goal, "build"]);

    let titles: Vec<String> = vault.json("alan", &["task", "get", &goal])["subtasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["title"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        titles.len(),
        2,
        "both agents' subtasks are there: {titles:?}"
    );
}

#[test]
fn blocking_and_finishing_one_task_at_once_leaves_one_coherent_status() {
    let vault = Vault::new();

    for round in 0..5 {
        let id = vault.new_task(&format!("contested {round}"));
        vault.ok("claude", &["task", "claim", &id]);

        let outcomes = in_parallel(&["claude", "claude"], |_| {
            vec![
                vault.raw(
                    "claude",
                    &["task", "block", &id, "--reason", "waiting on review"],
                ),
                vault.raw("claude", &["task", "done", &id]),
            ]
        });
        let won = outcomes
            .iter()
            .flatten()
            .filter(|o| o.status.success())
            .count();
        assert!(won >= 1, "round {round}: {outcomes:?}");

        let status = vault.status_of(&id);
        assert!(
            ["blocked", "completed"].contains(&status.as_str()),
            "round {round} left `{status}`, which neither command asked for"
        );
    }
}

#[test]
fn reparenting_a_task_from_two_goals_at_once_attaches_it_to_one() {
    let vault = Vault::new();
    let first = vault.new_task("first goal");
    let second = vault.new_task("second goal");
    let child = vault.new_task("the work");

    let goals = [first.clone(), second.clone()];
    let results = in_parallel(&["claude", "codex"], |agent| {
        let index = usize::from(agent == "codex");
        vault.run(
            agent,
            &["task", "update", &child, "--parent", &goals[index]],
        )
    });
    assert!(successes(&results) >= 1, "{results:?}");

    let parent = vault.json("alan", &["task", "get", &child])["task"]["parent"]
        .as_str()
        .map(str::to_owned);
    assert!(
        parent.as_deref() == Some(first.as_str()) || parent.as_deref() == Some(second.as_str()),
        "the child hangs under one of the two goals, not neither: {parent:?}"
    );

    let counted: usize = [&first, &second]
        .iter()
        .map(|goal| {
            vault.json("alan", &["task", "get", goal])["subtasks"]
                .as_array()
                .unwrap()
                .len()
        })
        .sum();
    assert_eq!(counted, 1, "and under only one of them");
}

#[test]
fn a_dependency_added_as_its_blocker_finishes_does_not_strand_the_task() {
    let vault = Vault::new();

    for round in 0..5 {
        let blocker = vault.new_task(&format!("blocker {round}"));
        let waiting = vault.new_task(&format!("waiting {round}"));
        vault.ok("claude", &["task", "claim", &blocker]);

        let outcomes = in_parallel(&["claude", "codex"], |agent| {
            if agent == "claude" {
                vault.raw(agent, &["task", "done", &blocker])
            } else {
                vault.raw(agent, &["task", "dep", &waiting, "--add", &blocker])
            }
        });
        assert!(outcomes.iter().any(|o| o.status.success()), "round {round}");

        assert_eq!(
            vault.status_of(&blocker),
            "completed",
            "the blocker finished whatever the dependency write did"
        );
        let status = vault.status_of(&waiting);
        assert!(
            ["pending", "blocked"].contains(&status.as_str()),
            "round {round} left the waiting task `{status}`"
        );
    }
}

#[test]
fn a_thread_read_by_everyone_at_once_stays_one_thread() {
    let vault = Vault::new();
    let readers = ["claude", "codex", "gemini", "pi"];
    for agent in readers {
        vault.ok(agent, &["announce"]);
    }
    let root = vault.json("alan", &["msg", "send", "broadcast", "--body", "opening"])["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let replies = in_parallel(&readers, |agent| {
        vault.run(
            agent,
            &[
                "msg",
                "send",
                "broadcast",
                "--reply-to",
                &root,
                "--body",
                &format!("from {agent}"),
            ],
        )
    });
    assert_eq!(successes(&replies), readers.len(), "{replies:?}");

    let thread = vault.json("alan", &["msg", "thread", &root]);
    assert_eq!(
        thread.as_array().unwrap().len(),
        readers.len() + 1,
        "four replies to one opening, all on the same thread"
    );
}
