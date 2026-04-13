use orchy_core::namespace::Namespace;
use orchy_core::skill::Skill;
use orchy_core::skill::SkillStore;
use orchy_core::skill::service::SkillService;

pub async fn generate_bootstrap_prompt<S: SkillStore>(
    namespace: &Namespace,
    host: &str,
    port: u16,
    skill_service: &SkillService<S>,
) -> Result<String, String> {
    let skills = skill_service
        .list_with_inherited(namespace)
        .await
        .map_err(|e| e.to_string())?;

    Ok(render(namespace, host, port, &skills))
}

fn render(namespace: &Namespace, host: &str, port: u16, skills: &[Skill]) -> String {
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
   - `namespace`: `"{namespace}"`
   - `roles`: your capabilities (e.g. `["coder", "reviewer"]`)
   - `description`: what you are (e.g. "Claude Code agent for backend")

2. **Load skills** — call `list_skills` with `inherited: true` to get project
   conventions. Read and follow them.

3. **Check for work** — call `get_next_task` to claim pending tasks assigned
   to your roles, or `check_mailbox` for messages from other agents.

4. **Heartbeat** — call `heartbeat` periodically (every ~30s) to signal liveness.
   If you stop sending heartbeats, orchy will mark you disconnected and release
   your claimed tasks.

## Namespace Rules

All data is scoped to the project namespace `{namespace}`. You can use
sub-scopes (e.g. `{namespace}/backend`) but the first segment must always
match. You cannot access other projects.

## Available Tools

| Category | Tools |
|----------|-------|
| Agent    | `register_agent`, `list_agents`, `heartbeat` |
| Tasks    | `post_task`, `get_next_task`, `list_tasks`, `claim_task`, `start_task`, `complete_task`, `fail_task` |
| Memory   | `write_memory`, `read_memory`, `list_memory`, `search_memory`, `delete_memory` |
| Messages | `send_message`, `check_mailbox`, `mark_read` |
| Context  | `save_context`, `load_context`, `list_contexts`, `search_contexts` |
| Skills   | `write_skill`, `read_skill`, `list_skills`, `delete_skill` |
| Bootstrap| `get_bootstrap_prompt` |

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

    if !skills.is_empty() {
        out.push_str("\n## Project Skills\n\n");
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
