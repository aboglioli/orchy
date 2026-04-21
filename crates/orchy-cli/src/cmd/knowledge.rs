use clap::{Args, Subcommand};

use crate::client::OrchyClient;
use crate::config::Config;
use crate::output;

/// Knowledge commands
#[derive(Args)]
pub struct KnowledgeCommand {
    #[command(subcommand)]
    pub command: KnowledgeSubcommand,
}

#[derive(Subcommand)]
pub enum KnowledgeSubcommand {
    /// List knowledge entries
    List {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        path_prefix: Option<String>,
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        orphaned: Option<bool>,
        #[arg(long)]
        after: Option<String>,
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Write (create or update) a knowledge entry
    Write {
        /// Hierarchical path (e.g. auth/jwt-strategy)
        path: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        content: String,
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long)]
        task_id: Option<String>,
    },
    /// Read a knowledge entry by path
    Read {
        path: String,
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        rel_types: Option<String>,
    },
    /// Delete a knowledge entry
    Delete {
        path: String,
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Search knowledge entries
    Search {
        #[arg(long)]
        query: String,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        limit: Option<u32>,
    },
    /// List available knowledge types
    Types,
    /// Append text to a knowledge entry
    Append {
        path: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        content: String,
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Rename a knowledge entry
    Rename {
        path: String,
        #[arg(long)]
        new_path: String,
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Move a knowledge entry to a different namespace or project
    Move {
        path: String,
        #[arg(long)]
        new_namespace: Option<String>,
        #[arg(long)]
        new_project: Option<String>,
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Tag a knowledge entry
    Tag { path: String, tag: String },
    /// Untag a knowledge entry
    Untag { path: String, tag: String },
    /// Change the kind of a knowledge entry
    ChangeKind {
        path: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Import a knowledge entry from a linked project
    Import {
        #[arg(long)]
        source_project: String,
        #[arg(long)]
        path: String,
        #[arg(long)]
        source_namespace: Option<String>,
    },
    /// Patch metadata without changing content
    PatchMeta {
        path: String,
        #[arg(long)]
        set: Vec<String>,
        #[arg(long)]
        remove: Vec<String>,
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Assemble rich context from the edge graph
    Context {
        /// Resource kind: task, knowledge, agent
        kind: String,
        /// Resource ID
        id: String,
        #[arg(long)]
        max_tokens: Option<usize>,
    },
}

pub async fn run(
    cmd: &KnowledgeSubcommand,
    client: &OrchyClient,
    config: &Config,
) -> crate::client::CliResult<()> {
    match cmd {
        KnowledgeSubcommand::List {
            kind,
            tag,
            path_prefix,
            namespace,
            orphaned,
            after,
            limit,
        } => {
            let mut qs = vec![];
            if let Some(k) = kind {
                qs.push(format!("kind={k}"));
            }
            if let Some(t) = tag {
                qs.push(format!("tag={t}"));
            }
            if let Some(pp) = path_prefix {
                qs.push(format!("path_prefix={pp}"));
            }
            if let Some(ns) = namespace {
                qs.push(format!("namespace={ns}"));
            }
            if let Some(o) = orphaned {
                qs.push(format!("orphaned={o}"));
            }
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
            let v = client
                .get_project_json(&format!("/knowledge{query}"))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let empty = vec![];
                let items = v
                    .as_array()
                    .or_else(|| v.get("items").and_then(|v| v.as_array()))
                    .unwrap_or(&empty);
                print!("{}", output::format_knowledge_list(items));
            }
        }

        KnowledgeSubcommand::Write {
            path,
            kind,
            title,
            content,
            namespace,
            tags,
            task_id,
        } => {
            let mut body = serde_json::json!({
                "kind": kind,
                "title": title,
                "content": content,
            });
            if let Some(ns) = namespace {
                body["namespace"] = serde_json::Value::String(ns.clone());
            }
            if let Some(t) = tags {
                body["tags"] = serde_json::Value::Array(
                    t.split(',')
                        .map(|s| serde_json::Value::String(s.trim().to_string()))
                        .collect(),
                );
            }
            if let Some(tid) = task_id {
                body["task_id"] = serde_json::Value::String(tid.clone());
            }
            if let Some(aid) = &client.alias {
                body["alias"] = serde_json::Value::String(aid.clone());
            }
            let v = client
                .put_project_json(&format!("/knowledge/{path}"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Knowledge entry '{path}' written.");
            }
        }

        KnowledgeSubcommand::Read {
            path,
            namespace,
            rel_types,
        } => {
            let mut qs = vec![];
            if let Some(ns) = namespace {
                qs.push(format!("namespace={ns}"));
            }
            if let Some(rt) = rel_types {
                qs.push(format!("rel_types={rt}"));
            }
            let query = if qs.is_empty() {
                String::new()
            } else {
                format!("?{}", qs.join("&"))
            };
            let v = client
                .get_project_json(&format!("/knowledge/{path}{query}"))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                print!("{}", output::format_knowledge(&v));
            }
        }

        KnowledgeSubcommand::Delete { path, namespace } => {
            let mut qs = vec![];
            if let Some(ns) = namespace {
                qs.push(format!("namespace={ns}"));
            }
            let query = if qs.is_empty() {
                String::new()
            } else {
                format!("?{}", qs.join("&"))
            };
            client
                .delete_project(&format!("/knowledge/{path}{query}"))
                .await?;
            if config.json {
                println!("{{\"ok\": true}}");
            } else {
                println!("Knowledge entry '{path}' deleted.");
            }
        }

        KnowledgeSubcommand::Search { query, kind, limit } => {
            let mut body = serde_json::json!({ "query": query });
            if let Some(k) = kind {
                body["kind"] = serde_json::Value::String(k.clone());
            }
            if let Some(l) = limit {
                body["limit"] = serde_json::Value::Number((*l).into());
            }
            let v = client
                .post_project_json("/knowledge/search", Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let empty = vec![];
                let items = v.as_array().unwrap_or(&empty);
                print!("{}", output::format_knowledge_list(items));
            }
        }

        KnowledgeSubcommand::Types => {
            let v = client.get_project_json("/knowledge/types").await?;
            if config.json {
                output::print_json(config, &v);
            } else if let Some(items) = v.as_array() {
                for t in items {
                    let typ = t.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                    let desc = t.get("description").and_then(|v| v.as_str()).unwrap_or("");
                    println!("{typ:>12}  {desc}");
                }
            }
        }

        KnowledgeSubcommand::Append {
            path,
            kind,
            content,
            namespace,
        } => {
            let mut body = serde_json::json!({ "kind": kind, "value": content });
            if let Some(ns) = namespace {
                body["namespace"] = serde_json::Value::String(ns.clone());
            }
            let v = client
                .post_project_json(&format!("/knowledge/{path}/append"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Content appended to {path}.");
            }
        }

        KnowledgeSubcommand::Rename {
            path,
            new_path,
            namespace,
        } => {
            let mut body = serde_json::json!({ "new_path": new_path });
            if let Some(ns) = namespace {
                body["namespace"] = serde_json::Value::String(ns.clone());
            }
            let v = client
                .post_project_json(&format!("/knowledge/{path}/rename"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Knowledge entry '{path}' renamed to '{new_path}'.");
            }
        }

        KnowledgeSubcommand::Move {
            path,
            new_namespace,
            new_project,
            namespace,
        } => {
            let mut body = serde_json::json!({});
            if let Some(ns) = new_namespace {
                body["new_namespace"] = serde_json::Value::String(ns.clone());
            }
            if let Some(np) = new_project {
                body["new_project"] = serde_json::Value::String(np.clone());
            }
            if let Some(ns) = namespace {
                body["namespace"] = serde_json::Value::String(ns.clone());
            }
            let v = client
                .post_project_json(&format!("/knowledge/{path}/move"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Knowledge entry '{path}' moved.");
            }
        }

        KnowledgeSubcommand::Tag { path, tag } => {
            let v = client
                .post_project_json(
                    &format!("/knowledge/{path}/tags/{tag}"),
                    Some(&serde_json::json!({})),
                )
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Tag '{tag}' added to {path}.");
            }
        }

        KnowledgeSubcommand::Untag { path, tag } => {
            client
                .delete_project(&format!("/knowledge/{path}/tags/{tag}"))
                .await?;
            if config.json {
                println!("{{\"ok\": true}}");
            } else {
                println!("Tag '{tag}' removed from {path}.");
            }
        }

        KnowledgeSubcommand::ChangeKind {
            path,
            kind,
            namespace,
        } => {
            let mut body = serde_json::json!({ "kind": kind });
            if let Some(ns) = namespace {
                body["namespace"] = serde_json::Value::String(ns.clone());
            }
            let v = client
                .patch_project_json(&format!("/knowledge/{path}/kind"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Kind of '{path}' changed to {kind}.");
            }
        }

        KnowledgeSubcommand::Import {
            source_project,
            path,
            source_namespace,
        } => {
            let mut body = serde_json::json!({ "source_project": source_project, "path": path });
            if let Some(ns) = source_namespace {
                body["source_namespace"] = serde_json::Value::String(ns.clone());
            }
            let v = client
                .post_project_json("/knowledge/import", Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Knowledge entry '{path}' imported from {source_project}.");
            }
        }

        KnowledgeSubcommand::PatchMeta {
            path,
            set,
            remove,
            namespace,
        } => {
            let mut body = serde_json::json!({});
            if !set.is_empty() {
                let map: std::collections::HashMap<String, String> = set
                    .iter()
                    .filter_map(|s| {
                        s.split_once('=')
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                    })
                    .collect();
                body["set"] = serde_json::to_value(&map).unwrap();
            }
            if !remove.is_empty() {
                body["remove"] = serde_json::Value::Array(
                    remove
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                );
            }
            if let Some(ns) = namespace {
                body["namespace"] = serde_json::Value::String(ns.clone());
            }
            let v = client
                .patch_project_json(&format!("/knowledge/{path}/metadata"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Metadata patched for '{path}'.");
            }
        }

        KnowledgeSubcommand::Context {
            kind,
            id,
            max_tokens,
        } => {
            let mut body = serde_json::json!({ "kind": kind, "id": id });
            if let Some(mt) = max_tokens {
                body["max_tokens"] = serde_json::Value::Number((*mt).into());
            }
            let v = client.post_json("/graph/context", Some(&body)).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                if let Some(facts) = v.get("core_facts").and_then(|v| v.as_array()) {
                    println!("Core Facts:");
                    for f in facts {
                        print!("{}", output::format_knowledge(f));
                    }
                }
                if let Some(deps) = v
                    .get("open_dependencies")
                    .and_then(|v| v.as_array())
                    .filter(|a| !a.is_empty())
                {
                    println!("\nOpen Dependencies:");
                    for d in deps {
                        print!("{}", output::format_task(d));
                    }
                }
                if let Some(decisions) = v
                    .get("relevant_decisions")
                    .and_then(|v| v.as_array())
                    .filter(|a| !a.is_empty())
                {
                    println!("\nRelevant Decisions:");
                    for d in decisions {
                        print!("{}", output::format_knowledge(d));
                    }
                }
                if let Some(risks) = v
                    .get("risk_flags")
                    .and_then(|v| v.as_array())
                    .filter(|a| !a.is_empty())
                {
                    println!("\nRisk Flags:");
                    for r in risks {
                        println!("  - {}", r.as_str().unwrap_or("?"));
                    }
                }
            }
        }
    }
    Ok(())
}
