use orchy_core::agent::Agent;
use orchy_core::agent::AgentStore;
use orchy_core::agent::service::AgentService;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::project::Project;
use orchy_core::project::ProjectStore;
use orchy_core::project::service::ProjectService;
use orchy_core::skill::Skill;
use orchy_core::skill::SkillStore;
use orchy_core::skill::service::SkillService;
use orchy_core::task::{Task, TaskFilter, TaskStatus, TaskStore};
use orchy_core::task::service::TaskService;

pub async fn generate_bootstrap_prompt<SS: SkillStore, PS: ProjectStore, AS: AgentStore, TS: TaskStore>(
    project_id: &ProjectId,
    namespace: &Namespace,
    host: &str,
    port: u16,
    skill_service: &SkillService<SS>,
    project_service: &ProjectService<PS>,
    agent_service: &AgentService<AS>,
    task_service: &TaskService<TS, AS>,
) -> Result<String, String> {
    let skills = skill_service
        .list_with_inherited(project_id, namespace)
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
        for status in &[TaskStatus::Pending, TaskStatus::Claimed, TaskStatus::InProgress, TaskStatus::Blocked] {
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

    Ok(render(namespace, host, port, &project, &skills, &agents, &active_tasks))
}

fn render(
    namespace: &Namespace,
    host: &str,
    port: u16,
    project: &Project,
    skills: &[Skill],
    agents: &[Agent],
    tasks: &[Task],
) -> String {
    let mut out = format!(
        r#"# Multi-Agent Coordination — Project `{namespace}`

You are part of a coordinated multi-agent system managed by **orchy**.
orchy is NOT an orchestrator — it's shared infrastructure: a task board,
shared memory, messaging bus, and skill registry exposed as MCP tools.
You bring the intelligence; orchy enforces the rules.

## Connection

MCP server: `http://{host}:{port}/mcp`
Project namespace: `{namespace}`

## Bootstrap Protocol

On every session start, execute these steps in order:

1. **Register** — call `register_agent` with:
   - `project`: your project identifier
   - `roles`: your capabilities (e.g. `["coder", "reviewer"]`). Leave empty to let orchy assign roles based on pending task demand.
   - `description`: what you are (e.g. "Claude Code backend agent")
   - `namespace`: scope within the project (optional, e.g. "backend")

2. **Load context** — call `get_project` for project description and notes.
   Call `list_skills(inherited: true)` for project conventions.

3. **Check for work** — call `get_next_task` to claim pending tasks,
   or `check_mailbox` for messages from other agents.

4. **Heartbeat** — call `heartbeat` periodically (every ~30s) to signal liveness.

## Namespace Rules

Resources are organized in namespaces within the project: `/` is root,
`/backend` and `/backend/auth` are scopes. Namespace is optional for
reading — omit it to see all project resources. Write operations default
to your current namespace. Use `move_agent` to switch namespaces.
Use `list_namespaces` to discover available scopes.

## Available Tools

| Category | Tools |
|----------|-------|
| Agent    | `register_agent`, `list_agents`, `change_roles`, `move_agent`, `heartbeat`, `disconnect` |
| Tasks    | `post_task`, `get_next_task`, `list_tasks`, `claim_task`, `start_task`, `complete_task`, `fail_task`, `assign_task`, `add_task_note` |
| Task Hierarchy | `split_task`, `replace_task`, `add_dependency`, `remove_dependency` |
| Move     | `move_task`, `move_memory`, `move_skill` |
| Memory   | `write_memory`, `read_memory`, `list_memory`, `search_memory`, `delete_memory` |
| Messages | `send_message`, `check_mailbox`, `mark_read`, `check_sent_messages`, `list_conversation` |
| Context  | `save_context`, `load_context`, `list_contexts`, `search_contexts` |
| Skills   | `write_skill`, `read_skill`, `list_skills`, `delete_skill` |
| Project  | `get_project`, `update_project`, `add_project_note` |
| Discovery| `list_namespaces`, `get_bootstrap_prompt` |

## Coordination Patterns

- **Claim before working** — always claim a task before starting. If another
  agent claimed it first, move on to the next one.
- **Split large tasks** — use `split_task` to break a task into subtasks.
  The parent blocks automatically and completes when all subtasks finish.
  Work on subtasks directly, not the parent.
- **Report results** — call `complete_task` with a summary when done.
- **Share knowledge** — use `write_memory` to store decisions, discoveries,
  or context that other agents need.
- **Message teammates** — use `send_message` to coordinate directly with
  other agents or broadcast to all.
- **Save context** — before your session ends, call `save_context` so the
  next agent picking up your work has continuity.
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

        for skill in skills {
            out.push_str(&format!(
                "### {} ({})\n\n{}\n\n",
                skill.name(),
                skill.namespace(),
                skill.content()
            ));
        }
    }

    out
}
