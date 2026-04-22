use orchy_application::{
    AgentResponse, Application, GetProjectCommand, KnowledgeResponse, ListAgentsCommand,
    ListOverviewsCommand, ListSkillsCommand, ListTasksCommand, ProjectResponse, TaskResponse,
};
use orchy_core::namespace::{Namespace, ProjectId};

pub async fn generate_bootstrap_prompt(
    project_id: &ProjectId,
    namespace: &Namespace,
    host: &str,
    port: u16,
    app: &Application,
) -> Result<String, String> {
    let default_org = "default".to_string();

    let skills = app
        .list_skills
        .execute(ListSkillsCommand {
            org_id: default_org.clone(),
            project: project_id.to_string(),
            namespace: Some(namespace.to_string()),
        })
        .await
        .map_err(|e| e.to_string())?;

    let overviews = app
        .list_overviews
        .execute(ListOverviewsCommand {
            org_id: default_org.clone(),
            project: project_id.to_string(),
            namespace: Some(namespace.to_string()),
        })
        .await
        .map_err(|e| e.to_string())?;

    let project = app
        .get_project
        .execute(GetProjectCommand {
            org_id: default_org.clone(),
            project: project_id.to_string(),
        })
        .await
        .map_err(|e| e.to_string())?;

    let agents: Vec<AgentResponse> = app
        .list_agents
        .execute(ListAgentsCommand {
            org_id: default_org.clone(),
            project: Some(project_id.to_string()),
            after: None,
            limit: None,
        })
        .await
        .map_err(|e| e.to_string())?
        .items;

    let active_tasks: Vec<TaskResponse> = {
        let mut all = Vec::new();
        for status in &["pending", "claimed", "in_progress", "blocked"] {
            let cmd = ListTasksCommand {
                org_id: default_org.clone(),
                project: Some(project_id.to_string()),
                namespace: None,
                status: Some(status.to_string()),
                assigned_to: None,
                tag: None,
                after: None,
                limit: None,
            };
            if let Ok(page) = app.list_tasks.execute(cmd).await {
                all.extend(page.items);
            }
        }
        all
    };

    Ok(render(
        namespace,
        host,
        port,
        &project,
        &overviews,
        &skills,
        &agents,
        &active_tasks,
    ))
}

#[allow(clippy::too_many_arguments)]
fn render(
    namespace: &Namespace,
    host: &str,
    port: u16,
    project: &ProjectResponse,
    overviews: &[KnowledgeResponse],
    skills: &[KnowledgeResponse],
    agents: &[AgentResponse],
    tasks: &[TaskResponse],
) -> String {
    let mut out = format!(
        r#"# Multi-Agent Coordination — Project `{namespace}`

You are part of a coordinated multi-agent system managed by **orchy**.
orchy provides shared infrastructure: a task board, knowledge base,
messaging, resource locks, and cross-project links
— all exposed as MCP tools. You bring the intelligence; orchy enforces the rules.

## Connection

MCP server: `http://{host}:{port}/mcp`
Project namespace: `{namespace}`

If orchy or the MCP client restarts, you get a new MCP session. Call `register_agent` again with
the **same** `id` from your last registration or handoff (`session_status` summarizes this).

## On Session Start

1. **Register** — `register_agent(project, description)`. Pass `id` to resume
   a previous agent.
2. **Load context** — `get_agent_context` returns project metadata, inbox,
   pending tasks, skills, and handoff context in one call.
3. **Claim work** — `get_next_task` (`claim: false` to peek only).
4. **Heartbeat** — `heartbeat` every ~30s to stay alive.

## Session Handoff

Always `write_knowledge(kind: "context", path: "handoff")` with:
- What task you were working on (task ID and title)
- What you accomplished
- What's left to do
- Key decisions made and reasoning
- Any blockers or open questions

This is the handoff note for the next agent (or your next session).

## Namespaces

Resources live in namespaces: `/` (root), `/backend`, `/backend/auth`.
Tools default to your current namespace when you omit the namespace parameter. Pass `namespace=/` to see all namespaces.
Writes default to your current namespace. Namespaces are auto-created on first use.

## Task Workflow

`pending → claimed → in_progress → completed/failed`

- Always **claim** before starting. If another agent claimed it, move on.
- Call **start_task** after claiming, then **complete_task** with a summary.
- **split_task** breaks a task into subtasks — parent auto-completes when all finish.
- **merge_tasks** consolidates related tasks into one.
- **delegate_task** creates subtasks without blocking the parent.
- **tag_task** / **untag_task** / **list_tags** — label tasks for cross-cutting organization.
- **release_task** — return a claimed task to pending.
- Stale claimed tasks can be reclaimed by other agents after a timeout.

## Coordination

- **write_knowledge** — persist decisions, discoveries, patterns, configs, plans.
  Call `list_knowledge_types` to see available types.
- **send_message** — coordinate by agent ID, `role:name`, or `broadcast`.
- **lock_resource** / **unlock_resource** — prevent conflicts on shared resources.
- **poll_updates** + **check_mailbox** — poll on each heartbeat cycle for reactivity.
- **write_knowledge(kind: "context")** — save session state before ending.
- **get_project** (`include_summary`) — check project status and your workload.

## Knowledge Capture

You must externalize knowledge so future agents can benefit:

- After completing a task, `write_knowledge` for each key decision
  (e.g. path: `auth-algorithm`, kind: `decision`).
- `complete_task` summary must be actionable: what was done, what was learned,
  what the next agent should know. Never just "done".
- Before ending your session, `write_knowledge(kind: "context", path: "handoff")`
  with structured summary: current task, progress, blockers, decisions.
- Use `kind: "summary"` for concise task/feature summaries, `kind: "report"` for
  detailed analysis or post-mortems.
- When you discover something non-obvious, write it to knowledge immediately.
- **Always `search_knowledge` before writing** to avoid duplicating existing
  entries. If an entry exists, update it instead of creating a new one.
- Use `list_knowledge_types` to see available kinds.

## Graph (Relationships Between Resources)

Use edges to record meaningful relationships between tasks, knowledge entries, agents, and messages.

**When to create edges:**
- Task produces a knowledge artifact → `add_edge(from_kind=task, to_kind=knowledge, rel_type=produces)`
- Knowledge governs a task → `add_edge(from_kind=knowledge, to_kind=task, rel_type=implements)`
- Task split/delegated into subtasks → `add_edge(from=parent, to=child, rel_type=spawns)`
- Knowledge supersedes an older entry → `add_edge(from=new, to=old, rel_type=supersedes)`

**When to traverse:**
- Before starting a task: query edges with `list_knowledge(orphaned=false)` and `add_edge/from_kind=task` lookups — see linked decisions and prior work.
- Mapping a task tree: use `add_edge` relations and recursive subtask lookups — full dependency + knowledge graph.
- Finding latest decision: query edges with `rel_type=supersedes` to trace supersession chains.

**Relationship types:** `derived_from`, `produces`, `supersedes`, `merged_from`, `summarizes`, `implements`, `spawns`, `related_to`, `depends_on`
"#
    );

    if !project.description.is_empty() {
        out.push_str("\n## Project Description\n\n");
        out.push_str(&project.description);
        out.push_str("\n\n");
    }

    if !overviews.is_empty() {
        out.push_str("## Project overview (knowledge)\n\n");
        out.push_str(
            "Entries with kind `overview` (namespace inheritance applies). \
             Use `write_knowledge(kind: \"overview\", ...)` for durable summaries.\n\n",
        );
        for entry in overviews {
            out.push_str(&format!(
                "### {} (`{}`)\n\n{}\n\n",
                entry.title, entry.namespace, entry.content
            ));
        }
    }

    if !agents.is_empty() {
        out.push_str("## Connected Agents\n\n");
        out.push_str("| ID | Roles | Namespace | Status |\n|-----|-------|-----------|--------|\n");
        for agent in agents {
            out.push_str(&format!(
                "| `{}` | {} | `{}` | {} |\n",
                agent.id,
                agent.roles.join(", "),
                agent.namespace,
                agent.status,
            ));
        }
        out.push('\n');
    }

    if !tasks.is_empty() {
        out.push_str("## Active Tasks\n\n");
        out.push_str("| ID | Title | Status | Roles | Assigned To |\n|-----|-------|--------|-------|-------------|\n");
        for task in tasks {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                task.id,
                task.title,
                task.status,
                task.assigned_roles.join(", "),
                task.assigned_to
                    .as_deref()
                    .map(|a| format!("`{a}`"))
                    .unwrap_or_else(|| "-".to_string()),
            ));
        }
        out.push('\n');
    }

    if !skills.is_empty() {
        out.push_str("## Project Skills\n\n");
        out.push_str(
            "The following conventions and instructions apply to this project.\n\
             Follow them in all your work.\n\n",
        );

        for entry in skills {
            out.push_str(&format!(
                "### {} ({})\n\n{}\n\n",
                entry.title, entry.namespace, entry.content
            ));
        }
    }

    out
}
