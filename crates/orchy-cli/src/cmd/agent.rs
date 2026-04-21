use clap::{Args, Subcommand};

use crate::client::OrchyClient;
use crate::config::Config;
use crate::output;

/// Agent commands
#[derive(Args)]
pub struct AgentCommand {
    #[command(subcommand)]
    pub command: AgentSubcommand,
}

#[derive(Subcommand)]
pub enum AgentSubcommand {
    /// Register or resume an agent (creates if new, resumes if alias exists)
    Register {
        #[arg(long)]
        description: Option<String>,
        #[arg(long, value_delimiter = ',')]
        roles: Option<Vec<String>>,
        #[arg(long)]
        alias: Option<String>,
    },
    /// List all agents in the org
    List,
    /// Get full agent context by alias
    Context {
        /// Agent alias
        alias: String,
    },
    /// Change an agent's roles
    ChangeRoles {
        /// Agent alias
        alias: String,
        /// New roles (comma-separated)
        #[arg(long)]
        roles: String,
    },
    /// Rename an agent's alias
    Rename {
        /// Current agent alias or ID
        alias: String,
        /// New alias
        #[arg(long)]
        new_alias: String,
    },
    /// Switch agent to a different project or namespace
    Switch {
        /// Agent alias or ID
        alias: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Get agent summary (tasks, knowledge, activity)
    Summary {
        /// Agent alias or ID
        alias: String,
    },
}

pub async fn run(
    cmd: &AgentSubcommand,
    client: &OrchyClient,
    config: &Config,
) -> crate::client::CliResult<()> {
    match cmd {
        AgentSubcommand::Register {
            description,
            roles,
            alias,
        } => {
            let desc = description
                .clone()
                .or_else(|| config.description.clone())
                .unwrap_or_default();
            let agent_roles = roles.clone().unwrap_or_else(|| config.roles.clone());
            let alias = alias.clone().or_else(|| config.alias.clone());
            let mut body = serde_json::json!({
                "description": desc,
                "roles": agent_roles,
            });
            if let Some(a) = &alias {
                body["alias"] = serde_json::Value::String(a.clone());
            }
            let v = client.post_project_json("/agents", Some(&body)).await?;
            let new_alias = v
                .get("agent")
                .and_then(|a| a.get("alias"))
                .and_then(|v| v.as_str())
                .or_else(|| alias.as_deref())
                .unwrap_or("?");

            // Auto-save alias to .orchy.toml
            if new_alias != "?" {
                crate::config::save_alias(new_alias);
            }

            if config.json {
                output::print_json(config, &v);
            } else {
                let status = v
                    .get("agent")
                    .and_then(|a| a.get("status"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                println!("Agent registered: {new_alias} ({status})");
                println!("Saved alias to .orchy.toml");
            }
        }
        AgentSubcommand::List => {
            let v = client.get_json("/agents").await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let empty = vec![];
                let items = v.as_array().unwrap_or(&empty);
                for a in items {
                    print!("{}", output::format_agent(a));
                }
            }
        }
        AgentSubcommand::Context { alias } => {
            let v = client
                .get_json(&format!(
                    "/agents/{alias}/context?project={}",
                    client.project
                ))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let desc = v
                    .get("agent")
                    .and_then(|a| a.get("description"))
                    .and_then(|d| d.as_str())
                    .unwrap_or("?");
                let status = v
                    .get("agent")
                    .and_then(|a| a.get("status"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("?");
                println!("Agent: {alias} ({desc})  Status: {status}");
                if let Some(inbox) = v.get("inbox").and_then(|v| v.as_array()) {
                    println!("Inbox ({}):", inbox.len());
                    for m in inbox {
                        print!("{}", output::format_message(m));
                    }
                }
                if let Some(tasks) = v.get("pending_tasks").and_then(|v| v.as_array()) {
                    println!("Pending tasks ({}):", tasks.len());
                    for t in tasks {
                        print!("{}", output::format_task(t));
                    }
                }
            }
        }
        AgentSubcommand::ChangeRoles { alias, roles } => {
            let body = serde_json::json!({ "roles": roles.split(',').map(|s| s.trim()).collect::<Vec<_>>() });
            let v = client
                .patch_json(
                    &format!("/agents/{alias}/roles?project={}", client.project),
                    Some(&body),
                )
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Roles updated for agent {alias}.");
            }
        }
        AgentSubcommand::Rename { alias, new_alias } => {
            let body = serde_json::json!({ "new_alias": new_alias });
            let v = client
                .post_json(
                    &format!("/agents/{alias}/rename?project={}", client.project),
                    Some(&body),
                )
                .await?;
            crate::config::save_alias(new_alias);
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Agent '{alias}' renamed to '{new_alias}'.");
                println!("Saved alias to .orchy.toml");
            }
        }
        AgentSubcommand::Switch {
            alias,
            project,
            namespace,
        } => {
            let mut body = serde_json::json!({});
            if let Some(p) = project {
                body["project"] = serde_json::Value::String(p.clone());
            }
            if let Some(ns) = namespace {
                body["namespace"] = serde_json::Value::String(ns.clone());
            }
            let v = client
                .post_json(
                    &format!("/agents/{alias}/switch-context?project={}", client.project),
                    Some(&body),
                )
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Agent '{alias}' context switched.");
            }
        }
        AgentSubcommand::Summary { alias } => {
            let v = client
                .get_json(&format!(
                    "/agents/{alias}/summary?project={}",
                    client.project
                ))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                print!("{}", output::format_agent_summary(&v));
            }
        }
    }
    Ok(())
}
