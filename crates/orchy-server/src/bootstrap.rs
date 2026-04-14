use orchy_core::agent::Agent;
use orchy_core::agent::AgentStore;
use orchy_core::agent::service::AgentService;
use orchy_core::knowledge::Entry;
use orchy_core::knowledge::EntryStore;
use orchy_core::knowledge::service::KnowledgeService;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::project::Project;
use orchy_core::project::ProjectStore;
use orchy_core::project::service::ProjectService;
use orchy_core::task::service::TaskService;
use orchy_core::task::{Task, TaskFilter, TaskStatus, TaskStore};

pub async fn generate_bootstrap_prompt<
    KS: EntryStore,
    PS: ProjectStore,
    AS: AgentStore,
    TS: TaskStore,
>(
    project_id: &ProjectId,
    namespace: &Namespace,
    host: &str,
    port: u16,
    knowledge_service: &KnowledgeService<KS, crate::embeddings::EmbeddingsBackend>,
    project_service: &ProjectService<PS>,
    agent_service: &AgentService<AS>,
    task_service: &TaskService<TS, AS>,
) -> Result<String, String> {
    let skills = knowledge_service
        .list_skills(project_id, namespace)
        .await
        .map_err(|e| e.to_string())?;

    let project = project_service
        .get_or_create(project_id)
        .await
        .map_err(|e| e.to_string())?;

    let agents: Vec<Agent> = agent_service
        .list()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|a| a.project() == project_id)
        .collect();

    let active_tasks: Vec<Task> = {
        let mut all = Vec::new();
        for status in &[
            TaskStatus::Pending,
            TaskStatus::Claimed,
            TaskStatus::InProgress,
            TaskStatus::Blocked,
        ] {
            let filter = TaskFilter {
                project: Some(project_id.clone()),
                status: Some(*status),
                ..Default::default()
            };
            if let Ok(tasks) = task_service.list(filter).await {
                all.extend(tasks);
            }
        }
        all
    };

    Ok(render(
        namespace,
        host,
        port,
        &project,
        &skills,
        &agents,
        &active_tasks,
    ))
}

fn render(
    namespace: &Namespace,
    host: &str,
    port: u16,
    project: &Project,
    skills: &[Entry],
    agents: &[Agent],
    tasks: &[Task],
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

## On Session Start

1. **Register** — `register_agent(project, description)`. Roles are optional;
   orchy assigns them based on pending task demand if omitted.
   Pass `agent_id` to resume a previous session.
2. **Load context** — `get_project` for description and notes,
   then `list_knowledge(entry_type: "skill")` for conventions. Follow them.
3. **Resume** — `load_context` to check for handoff notes from a previous
   session. Also `search_knowledge(query)` to find relevant context from
   other agents. Check `check_mailbox` for messages.
4. **Claim work** — `get_next_task` to claim a task. Tasks from disconnected
   agents return to pending automatically.
5. **Heartbeat** — `heartbeat` every ~30s to stay alive.

## Before Disconnecting

Always call `save_context` with a structured summary:
- What task you were working on (task ID and title)
- What you accomplished
- What's left to do
- Key decisions made and reasoning
- Any blockers or open questions

This is the handoff note for the next agent (or your next session).

## Namespaces

Resources live in namespaces: `/` (root), `/backend`, `/backend/auth`.
Omit namespace on reads to see everything. Writes default to your current
namespace. Namespaces are auto-created on first use.

## Task Workflow

`pending → claimed → in_progress → completed/failed`

- Always **claim** before starting. If another agent claimed it, move on.
- Call **start_task** after claiming, then **complete_task** with a summary.
- **split_task** breaks a task into subtasks — parent auto-completes when all finish.
- **merge_tasks** consolidates related tasks into one.
- **delegate_task** creates subtasks without blocking the parent.
- **tag_task** / **list_tags** — label tasks for cross-cutting organization.
- **release_task** — return a claimed task to pending.
- On disconnect, your claimed tasks return to pending automatically.

## Coordination

- **write_knowledge** — persist decisions, discoveries, patterns, configs, plans.
  Call `list_knowledge_types` to see available types.
- **send_message** — coordinate by agent ID, `role:name`, or `broadcast`.
- **watch_task** — get notified when a task you depend on changes status.
- **request_review** / **resolve_review** — approval workflows between agents.
- **lock_resource** / **unlock_resource** — prevent conflicts on shared resources.
- **poll_updates** + **check_mailbox** — poll on each heartbeat cycle for reactivity.
- **save_context** — save session state before ending for continuity.
- **link_project** — import knowledge from other projects.
- **get_project_summary** / **get_agent_workload** — check project status.

## Knowledge Capture

You must externalize knowledge so future agents can benefit:

- After completing a task, `write_knowledge` for each key decision
  (e.g. path: `decisions/auth-algorithm`, type: `decision`).
- `complete_task` summary must be actionable: what was done, what was learned,
  what the next agent should know. Never just "done".
- Before disconnecting, `save_context` with structured handoff: current task,
  progress, blockers, decisions.
- When you discover something non-obvious, write it to knowledge immediately.
- Use `search_knowledge` before starting work to check
  if a previous agent already explored this area.
"#
    );

    let description = project.description();
    if !description.is_empty() {
        out.push_str("\n## Project Description\n\n");
        out.push_str(description);
        out.push_str("\n\n");
    }

    let notes = project.notes();
    if !notes.is_empty() {
        out.push_str("## Project Notes\n\n");
        for note in notes {
            out.push_str(&format!("- {}\n", note.body()));
        }
        out.push('\n');
    }

    if !agents.is_empty() {
        out.push_str("## Connected Agents\n\n");
        out.push_str("| ID | Roles | Namespace | Status |\n|-----|-------|-----------|--------|\n");
        for agent in agents {
            out.push_str(&format!(
                "| `{}` | {} | `{}` | {} |\n",
                agent.id(),
                agent.roles().join(", "),
                agent.namespace(),
                agent.status(),
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
                task.id(),
                task.title(),
                task.status(),
                task.assigned_roles().join(", "),
                task.assigned_to()
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
                entry.title(),
                entry.namespace(),
                entry.content()
            ));
        }
    }

    out
}
