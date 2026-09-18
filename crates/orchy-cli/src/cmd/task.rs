use orchy_application::Application;
use orchy_application::block_task::BlockTaskCommand;
use orchy_application::cancel_task::CancelTaskCommand;
use orchy_application::claim_task::ClaimTaskCommand;
use orchy_application::complete_task::{CompleteTaskCommand, CompleteTaskResponse};
use orchy_application::create_task::CreateTaskCommand;
use orchy_application::dto::TaskDto;
use orchy_application::fail_task::FailTaskCommand;
use orchy_application::get_task::GetTaskCommand;
use orchy_application::list_tasks::ListTasksCommand;
use orchy_application::manage_dependencies::ManageDependenciesCommand;
use orchy_application::next_task::NextTaskCommand;
use orchy_application::release_task::ReleaseTaskCommand;
use orchy_application::split_task::SplitTaskCommand;
use orchy_application::start_task::StartTaskCommand;
use orchy_application::unblock_task::UnblockTaskCommand;
use orchy_application::update_task::UpdateTaskCommand;

use crate::cli::TaskCommand;
use crate::error::CliResult;
use crate::output::{Output, short};
use crate::resolve;

pub(crate) async fn run(
    app: &Application,
    actor: &str,
    command: TaskCommand,
    out: &Output,
) -> CliResult<()> {
    match command {
        TaskCommand::New {
            title,
            description,
            priority,
            namespace,
            role,
            tag,
            parent,
            depends_on,
        } => {
            let parent = match parent {
                Some(p) => Some(resolve::task(app, &p).await?),
                None => None,
            };
            let mut resolved = Vec::new();
            for dependency in &depends_on {
                resolved.push(resolve::task(app, dependency).await?);
            }
            let task = app
                .create_task
                .execute(CreateTaskCommand {
                    title,
                    description,
                    acceptance_criteria: None,
                    priority,
                    namespace,
                    roles: role,
                    tags: tag,
                    parent,
                    depends_on: resolved,
                })
                .await?;
            out.emit(&task, |t| format!("{}  {}", short(&t.id), t.title))
        }

        TaskCommand::List {
            status,
            namespace,
            mine,
            role,
            parent,
            tag,
            limit,
        } => {
            let page = app
                .list_tasks
                .execute(ListTasksCommand {
                    status,
                    namespace,
                    claimed_by: mine.then(|| actor.to_owned()),
                    role,
                    parent: match parent {
                        Some(p) => Some(resolve::task(app, &p).await?),
                        None => None,
                    },
                    tags: tag,
                    text: None,
                    offset: None,
                    limit,
                })
                .await?;
            out.emit(&page, |p| render_list(&p.items, out))
        }

        TaskCommand::Get { target } => {
            let task_id = resolve::task(app, &target).await?;
            let response = app.get_task.execute(GetTaskCommand { task_id }).await?;
            out.emit(&response, |r| {
                let mut lines = vec![detail(&r.task, out)];
                if !r.subtasks.is_empty() {
                    lines.push(String::new());
                    lines.push(out.bold("subtasks"));
                    lines.push(render_list(&r.subtasks, out));
                }
                lines.join("\n")
            })
        }

        TaskCommand::Next {
            role,
            namespace,
            peek,
        } => {
            let found = app
                .next_task
                .execute(NextTaskCommand {
                    actor: actor.to_owned(),
                    role,
                    namespace,
                    claim: !peek,
                })
                .await?;
            out.emit(&found, |t| match t {
                Some(task) => detail(task, out),
                None => "nothing claimable".to_owned(),
            })
        }

        TaskCommand::Claim { target, ttl, start } => {
            let task_id = resolve::task(app, &target).await?;
            let task = app
                .claim_task
                .execute(ClaimTaskCommand {
                    task_id,
                    actor: actor.to_owned(),
                    ttl_seconds: ttl,
                    start,
                })
                .await?;
            out.emit(&task, |t| detail(t, out))
        }

        TaskCommand::Release { target } => {
            let task_id = resolve::task(app, &target).await?;
            let task = app
                .release_task
                .execute(ReleaseTaskCommand {
                    task_id,
                    actor: actor.to_owned(),
                })
                .await?;
            out.emit(&task, |t| detail(t, out))
        }

        TaskCommand::Start { target } => {
            let task_id = resolve::task(app, &target).await?;
            let task = app.start_task.execute(StartTaskCommand { task_id }).await?;
            out.emit(&task, |t| detail(t, out))
        }

        TaskCommand::Done { target, note } => {
            let task_id = resolve::task(app, &target).await?;
            let response = app
                .complete_task
                .execute(CompleteTaskCommand {
                    task_id,
                    note,
                    actor: actor.to_owned(),
                })
                .await?;
            emit_finished(&response, out)
        }

        TaskCommand::Fail { target, reason } => {
            let task_id = resolve::task(app, &target).await?;
            let response = app
                .fail_task
                .execute(FailTaskCommand {
                    task_id,
                    reason,
                    actor: actor.to_owned(),
                })
                .await?;
            emit_finished(&response, out)
        }

        TaskCommand::Cancel { target, reason } => {
            let task_id = resolve::task(app, &target).await?;
            let response = app
                .cancel_task
                .execute(CancelTaskCommand {
                    task_id,
                    reason,
                    actor: actor.to_owned(),
                })
                .await?;
            emit_finished(&response, out)
        }

        TaskCommand::Block { target, reason } => {
            let task_id = resolve::task(app, &target).await?;
            let task = app
                .block_task
                .execute(BlockTaskCommand { task_id, reason })
                .await?;
            out.emit(&task, |t| detail(t, out))
        }

        TaskCommand::Unblock { target } => {
            let task_id = resolve::task(app, &target).await?;
            let task = app
                .unblock_task
                .execute(UnblockTaskCommand { task_id })
                .await?;
            out.emit(&task, |t| detail(t, out))
        }

        TaskCommand::Split { target, titles } => {
            let task_id = resolve::task(app, &target).await?;
            let response = app
                .split_task
                .execute(SplitTaskCommand { task_id, titles })
                .await?;
            for skipped in &response.skipped {
                out.note(format!("already a subtask, left alone: {skipped}"));
            }
            out.emit(&response, |r| render_list(&r.created, out))
        }

        TaskCommand::Dep {
            target,
            add,
            remove,
        } => {
            let task_id = resolve::task(app, &target).await?;
            let mut added = Vec::new();
            for dependency in &add {
                added.push(resolve::task(app, dependency).await?);
            }
            let mut removed = Vec::new();
            for dependency in &remove {
                removed.push(resolve::task(app, dependency).await?);
            }
            let task = app
                .manage_dependencies
                .execute(ManageDependenciesCommand {
                    task_id,
                    add: added,
                    remove: removed,
                })
                .await?;
            out.emit(&task, |t| detail(t, out))
        }

        TaskCommand::Update {
            target,
            title,
            description,
            priority,
            namespace,
            tag,
            untag,
        } => {
            let task_id = resolve::task(app, &target).await?;
            let task = app
                .update_task
                .execute(UpdateTaskCommand {
                    task_id,
                    title,
                    description,
                    acceptance_criteria: None,
                    priority,
                    roles: None,
                    namespace,
                    add_tags: tag,
                    remove_tags: untag,
                })
                .await?;
            out.emit(&task, |t| detail(t, out))
        }
    }
}

fn emit_finished(response: &CompleteTaskResponse, out: &Output) -> CliResult<()> {
    for parent in &response.ancestors {
        out.note(format!("↑ {} → {}", short(&parent.id), parent.status));
    }
    out.emit(response, |r| detail(&r.task, out))
}

fn render_list(tasks: &[TaskDto], out: &Output) -> String {
    if tasks.is_empty() {
        return "no tasks".to_owned();
    }
    tasks
        .iter()
        .map(|t| {
            format!(
                "{}  {:<12} {:<7} {}",
                out.dim(short(&t.id)),
                t.status,
                t.priority,
                t.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn detail(task: &TaskDto, out: &Output) -> String {
    let mut lines = vec![
        format!("{}  {}", out.bold(&task.id), task.title),
        format!("  status     {}", task.status),
        format!("  priority   {}", task.priority),
        format!("  namespace  {}", task.namespace),
    ];
    if let Some(parent) = &task.parent {
        lines.push(format!("  parent     {}", short(parent)));
    }
    if let Some(holder) = &task.claimed_by {
        lines.push(format!("  claimed by {holder}"));
    }
    if !task.depends_on.is_empty() {
        let deps: Vec<&str> = task.depends_on.iter().map(|d| short(d)).collect();
        lines.push(format!("  depends on {}", deps.join(", ")));
    }
    if !task.tags.is_empty() {
        lines.push(format!("  tags       {}", task.tags.join(", ")));
    }
    if !task.description.is_empty() {
        lines.push(String::new());
        lines.push(task.description.clone());
    }
    lines.join("\n")
}
