use clap::{Args, Subcommand};

use crate::client::OrchyClient;
use crate::config::Config;
use crate::output;

/// Task commands
#[derive(Args)]
pub struct TaskCommand {
    #[command(subcommand)]
    pub command: TaskSubcommand,
}

#[derive(Subcommand)]
pub enum TaskSubcommand {
    /// Create a new task
    Create {
        #[arg(long)]
        title: String,
        #[arg(long)]
        description: String,
        #[arg(long)]
        acceptance_criteria: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        roles: Option<String>,
    },
    /// List tasks
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        archived: bool,
        #[arg(long)]
        after: Option<String>,
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Get a task by ID with full context
    Get {
        id: String,
        #[arg(long)]
        rel_types: Option<String>,
        #[arg(long)]
        direction: Option<String>,
        #[arg(long)]
        max_depth: Option<u32>,
    },
    /// Update a task
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        acceptance_criteria: Option<String>,
        #[arg(long)]
        priority: Option<String>,
    },
    /// Claim a task
    Claim {
        id: String,
        #[arg(long)]
        start: Option<bool>,
    },
    /// Start a claimed task
    Start { id: String },
    /// Complete a task
    Complete {
        id: String,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long, help = "Links as kind:id:rel_type (comma-separated)")]
        links: Option<String>,
    },
    /// Fail a task
    Fail {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Cancel a task
    Cancel {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Archive a completed/failed/cancelled task
    Archive {
        task_id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Unarchive a task
    Unarchive { task_id: String },
    /// Release a claimed/in-progress task
    Release { id: String },
    /// Unblock a blocked task
    Unblock { id: String },
    /// Assign a task to another agent
    Assign {
        id: String,
        #[arg(long)]
        agent: String,
    },
    /// Get the next available task matching your roles
    Next {
        #[arg(long)]
        claim: Option<bool>,
        #[arg(long)]
        role: Option<String>,
    },
    /// Split a task into subtasks
    Split {
        id: String,
        #[arg(long, value_delimiter = ',')]
        titles: Vec<String>,
    },
    /// Merge multiple tasks into one
    Merge {
        #[arg(long)]
        ids: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        description: String,
    },
    /// Cancel a task and create replacements
    Replace {
        id: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long, value_delimiter = ',')]
        titles: Vec<String>,
    },
    /// Create a subtask without blocking the parent
    Delegate {
        id: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        description: String,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        roles: Option<String>,
    },
    /// Move a task to a different namespace
    Move {
        id: String,
        #[arg(long)]
        new_namespace: String,
    },
    /// Add a tag to a task
    Tag { id: String, tag: String },
    /// Remove a tag from a task
    Untag { id: String, tag: String },
    /// Keep a task alive (prevent staleness)
    Touch { id: String },
    /// List all unique tags used across project tasks
    ListTags,
    /// Add a dependency
    AddDep {
        id: String,
        #[arg(long)]
        dep: String,
    },
    /// Remove a dependency
    RemoveDep {
        id: String,
        #[arg(long)]
        dep: String,
    },
}

fn require_alias(client: &OrchyClient) -> Result<String, crate::client::CliError> {
    client
        .alias
        .clone()
        .ok_or(crate::client::CliError::MissingAgentId)
}

pub async fn run(
    cmd: &TaskSubcommand,
    client: &OrchyClient,
    config: &Config,
) -> crate::client::CliResult<()> {
    match cmd {
        TaskSubcommand::Create {
            title,
            description,
            acceptance_criteria,
            priority,
            roles,
        } => {
            let mut body = serde_json::json!({ "title": title, "description": description });
            if let Some(ac) = acceptance_criteria {
                body["acceptance_criteria"] = serde_json::Value::String(ac.clone());
            }
            if let Some(p) = priority {
                body["priority"] = serde_json::Value::String(p.clone());
            }
            if let Some(r) = roles {
                body["assigned_roles"] = serde_json::Value::Array(
                    r.split(',')
                        .map(|s| serde_json::Value::String(s.trim().to_string()))
                        .collect(),
                );
            }
            let v = client.post_project_json("/tasks", Some(&body)).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let id = v.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                println!("Task {id} created.");
            }
        }

        TaskSubcommand::List {
            status,
            namespace,
            archived,
            after,
            limit,
        } => {
            let mut qs = vec![];
            if let Some(s) = status {
                qs.push(format!("status={s}"));
            }
            if let Some(ns) = namespace {
                qs.push(format!("namespace={ns}"));
            }
            if *archived {
                qs.push("archived=true".to_string());
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
            let v = client.get_project_json(&format!("/tasks{query}")).await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let empty = vec![];
                let items = v
                    .as_array()
                    .or_else(|| v.get("items").and_then(|v| v.as_array()))
                    .unwrap_or(&empty);
                print!("{}", output::format_task_list(items));
            }
        }

        TaskSubcommand::Get {
            id,
            rel_types,
            direction,
            max_depth,
        } => {
            let mut qs = vec![];
            if let Some(rt) = rel_types {
                qs.push(format!("rel_types={rt}"));
            }
            if let Some(d) = direction {
                qs.push(format!("direction={d}"));
            }
            if let Some(md) = max_depth {
                qs.push(format!("max_depth={md}"));
            }
            let query = if qs.is_empty() {
                String::new()
            } else {
                format!("?{}", qs.join("&"))
            };
            let v = client
                .get_project_json(&format!("/tasks/{id}{query}"))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                print!("{}", output::format_task(&v));
            }
        }

        TaskSubcommand::Update {
            id,
            title,
            description,
            acceptance_criteria,
            priority,
        } => {
            let mut body = serde_json::json!({});
            if let Some(t) = title {
                body["title"] = serde_json::Value::String(t.clone());
            }
            if let Some(d) = description {
                body["description"] = serde_json::Value::String(d.clone());
            }
            if let Some(ac) = acceptance_criteria {
                body["acceptance_criteria"] = serde_json::Value::String(ac.clone());
            }
            if let Some(p) = priority {
                body["priority"] = serde_json::Value::String(p.clone());
            }
            let v = client
                .patch_project_json(&format!("/tasks/{id}"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} updated.");
            }
        }

        TaskSubcommand::Claim { id, start } => {
            let alias = require_alias(client)?;
            let mut body = serde_json::json!({ "agent": alias });
            if let Some(s) = start {
                body["start"] = serde_json::Value::Bool(*s);
            }
            let v = client
                .post_project_json(&format!("/tasks/{id}/claim"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} claimed.");
            }
        }

        TaskSubcommand::Start { id } => {
            let alias = require_alias(client)?;
            let body = serde_json::json!({ "agent": alias });
            let v = client
                .post_project_json(&format!("/tasks/{id}/start"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} started.");
            }
        }

        TaskSubcommand::Complete { id, summary, links } => {
            let mut body = serde_json::json!({});
            if let Some(s) = summary {
                body["summary"] = serde_json::Value::String(s.clone());
            }
            if let Some(l) = links {
                let parsed: Vec<serde_json::Value> = l
                    .split(',')
                    .filter_map(|link| {
                        let parts: Vec<&str> = link.trim().splitn(3, ':').collect();
                        if parts.len() == 3 {
                            Some(serde_json::json!({
                                "to_kind": parts[0],
                                "to_id": parts[1],
                                "rel_type": parts[2],
                            }))
                        } else {
                            None
                        }
                    })
                    .collect();
                body["links"] = serde_json::Value::Array(parsed);
            }
            let v = client
                .post_project_json(&format!("/tasks/{id}/complete"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} completed.");
            }
        }

        TaskSubcommand::Fail { id, reason } => {
            let mut body = serde_json::json!({});
            if let Some(r) = reason {
                body["reason"] = serde_json::Value::String(r.clone());
            }
            let v = client
                .post_project_json(&format!("/tasks/{id}/fail"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} failed.");
            }
        }

        TaskSubcommand::Cancel { id, reason } => {
            let mut body = serde_json::json!({});
            if let Some(r) = reason {
                body["reason"] = serde_json::Value::String(r.clone());
            }
            let v = client
                .post_project_json(&format!("/tasks/{id}/cancel"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} cancelled.");
            }
        }

        TaskSubcommand::Archive { task_id, reason } => {
            let mut body = serde_json::json!({});
            if let Some(r) = reason {
                body["reason"] = serde_json::Value::String(r.clone());
            }
            let v = client
                .post_project_json(&format!("/tasks/{task_id}/archive"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {task_id} archived.");
            }
        }

        TaskSubcommand::Unarchive { task_id } => {
            let v = client
                .post_project_json(
                    &format!("/tasks/{task_id}/unarchive"),
                    Some(&serde_json::json!({})),
                )
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {task_id} unarchived.");
            }
        }

        TaskSubcommand::Release { id } => {
            let v = client
                .post_project_json(
                    &format!("/tasks/{id}/release"),
                    Some(&serde_json::json!({})),
                )
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} released.");
            }
        }

        TaskSubcommand::Unblock { id } => {
            let v = client
                .post_project_json(
                    &format!("/tasks/{id}/unblock"),
                    Some(&serde_json::json!({})),
                )
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} unblocked.");
            }
        }

        TaskSubcommand::Assign { id, agent } => {
            let body = serde_json::json!({ "agent": agent });
            let v = client
                .post_project_json(&format!("/tasks/{id}/assign"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} assigned to {agent}.");
            }
        }

        TaskSubcommand::Next { claim, role } => {
            let mut qs = vec![];
            if let Some(c) = claim {
                qs.push(format!("claim={c}"));
            }
            if let Some(aid) = &client.alias {
                qs.push(format!("agent_id={aid}"));
            }
            if let Some(r) = role {
                qs.push(format!("role={r}"));
            }
            let query = if qs.is_empty() {
                String::new()
            } else {
                format!("?{}", qs.join("&"))
            };
            let v = client
                .get_project_json(&format!("/tasks/next{query}"))
                .await?;
            if v.is_null() {
                if config.json {
                    println!("null");
                } else {
                    println!("No matching tasks available.");
                }
            } else if config.json {
                output::print_json(config, &v);
            } else {
                print!("{}", output::format_task(&v));
            }
        }

        TaskSubcommand::Split { id, titles } => {
            let subtasks: Vec<serde_json::Value> = titles
                .iter()
                .map(|t| serde_json::json!({ "title": t, "description": "" }))
                .collect();
            let body = serde_json::json!({ "subtasks": subtasks });
            let v = client
                .post_project_json(&format!("/tasks/{id}/split"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} split into {} subtasks.", titles.len());
            }
        }

        TaskSubcommand::Merge {
            ids,
            title,
            description,
        } => {
            let body = serde_json::json!({
                "task_ids": ids.split(',').map(|s| s.trim()).collect::<Vec<_>>(),
                "title": title,
                "description": description,
            });
            let v = client
                .post_project_json("/tasks/merge", Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Tasks merged.");
            }
        }

        TaskSubcommand::Replace { id, reason, titles } => {
            let replacements: Vec<serde_json::Value> = titles
                .iter()
                .map(|t| serde_json::json!({ "title": t, "description": "" }))
                .collect();
            let mut body = serde_json::json!({ "replacements": replacements });
            if let Some(r) = reason {
                body["reason"] = serde_json::Value::String(r.clone());
            }
            let v = client
                .post_project_json(&format!("/tasks/{id}/replace"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} replaced.");
            }
        }

        TaskSubcommand::Delegate {
            id,
            title,
            description: desc,
            priority,
            roles,
        } => {
            let mut body = serde_json::json!({ "title": title, "description": desc });
            if let Some(p) = priority {
                body["priority"] = serde_json::Value::String(p.clone());
            }
            if let Some(r) = roles {
                body["assigned_roles"] = serde_json::Value::Array(
                    r.split(',')
                        .map(|s| serde_json::Value::String(s.trim().to_string()))
                        .collect(),
                );
            }
            let v = client
                .post_project_json(&format!("/tasks/{id}/delegate"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task delegated from {id}.");
            }
        }

        TaskSubcommand::Move { id, new_namespace } => {
            let body = serde_json::json!({ "new_namespace": new_namespace });
            let v = client
                .post_project_json(&format!("/tasks/{id}/move"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} moved to {new_namespace}.");
            }
        }

        TaskSubcommand::Tag { id, tag } => {
            let v = client
                .post_project_json(
                    &format!("/tasks/{id}/tags/{tag}"),
                    Some(&serde_json::json!({})),
                )
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Tag '{tag}' added to task {id}.");
            }
        }

        TaskSubcommand::Untag { id, tag } => {
            client
                .delete_project(&format!("/tasks/{id}/tags/{tag}"))
                .await?;
            if config.json {
                println!("{{\"ok\": true}}");
            } else {
                println!("Tag '{tag}' removed from task {id}.");
            }
        }

        TaskSubcommand::Touch { id } => {
            let v = client
                .post_project_json(&format!("/tasks/{id}/touch"), Some(&serde_json::json!({})))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Task {id} touched.");
            }
        }

        TaskSubcommand::ListTags => {
            let v = client.get_project_json("/tasks/tags").await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                let empty = vec![];
                let tags = v.as_array().unwrap_or(&empty);
                for tag in tags {
                    if let Some(s) = tag.as_str() {
                        println!("{s}");
                    }
                }
            }
        }

        TaskSubcommand::AddDep { id, dep } => {
            let body = serde_json::json!({ "dependency_id": dep });
            let v = client
                .post_project_json(&format!("/tasks/{id}/dependencies"), Some(&body))
                .await?;
            if config.json {
                output::print_json(config, &v);
            } else {
                println!("Dependency {dep} added to task {id}.");
            }
        }

        TaskSubcommand::RemoveDep { id, dep } => {
            client
                .delete_project(&format!("/tasks/{id}/dependencies/{dep}"))
                .await?;
            if config.json {
                println!("{{\"ok\": true}}");
            } else {
                println!("Dependency {dep} removed from task {id}.");
            }
        }
    }
    Ok(())
}
