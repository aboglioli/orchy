use clap::{Args, Subcommand};

use crate::client::OrchyClient;
use crate::config::Config;
use crate::output;

#[derive(Args)]
pub struct LockCommand {
    #[command(subcommand)]
    pub command: LockSubcommand,
}

#[derive(Subcommand)]
pub enum LockSubcommand {
    Acquire {
        name: String,
        #[arg(long)]
        ttl: Option<u64>,
    },
    Release {
        name: String,
    },
    Check {
        name: String,
    },
}

pub async fn run(
    cmd: &LockSubcommand,
    client: &OrchyClient,
    config: &Config,
) -> crate::client::CliResult<()> {
    match cmd {
        LockSubcommand::Acquire { name, ttl } => {
            let agent_id = client.agent_id.as_deref().unwrap_or("cli");
            let mut body = serde_json::json!({ "name": name, "agent_id": agent_id });
            if let Some(t) = ttl {
                body["ttl_secs"] = serde_json::Value::Number((*t).into());
            }
            let v = client.post_project_json("/locks", Some(&body)).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Lock '{name}' acquired.");
            }
        }
        LockSubcommand::Release { name } => {
            let agent_id = client.agent_id.as_deref().unwrap_or("cli");
            client
                .delete_project(&format!("/locks/{name}?agent_id={agent_id}"))
                .await?;
            if config.json {
                println!("{{\"ok\": true}}");
            } else {
                println!("Lock '{name}' released.");
            }
        }
        LockSubcommand::Check { name } => {
            let v = client.get_project_json(&format!("/locks/{name}")).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let holder = v.get("holder").and_then(|v| v.as_str()).unwrap_or("?");
                let expires = v.get("expires_at").and_then(|v| v.as_str()).unwrap_or("?");
                println!("Lock '{name}': holder={holder}, expires={expires}");
            }
        }
    }
    Ok(())
}
