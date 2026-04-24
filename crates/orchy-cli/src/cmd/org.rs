use clap::{Args, Subcommand};

use crate::client::OrchyClient;
use crate::config::Config;
use crate::output;

#[derive(Args)]
pub struct OrgCommand {
    #[command(subcommand)]
    pub command: OrgSubcommand,
}

#[derive(Subcommand)]
pub enum OrgSubcommand {
    Create {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
    },
    #[command(subcommand)]
    ApiKey(OrgApiKeySubcommand),
}

#[derive(Subcommand)]
pub enum OrgApiKeySubcommand {
    Generate {
        #[arg(long)]
        name: String,
    },
    List,
}

pub async fn run(
    cmd: &OrgSubcommand,
    client: &OrchyClient,
    config: &Config,
) -> crate::client::CliResult<()> {
    match cmd {
        OrgSubcommand::Create { id, name } => {
            let v = client.create_organization_json(id, name).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let org_id = v.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                let org_name = v.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                println!("Organization created: {org_id} ({org_name})");
            }
        }
        OrgSubcommand::ApiKey(OrgApiKeySubcommand::Generate { name }) => {
            let v = client.generate_api_key_json(name).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let api_key = v.get("api_key").and_then(|v| v.as_str()).unwrap_or("?");
                println!("API key generated:");
                println!("  {api_key}");
                println!("Save this key — it cannot be retrieved again.");
            }
        }
        OrgSubcommand::ApiKey(OrgApiKeySubcommand::List) => {
            let v = client.list_api_keys_json().await?;
            if config.json {
                output::print_json(config, &v);
            } else if let Some(keys) = v.as_array() {
                for k in keys {
                    let name = k.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let prefix = k.get("key_prefix").and_then(|v| v.as_str()).unwrap_or("");
                    let suffix = k.get("key_suffix").and_then(|v| v.as_str()).unwrap_or("");
                    let active = k
                        .get("is_active")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let status = if active { "active" } else { "revoked" };
                    println!("  {name}: {prefix}...{suffix} ({status})");
                }
            }
        }
    }
    Ok(())
}
