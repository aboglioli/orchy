use clap::{Args, Subcommand};

use crate::client::OrchyClient;
use crate::config::Config;
use crate::output;

#[derive(Args)]
pub struct ProjectCommand {
    #[command(subcommand)]
    pub command: ProjectSubcommand,
}

#[derive(Subcommand)]
pub enum ProjectSubcommand {
    Get,
    Update {
        #[arg(long)]
        description: String,
    },
    SetMeta {
        #[arg(long)]
        key: String,
        #[arg(long)]
        value: String,
    },
    Namespaces,
}

pub async fn run(
    cmd: &ProjectSubcommand,
    client: &OrchyClient,
    config: &Config,
) -> crate::client::CliResult<()> {
    match cmd {
        ProjectSubcommand::Get => {
            let v = client.get_project_json("").await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let id = v.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                let desc = v.get("description").and_then(|v| v.as_str()).unwrap_or("");
                println!("Project: {id}");
                if !desc.is_empty() {
                    println!("Description: {desc}");
                }
                if let Some(meta) = v
                    .get("metadata")
                    .and_then(|v| v.as_object())
                    .filter(|m| !m.is_empty())
                {
                    println!("Metadata:");
                    for (k, v) in meta {
                        println!("  {k}: {}", v.as_str().unwrap_or("?"));
                    }
                }
            }
        }
        ProjectSubcommand::Update { description } => {
            let body = serde_json::json!({ "description": description });
            let v = client.put_project_json("", Some(&body)).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Project updated.");
            }
        }
        ProjectSubcommand::SetMeta { key, value } => {
            let body = serde_json::json!({ "key": key, "value": value });
            let v = client.post_project_json("/metadata", Some(&body)).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Metadata set: {key}={value}");
            }
        }
        ProjectSubcommand::Namespaces => {
            let v = client.get_project_json("/namespaces").await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let empty = vec![];
                let items = v.as_array().unwrap_or(&empty);
                if items.is_empty() {
                    println!("No namespaces.");
                } else {
                    for item in items {
                        println!("{}", item.as_str().unwrap_or("?"));
                    }
                }
            }
        }
    }
    Ok(())
}
