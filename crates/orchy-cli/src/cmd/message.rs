use clap::{Args, Subcommand};

use crate::client::OrchyClient;
use crate::config::Config;
use crate::output;

#[derive(Args)]
pub struct MessageCommand {
    #[command(subcommand)]
    pub command: MessageSubcommand,
}

#[derive(Subcommand)]
pub enum MessageSubcommand {
    Send {
        #[arg(long)]
        to: String,
        #[arg(long)]
        body: String,
        #[arg(long)]
        reply_to: Option<String>,
        #[arg(long)]
        refs: Option<String>,
    },
    Inbox {
        #[arg(long)]
        limit: Option<u32>,
    },
    Sent {
        #[arg(long)]
        limit: Option<u32>,
    },
    MarkRead {
        #[arg(long)]
        ids: String,
    },
    Thread {
        id: String,
        #[arg(long)]
        limit: Option<u32>,
    },
}

pub async fn run(
    cmd: &MessageSubcommand,
    client: &OrchyClient,
    config: &Config,
) -> crate::client::CliResult<()> {
    match cmd {
        MessageSubcommand::Send {
            to,
            body,
            reply_to,
            refs,
        } => {
            let alias = client.alias.as_deref().unwrap_or("cli");
            let mut body_json = serde_json::json!({ "from_alias": alias, "to": to, "body": body });
            if let Some(rt) = reply_to {
                body_json["reply_to"] = serde_json::Value::String(rt.clone());
            }
            if let Some(r) = refs {
                let refs_vec: Vec<serde_json::Value> = r
                    .split(',')
                    .filter_map(|s| {
                        let parts: Vec<_> = s.split(':').collect();
                        if parts.len() >= 2 {
                            Some(serde_json::json!({
                                "kind": parts[0],
                                "id": parts[1..].join(":"),
                            }))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !refs_vec.is_empty() {
                    body_json["refs"] = serde_json::Value::Array(refs_vec);
                }
            }
            let v = client
                .post_project_json("/messages", Some(&body_json))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let id = v.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                println!("Message sent: {id}");
            }
        }
        MessageSubcommand::Inbox { limit } => {
            let mut qs = vec![];
            if let Some(l) = limit {
                qs.push(format!("limit={l}"));
            }
            let query = if qs.is_empty() {
                String::new()
            } else {
                format!("?{}", qs.join("&"))
            };
            let alias = client
                .alias
                .clone()
                .ok_or(crate::client::CliError::MissingAgentId)?;
            let v = client
                .get_json(&format!("/agents/{alias}/inbox{query}"))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let empty = vec![];
                let items = v
                    .as_array()
                    .or_else(|| v.get("items").and_then(|v| v.as_array()))
                    .unwrap_or(&empty);
                print!("{}", output::format_message_list(items));
            }
        }
        MessageSubcommand::Sent { limit } => {
            let mut qs = vec![];
            if let Some(l) = limit {
                qs.push(format!("limit={l}"));
            }
            let query = if qs.is_empty() {
                String::new()
            } else {
                format!("?{}", qs.join("&"))
            };
            let alias = client
                .alias
                .clone()
                .ok_or(crate::client::CliError::MissingAgentId)?;
            let v = client
                .get_json(&format!("/agents/{alias}/sent-messages{query}"))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let empty = vec![];
                let items = v
                    .as_array()
                    .or_else(|| v.get("items").and_then(|v| v.as_array()))
                    .unwrap_or(&empty);
                print!("{}", output::format_message_list(items));
            }
        }
        MessageSubcommand::MarkRead { ids } => {
            let id_vec: Vec<String> = ids.split(',').map(|s| s.trim().to_string()).collect();
            let body = serde_json::json!({ "message_ids": id_vec });
            let alias = client
                .alias
                .clone()
                .ok_or(crate::client::CliError::MissingAgentId)?;
            let v = client
                .post_json(&format!("/agents/{alias}/messages/read"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Messages marked read.");
            }
        }
        MessageSubcommand::Thread { id, limit } => {
            let mut qs = vec![];
            if let Some(l) = limit {
                qs.push(format!("limit={l}"));
            }
            let query = if qs.is_empty() {
                String::new()
            } else {
                format!("?{}", qs.join("&"))
            };
            let v = client
                .get_project_json(&format!("/messages/{id}/thread{query}"))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let empty = vec![];
                let items = v
                    .as_array()
                    .or_else(|| v.get("items").and_then(|v| v.as_array()))
                    .unwrap_or(&empty);
                print!("{}", output::format_message_list(items));
            }
        }
    }
    Ok(())
}
