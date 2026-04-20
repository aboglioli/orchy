use clap::{Args, Subcommand};

use crate::client::OrchyClient;
use crate::config::Config;
use crate::output;

#[derive(Args)]
pub struct EdgeCommand {
    #[command(subcommand)]
    pub command: EdgeSubcommand,
}

#[derive(Subcommand)]
pub enum EdgeSubcommand {
    Add {
        #[arg(long)]
        from_kind: String,
        #[arg(long)]
        from_id: String,
        #[arg(long)]
        to_kind: String,
        #[arg(long)]
        to_id: String,
        #[arg(long)]
        rel_type: String,
        #[arg(long)]
        if_not_exists: Option<bool>,
    },
    Remove {
        edge_id: String,
    },
    Query {
        #[arg(long)]
        anchor_kind: String,
        #[arg(long)]
        anchor_id: String,
        #[arg(long)]
        rel_types: Option<String>,
        #[arg(long)]
        direction: Option<String>,
        #[arg(long)]
        max_depth: Option<u32>,
    },
    List {
        #[arg(long)]
        from_kind: Option<String>,
        #[arg(long)]
        from_id: Option<String>,
        #[arg(long)]
        to_kind: Option<String>,
        #[arg(long)]
        to_id: Option<String>,
        #[arg(long)]
        rel_type: Option<String>,
    },
}

pub async fn run(
    cmd: &EdgeSubcommand,
    client: &OrchyClient,
    config: &Config,
) -> crate::client::CliResult<()> {
    match cmd {
        EdgeSubcommand::Add {
            from_kind,
            from_id,
            to_kind,
            to_id,
            rel_type,
            if_not_exists,
        } => {
            let mut body = serde_json::json!({
                "from_kind": from_kind,
                "from_id": from_id,
                "to_kind": to_kind,
                "to_id": to_id,
                "rel_type": rel_type,
            });
            if let Some(ine) = if_not_exists {
                body["if_not_exists"] = serde_json::Value::Bool(*ine);
            }
            let v = client.post_json("/graph/edges", Some(&body)).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let id = v.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                println!("Edge created: {id}");
            }
        }
        EdgeSubcommand::Remove { edge_id } => {
            client.delete(&format!("/graph/edges/{edge_id}")).await?;
            if config.json {
                println!("{{\"ok\": true}}");
            } else {
                println!("Edge {edge_id} removed.");
            }
        }
        EdgeSubcommand::Query {
            anchor_kind,
            anchor_id,
            rel_types,
            direction,
            max_depth,
        } => {
            let mut qs = vec![];
            qs.push(format!("anchor_kind={anchor_kind}"));
            qs.push(format!("anchor_id={anchor_id}"));
            if let Some(rt) = rel_types {
                qs.push(format!("rel_types={rt}"));
            }
            if let Some(d) = direction {
                qs.push(format!("direction={d}"));
            }
            if let Some(md) = max_depth {
                qs.push(format!("max_depth={md}"));
            }
            let query = format!("?{}", qs.join("&"));
            let v = client.get_json(&format!("/graph/relations{query}")).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                if let Some(edges) = v.get("edges").and_then(|v| v.as_array()) {
                    println!("Edges ({}):", edges.len());
                    for e in edges {
                        print!("{}", output::format_edge(e));
                    }
                }
                if let Some(nodes) = v.get("nodes").and_then(|v| v.as_array()) {
                    println!("\nNodes ({}):", nodes.len());
                    for n in nodes {
                        println!("  {}", n.as_str().unwrap_or("?"));
                    }
                }
            }
        }
        EdgeSubcommand::List {
            from_kind,
            from_id,
            to_kind,
            to_id,
            rel_type,
        } => {
            let mut qs = vec![];
            if let Some(fk) = from_kind {
                qs.push(format!("from_kind={fk}"));
            }
            if let Some(fi) = from_id {
                qs.push(format!("from_id={fi}"));
            }
            if let Some(tk) = to_kind {
                qs.push(format!("to_kind={tk}"));
            }
            if let Some(ti) = to_id {
                qs.push(format!("to_id={ti}"));
            }
            if let Some(rt) = rel_type {
                qs.push(format!("rel_type={rt}"));
            }
            let query = if qs.is_empty() {
                String::new()
            } else {
                format!("?{}", qs.join("&"))
            };
            let v = client.get_json(&format!("/graph/edges{query}")).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let items = v.get("items").and_then(|v| v.as_array());
                match items {
                    None => println!("No edges found."),
                    Some(arr) if arr.is_empty() => println!("No edges found."),
                    Some(arr) => {
                        for e in arr {
                            print!("{}", output::format_edge(e));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
