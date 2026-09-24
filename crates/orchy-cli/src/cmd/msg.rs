use orchy_application::Application;
use orchy_application::dto::MessageDto;
use orchy_application::list_sent::ListSentCommand;
use orchy_application::promote_message::PromoteMessageCommand;
use orchy_application::read_inbox::ReadInboxCommand;
use orchy_application::read_message::ReadMessageCommand;
use orchy_application::read_thread::ReadThreadCommand;
use orchy_application::resolve_thread::ResolveThreadCommand;
use orchy_application::send_message::SendMessageCommand;

use crate::cli::MsgCommand;
use crate::error::CliResult;
use crate::output::{Output, short};
use crate::stdin;

pub(crate) async fn run(
    app: &Application,
    actor: &str,
    command: MsgCommand,
    out: &Output,
) -> CliResult<()> {
    match command {
        MsgCommand::Send {
            to,
            subject,
            body,
            reply_to,
            priority,
        } => {
            let message = app
                .send_message
                .execute(SendMessageCommand {
                    from: actor.to_owned(),
                    to,
                    subject,
                    body: stdin::or_read(body)?,
                    namespace: None,
                    priority,
                    reply_to,
                })
                .await?;
            out.emit(&message, |m| format!("{}  sent", short(&m.id)))
        }

        MsgCommand::Inbox { all } => {
            let messages = app
                .read_inbox
                .execute(ReadInboxCommand {
                    actor: actor.to_owned(),
                    all,
                })
                .await?;
            out.emit(&messages, |m| render_list(m, out))
        }

        MsgCommand::Read { target } => {
            let message = app
                .read_message
                .execute(ReadMessageCommand {
                    message_id: target,
                    actor: actor.to_owned(),
                })
                .await?;
            out.emit(&message, |m| detail(m, out))
        }

        MsgCommand::Thread { target } => {
            let thread = app
                .read_thread
                .execute(ReadThreadCommand { message_id: target })
                .await?;
            out.emit(&thread, |t| {
                t.iter()
                    .map(|m| detail(m, out))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            })
        }

        MsgCommand::Sent => {
            let messages = app
                .list_sent
                .execute(ListSentCommand {
                    actor: actor.to_owned(),
                })
                .await?;
            out.emit(&messages, |m| render_list(m, out))
        }

        MsgCommand::Resolve { target } => {
            let message = app
                .resolve_thread
                .execute(ResolveThreadCommand {
                    message_id: target,
                    actor: actor.to_owned(),
                })
                .await?;
            out.emit(&message, |m| format!("{}  resolved", short(&m.id)))
        }

        MsgCommand::Promote {
            target,
            title,
            role,
        } => {
            let response = app
                .promote_message
                .execute(PromoteMessageCommand {
                    message_id: target,
                    actor: actor.to_owned(),
                    title,
                    roles: role,
                })
                .await?;
            out.note("thread resolved");
            out.emit(&response, |r| {
                format!("{}  {}", short(&r.task.id), r.task.title)
            })
        }
    }
}

fn render_list(messages: &[MessageDto], out: &Output) -> String {
    if messages.is_empty() {
        return "nothing unread".to_owned();
    }
    messages
        .iter()
        .map(|m| {
            format!(
                "{}  {:<9} {:<28} {}",
                out.dim(short(&m.id)),
                m.status,
                m.from,
                m.subject.as_deref().unwrap_or("(no subject)")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn detail(message: &MessageDto, out: &Output) -> String {
    let mut lines = vec![
        out.bold(message.subject.as_deref().unwrap_or("(no subject)")),
        format!("  from    {}", message.from),
        format!("  to      {}", message.to.join(", ")),
        format!("  status  {}", message.status),
        String::new(),
        message.body.clone(),
    ];
    if let Some(parent) = &message.in_reply_to {
        lines.insert(3, format!("  reply to {}", short(parent)));
    }
    lines.join("\n")
}
