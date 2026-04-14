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
use orchy_core::task::service::TaskService;
use orchy_core::task::{Task, TaskFilter, TaskStatus, TaskStore};

pub async fn generate_bootstrap_prompt<
    SS: SkillStore,
    PS: ProjectStore,
    AS: AgentStore,
    TS: TaskStore,
>(
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
    skills: &[Skill],
    agents: &[Agent],
    tasks: &[Task],
) -> String {
    let mut out = format!(
        r#"# Multi-Agent Coordination — Project `{namespace}`

You are part of a coordinated multi-agent system managed by **orchy**.
orchy provides shared infrastructure: a task board, shared memory,
messaging, and skill registry exposed as MCP tools.
You bring the intelligence; orchy enforces the rules.

## Connection

MCP server: `http://{host}:{port}/mcp`
Project namespace: `{namespace}`

## On Session Start

1. **Register** — `register_agent(project, description)`. Roles are optional;
   orchy assigns them based on pending task demand if omitted.
2. **Load context** — `get_project` for description and notes,
   then `list_skills(inherited: true)` for conventions. Follow them.
3. **Check for work** — `get_next_task` to claim a task,
   or `check_mailbox` for messages from other agents.
4. **Heartbeat** — `heartbeat` every ~30s to stay alive.

## Namespaces

Resources live in namespaces: `/` (root), `/backend`, `/backend/auth`.
Omit namespace on reads to see everything. Writes default to your current
namespace. Namespaces are auto-created on first use.

## Task Workflow

`pending → claimed → in_progress → completed/failed`

- Always **claim** before starting. If another agent claimed it, move on.
- Call **start_task** after claiming, then **complete_task** with a summary.
- **split_task** breaks a task into subtasks. The parent blocks and
  auto-completes when all subtasks finish. Work on subtasks directly.
- On disconnect, your claimed tasks return to pending automatically.

## Coordination

- **write_memory** — share decisions and context with other agents.
- **send_message** — coordinate by agent ID, `role:name`, or `broadcast`.
- **save_context** — save session state before ending for continuity.
- **add_dependency** / **remove_dependency** — manage task dependencies.
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
