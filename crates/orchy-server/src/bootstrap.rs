use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::project::Project;
use orchy_core::project::ProjectStore;
use orchy_core::project::service::ProjectService;
use orchy_core::skill::Skill;
use orchy_core::skill::SkillStore;
use orchy_core::skill::service::SkillService;

pub async fn generate_bootstrap_prompt<SS: SkillStore, PS: ProjectStore>(
    project_id: &ProjectId,
    namespace: &Namespace,
    host: &str,
    port: u16,
    skill_service: &SkillService<SS>,
    project_service: &ProjectService<PS>,
) -> Result<String, String> {
    let skills = skill_service
        .list_with_inherited(project_id, namespace)
        .await
        .map_err(|e| e.to_string())?;

    let project = project_service
        .get_or_create(project_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(render(namespace, host, port, &project, &skills))
}

fn render(
    namespace: &Namespace,
    host: &str,
    port: u16,
    project: &Project,
    skills: &[Skill],
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
   - `roles`: your capabilities (e.g. `["coder", "reviewer"]`)
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
| Move     | `move_task`, `move_memory`, `move_skill` |
| Memory   | `write_memory`, `read_memory`, `list_memory`, `search_memory`, `delete_memory` |
| Messages | `send_message`, `check_mailbox`, `mark_read` |
| Context  | `save_context`, `load_context`, `list_contexts`, `search_contexts` |
| Skills   | `write_skill`, `read_skill`, `list_skills`, `delete_skill` |
| Project  | `get_project`, `update_project`, `add_project_note` |
| Discovery| `list_namespaces`, `get_bootstrap_prompt` |

## Coordination Patterns

- **Claim before working** — always claim a task before starting. If another
  agent claimed it first, move on to the next one.
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
