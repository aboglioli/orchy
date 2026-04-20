use clap::{Args, Subcommand};

use crate::client::OrchyClient;
use crate::config::Config;
use crate::output;

#[derive(Args)]
pub struct EventCommand {
    #[command(subcommand)]
    pub command: EventSubcommand,
}

#[derive(Subcommand)]
pub enum EventSubcommand {
    Poll {
        #[arg(long)]
        after: Option<String>,
        #[arg(long)]
        limit: Option<u32>,
    },
}

pub async fn run(
    cmd: &EventSubcommand,
    client: &OrchyClient,
    config: &Config,
) -> crate::client::CliResult<()> {
    match cmd {
        EventSubcommand::Poll { after, limit } => {
            let mut qs = vec![];
            if let Some(a) = after {
                qs.push(format!("after={a}"));
            }
            if let Some(l) = limit {
                qs.push(format!("limit={l}"));
            }
            let query = if qs.is_empty() {
                String::new()
            } else {
                format!("?{}", qs.join("&"))
            };
            let v = client.get_project_json(&format!("/events{query}")).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let empty = vec![];
                let items = v
                    .as_array()
                    .or_else(|| v.get("items").and_then(|v| v.as_array()))
                    .unwrap_or(&empty);
                if items.is_empty() {
                    println!("No events.");
                } else {
                    for item in items {
                        let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                        let topic = item.get("topic").and_then(|v| v.as_str()).unwrap_or("?");
                        println!("{id} {topic}");
                    }
                }
            }
        }
    }
    Ok(())
}
