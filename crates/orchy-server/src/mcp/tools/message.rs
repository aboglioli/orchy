use orchy_application::SendMessageCommand;

use crate::mcp::handler::{NamespacePolicy, OrchyHandler, mcp_error, to_json};
use crate::mcp::params::SendMessageParams;

pub(super) async fn send_message(
    h: &OrchyHandler,
    params: SendMessageParams,
) -> Result<String, String> {
    let (agent_id, org, project, _) = h.require_session().await?;

    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::RegisterIfNew)
        .await?;

    let to = match orchy_core::message::MessageTarget::parse(&params.to) {
        Ok(_) => params.to.clone(),
        Err(_) => match h.resolve_agent_id(&params.to).await {
            Ok(id) => id.to_string(),
            Err(_) => {
                return Err(format!(
                    "invalid target: '{}' (not a UUID, role:name, broadcast, or known alias)",
                    params.to
                ));
            }
        },
    };

    let cmd = SendMessageCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        from_agent_id: agent_id.to_string(),
        to,
        body: params.body,
        reply_to: params.reply_to,
    };

    match h.container.app.send_message.execute(cmd).await {
        Ok(message) => Ok(to_json(&message)),
        Err(e) => Err(mcp_error(e)),
    }
}
