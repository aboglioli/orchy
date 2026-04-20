use crate::config::Config;

/// Print raw JSON (always used when --json is set, regardless of text).
pub fn print_json(config: &Config, v: &serde_json::Value) {
    if config.json {
        println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
    }
}

/// Format a task as readable text.
pub fn format_task(v: &serde_json::Value) -> String {
    let id = v.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let status = v.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    let priority = v.get("priority").and_then(|v| v.as_str()).unwrap_or("?");
    let title = v.get("title").and_then(|v| v.as_str()).unwrap_or("?");
    let assigned = v.get("assigned_to").and_then(|v| v.as_str());
    let assigned_suffix = match assigned {
        Some(a) => format!("  ← {} ", a),
        None => String::new(),
    };
    format!("{id}  {status:>12}  {priority:>8}  {title}{assigned_suffix}\n")
}

/// Format a list of tasks as a readable table.
pub fn format_task_list(items: &[serde_json::Value]) -> String {
    if items.is_empty() {
        return "No tasks found.\n".to_string();
    }
    let mut out = String::new();
    for t in items {
        out.push_str(&format_task(t));
    }
    out
}

/// Format a knowledge entry as readable text.
pub fn format_knowledge(v: &serde_json::Value) -> String {
    let id = v.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let kind = v.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
    let path = v.get("path").and_then(|v| v.as_str()).unwrap_or("?");
    let title = v.get("title").and_then(|v| v.as_str()).unwrap_or("?");
    let version = v.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    format!("{id}  {kind:>12}  {path}  v{version}  {title}\n")
}

/// Format a list of knowledge entries.
pub fn format_knowledge_list(items: &[serde_json::Value]) -> String {
    if items.is_empty() {
        return "No knowledge entries found.\n".to_string();
    }
    let mut out = String::new();
    for k in items {
        out.push_str(&format_knowledge(k));
    }
    out
}

/// Format a message as readable text.
pub fn format_message(v: &serde_json::Value) -> String {
    let id = v.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let from = v.get("from").and_then(|v| v.as_str()).unwrap_or("?");
    let to = v.get("to").and_then(|v| v.as_str()).unwrap_or("?");
    let body = v.get("body").and_then(|v| v.as_str()).unwrap_or("?");
    let truncated = if body.len() > 80 { &body[..80] } else { body };
    let suffix = if body.len() > 80 { "…" } else { "" };
    format!("[{id}] {from} → {to}: \"{truncated}{suffix}\"\n")
}

/// Format a list of messages.
pub fn format_message_list(items: &[serde_json::Value]) -> String {
    if items.is_empty() {
        return "No messages.\n".to_string();
    }
    let mut out = String::new();
    for m in items {
        out.push_str(&format_message(m));
    }
    out
}

/// Format an edge as readable text.
pub fn format_edge(v: &serde_json::Value) -> String {
    let id = v.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let from_kind = v.get("from_kind").and_then(|v| v.as_str()).unwrap_or("?");
    let from_id = v.get("from_id").and_then(|v| v.as_str()).unwrap_or("?");
    let to_kind = v.get("to_kind").and_then(|v| v.as_str()).unwrap_or("?");
    let to_id = v.get("to_id").and_then(|v| v.as_str()).unwrap_or("?");
    let rel = v.get("rel_type").and_then(|v| v.as_str()).unwrap_or("?");
    format!("{id}  {from_kind}:{from_id} --{rel}--> {to_kind}:{to_id}\n")
}

/// Format an agent as readable text.
pub fn format_agent(v: &serde_json::Value) -> String {
    let id = v.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let desc = v.get("description").and_then(|v| v.as_str()).unwrap_or("?");
    let status = v.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    let roles = v
        .get("roles")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    format!("{id}  {status:>12}  [{roles}]  {desc}\n")
}

/// Format the bootstrap output.
pub fn format_bootstrap(
    agent_v: &serde_json::Value,
    project_v: &serde_json::Value,
    org: &str,
    project: &str,
    verbose: bool,
) -> String {
    let mut out = String::new();
    out.push_str("=== ORCHY AGENT BRIEFING ===\n\n");

    // Identity — agent info may be nested under "agent" key (context endpoint)
    let agent_info = agent_v.get("agent").unwrap_or(agent_v);
    let agent_id = agent_info.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let desc = agent_info
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let ns = agent_info
        .get("namespace")
        .and_then(|v| v.as_str())
        .unwrap_or("/");
    let proj_desc = project_v
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    out.push_str(&format!(
        "Identity\n  Agent:     {agent_id} ({desc})\n  Org:       {org}\n  Project:   {project}"
    ));
    if !proj_desc.is_empty() {
        out.push_str(&format!(" — {proj_desc}"));
    }
    out.push_str(&format!("\n  Namespace: {ns}\n\n"));

    // Inbox
    if let Some(inbox) = agent_v.get("inbox").and_then(|v| v.as_array()) {
        out.push_str(&format!("Inbox  ({} unread)\n", inbox.len()));
        if inbox.is_empty() {
            out.push_str("  (empty)\n");
        } else {
            for m in inbox {
                let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                let from = m.get("from").and_then(|v| v.as_str()).unwrap_or("?");
                let body = m.get("body").and_then(|v| v.as_str()).unwrap_or("?");
                if verbose {
                    out.push_str(&format!("  [{id}] from {from}: \"{body}\"\n"));
                } else {
                    let truncated = if body.len() > 60 { &body[..60] } else { body };
                    out.push_str(&format!("  [{id}] from {from}: \"{truncated}\"\n"));
                }
            }
        }
        out.push('\n');
    }

    // Pending tasks
    if let Some(tasks) = agent_v.get("pending_tasks").and_then(|v| v.as_array()) {
        out.push_str(&format!("Tasks  ({} pending)\n", tasks.len()));
        if tasks.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for t in tasks {
                out.push_str(&format_task(t));
            }
        }
        out.push('\n');
    }

    // Skills
    if let Some(skills) = agent_v.get("skills").and_then(|v| v.as_array()) {
        out.push_str(&format!("Skills  ({} active)\n", skills.len()));
        for s in skills {
            let path = s.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let title = s.get("title").and_then(|v| v.as_str()).unwrap_or("");
            out.push_str(&format!("  {path}  {title}\n"));
            if let Some(content) = verbose
                .then(|| s.get("content").and_then(|v| v.as_str()))
                .flatten()
            {
                for line in content.lines() {
                    out.push_str(&format!("    {line}\n"));
                }
            }
        }
        out.push('\n');
    }

    // Quick start
    out.push_str("Quick start\n");
    out.push_str("  orchy task get <id> --json          get full task details\n");
    out.push_str("  orchy task claim <id>               claim a task\n");
    out.push_str("  orchy task complete <id> --summary  complete with summary\n");
    out.push_str("  orchy knowledge write <path>        write a knowledge entry\n");
    out.push_str("  orchy message send --to <agent>     send a message\n");
    out
}
