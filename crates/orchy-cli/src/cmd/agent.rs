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
    /// Register or resume an agent (creates if new, resumes if id exists)
    Register {
        #[arg(long)]
        description: Option<String>,
        #[arg(long, value_delimiter = ',')]
        roles: Option<Vec<String>>,
        #[arg(long)]
        id: Option<String>,
    },
    /// List all agents in the org
    List,
    /// Get full agent context
    Context {
        /// Agent ID
        id: String,
    },
    /// Change an agent's roles
    ChangeRoles {
        /// Agent ID
        id: String,
        /// New roles (comma-separated)
        #[arg(long)]
        roles: String,
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
            id,
        } => {
            let desc = description
                .clone()
                .or_else(|| config.description.clone())
                .unwrap_or_default();
            let agent_roles = roles.clone().unwrap_or_else(|| config.roles.clone());
            let alias = id.clone().or_else(|| config.alias.clone());
            let mut body = serde_json::json!({
                "description": desc,
                "roles": agent_roles,
            });
            if let Some(aid) = &alias {
                body["id"] = serde_json::Value::String(aid.clone());
            }
            let v = client.post_project_json("/agents", Some(&body)).await?;
            let new_id = v.get("id").and_then(|v| v.as_str()).unwrap_or("?");

            // Auto-save alias to .orchy.toml
            if new_id != "?" {
                crate::config::save_alias(new_id);
            }

            if config.json {
                output::print_json(config, &v);
            } else {
                let status = v.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                println!("Agent registered: {new_id} ({status})");
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
        AgentSubcommand::Context { id } => {
            let v = client.get_json(&format!("/agents/{id}/context")).await?;
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
                println!("Agent: {id} ({desc})  Status: {status}");
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
        AgentSubcommand::ChangeRoles { id, roles } => {
            let body = serde_json::json!({ "roles": roles.split(',').map(|s| s.trim()).collect::<Vec<_>>() });
            let v = client
                .patch_json(&format!("/agents/{id}/roles"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Roles updated for agent {id}.");
            }
        }
    }
    Ok(())
}
