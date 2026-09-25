mod cli;
mod cmd;
mod config;
mod container;
mod error;
mod init;
mod output;
mod resolve;
mod stdin;

use clap::{CommandFactory, Parser};
use orchy_application::list_actors::ListActorsCommand;
use orchy_application::manage_lease::LeaseAction;
use orchy_application::read_events::ReadEventsCommand;

use cli::{Cli, Command, LockCommand, SkillCommand};
use config::Config;
use error::{CliError, CliResult};
use output::{Output, short};

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let out = Output::new(cli.json, cli.no_color);

    match run(cli, &out).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("orchy: {e}");
            std::process::ExitCode::from(e.exit_code() as u8)
        }
    }
}

async fn run(cli: Cli, out: &Output) -> CliResult<()> {
    let Some(command) = cli.command else {
        use clap::CommandFactory;
        Cli::command().print_long_help()?;
        return Ok(());
    };
    let config = Config::resolve(cli.vault.clone(), cli.actor.clone())?;

    let command = match command {
        Command::Init { path } => {
            let root = path.unwrap_or_else(|| config.vault.clone());
            let written = init::scaffold(&root)?;
            return out.emit(&written, |w| {
                format!("initialised {}\n  {}", root.display(), w.join("\n  "))
            });
        }
        Command::Status => {
            let status = serde_json::json!({
                "vault": config.vault.display().to_string(),
                "actor": config.actor.to_string(),
                "machine": config.machine.to_string(),
                "initialised": config.is_initialised(),
                "settings": config::settings_path().display().to_string(),
            });
            return out.emit(&status, |s| {
                let initialised = s["initialised"].as_bool().unwrap_or(false);
                format!(
                    "vault    {}{}\nactor    {}\nmachine  {}\nsettings {}",
                    s["vault"].as_str().unwrap_or_default(),
                    if initialised {
                        String::new()
                    } else {
                        out.dim("  (not initialised — run `orchy init`)")
                    },
                    s["actor"].as_str().unwrap_or_default(),
                    s["machine"].as_str().unwrap_or_default(),
                    s["settings"].as_str().unwrap_or_default(),
                )
            });
        }
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "orchy", &mut std::io::stdout());
            return Ok(());
        }
        other => other,
    };

    if !config.is_initialised() {
        return Err(CliError::not_a_vault(config.vault.display()));
    }

    let app = container::build(&config).await?;
    let actor = config.actor.to_string();

    match command {
        Command::Init { .. } | Command::Status | Command::Completions { .. } => {
            unreachable!("answered before the vault is opened")
        }

        Command::Announce {
            roles,
            namespace,
            name,
        } => cmd::brief::announce(&app, &actor, roles, namespace, name, out).await,

        Command::Guide => cmd::brief::guide(out),

        Command::Skill(command) => match command {
            SkillCommand::Write {
                name,
                summary,
                namespace,
                body,
                tag,
            } => cmd::skill::write(&app, name, summary, namespace, body, tag, out).await,
            SkillCommand::Set {
                target,
                namespace,
                edits,
            } => cmd::skill::set(&app, target, namespace, edits, out).await,
            SkillCommand::List {
                namespace,
                tag,
                everywhere,
                retired,
            } => cmd::skill::list(&app, namespace, tag, everywhere, retired, out).await,
            SkillCommand::Find {
                query,
                namespace,
                tag,
                retired,
                limit,
            } => cmd::skill::find(&app, query, namespace, tag, retired, limit, out).await,
            SkillCommand::Show { target, namespace } => {
                cmd::skill::show(&app, target, namespace, out).await
            }
            SkillCommand::Retire { target } => cmd::skill::retire(&app, target, false, out).await,
            SkillCommand::Restore { target } => cmd::skill::retire(&app, target, true, out).await,
        },

        Command::Agents { live } => {
            let actors = app
                .list_actors
                .execute(ListActorsCommand { live_only: live })
                .await?;
            out.emit(&actors, |list| {
                if list.is_empty() {
                    return "no agents on the roster".to_owned();
                }
                list.iter()
                    .map(|a| {
                        format!(
                            "{:<40} {:<24} {}",
                            a.id,
                            a.roles.join(","),
                            a.last_seen.format("%Y-%m-%d %H:%M")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }

        Command::Types => {
            let types = serde_json::json!({
                "kinds": kind_names(),
                "relations": relation_names(),
            });
            out.emit(&types, |t| {
                format!(
                    "types\n  {}\n\nrelations\n  {}",
                    join(&t["kinds"]),
                    join(&t["relations"])
                )
            })
        }

        Command::Task(command) => cmd::task::run(&app, &actor, command, out).await,
        Command::Msg(command) => cmd::msg::run(&app, &actor, command, out).await,

        Command::New {
            kind,
            title,
            namespace,
            tag,
            body,
        } => cmd::doc::new(&app, kind, title, namespace, tag, body, out).await,

        Command::Read { target, section } => cmd::doc::read(&app, target, section, out).await,

        Command::Edit {
            target,
            section,
            replace_in,
            replace,
            if_match,
            content,
        } => {
            cmd::doc::edit(
                &app, target, section, replace_in, replace, if_match, content, out,
            )
            .await
        }

        Command::Set {
            target,
            assignments,
        } => cmd::doc::set(&app, target, assignments, out).await,

        Command::Recall {
            query,
            kind,
            entities,
            tag,
            namespace,
            anchor,
            limit,
        } => {
            cmd::doc::recall(
                &app, query, kind, entities, tag, namespace, anchor, limit, out,
            )
            .await
        }

        Command::Link { from, to, rel } => cmd::doc::link(&app, from, to, rel, false, out).await,
        Command::Unlink { from, to, rel } => cmd::doc::link(&app, from, to, rel, true, out).await,
        Command::Graph { from, depth } => cmd::doc::graph(&app, from, depth, out).await,
        Command::Supersede { old, by } => cmd::doc::supersede(&app, old, by, out).await,
        Command::Archive { target } => cmd::doc::set_status(&app, target, "archived", out).await,
        Command::Unarchive { target } => cmd::doc::set_status(&app, target, "active", out).await,
        Command::Promote {
            target,
            into,
            namespace,
        } => cmd::doc::promote(&app, target, into, namespace, out).await,

        Command::Lock(command) => match command {
            LockCommand::Acquire { resource, ttl } => {
                cmd::lock::manage(&app, &actor, resource, ttl, LeaseAction::Acquire, out).await
            }
            LockCommand::Renew { resource, ttl } => {
                cmd::lock::manage(&app, &actor, resource, ttl, LeaseAction::Renew, out).await
            }
            LockCommand::Release { resource } => {
                cmd::lock::manage(&app, &actor, resource, None, LeaseAction::Release, out).await
            }
            LockCommand::Check { resource } => {
                cmd::lock::manage(&app, &actor, resource, None, LeaseAction::Check, out).await
            }
            LockCommand::List => cmd::lock::list(&app, out).await,
            LockCommand::With {
                resource,
                ttl,
                command,
            } => cmd::lock::with(&app, &actor, resource, ttl, command, out).await,
        },

        Command::Events {
            topic,
            key,
            by,
            limit,
        } => {
            let events = app
                .read_events
                .execute(ReadEventsCommand {
                    topic_prefix: topic,
                    key,
                    actor: by,
                    since: None,
                    limit,
                })
                .await?;
            out.emit(&events, |list| {
                if list.is_empty() {
                    return "no events".to_owned();
                }
                list.iter()
                    .map(|e| {
                        format!(
                            "{}  {:<26} {}",
                            e.recorded_at.format("%Y-%m-%d %H:%M:%S"),
                            e.topic,
                            short(&e.key)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
    }
}

fn kind_names() -> Vec<String> {
    orchy_core::Kind::ALL
        .iter()
        .map(|k| {
            format!(
                "{k} ({})",
                k.statuses()
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
        })
        .collect()
}

fn relation_names() -> Vec<String> {
    orchy_core::Relation::ALL
        .iter()
        .map(|r| {
            let managed = match r.managed_by() {
                Some(command) => format!("  — set via {command}"),
                None => String::new(),
            };
            format!("{r} → {}{managed}", r.inverse())
        })
        .collect()
}

fn join(value: &serde_json::Value) -> String {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.as_str())
                .collect::<Vec<_>>()
                .join("\n  ")
        })
        .unwrap_or_default()
}
