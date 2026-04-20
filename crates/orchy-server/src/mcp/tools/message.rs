use orchy_application::SendMessageCommand;
use orchy_core::resource_ref::ResourceRef;

use crate::mcp::handler::{NamespacePolicy, OrchyHandler, mcp_error, to_json};
use crate::mcp::params::{RefParam, SendMessageParams};

fn convert_ref_param(param: RefParam) -> Result<ResourceRef, String> {
    let rr = match param.kind.as_str() {
        "task" => ResourceRef::task(&param.id),
        "knowledge" => ResourceRef::knowledge(&param.id),
        "agent" => ResourceRef::agent(&param.id),
        "message" => ResourceRef::message(&param.id),
        k => return Err(format!("invalid ref kind: '{}'", k)),
    };

    Ok(match param.display {
        Some(display) => rr.with_display(&display),
        None => rr,
    })
}

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

    let refs: Vec<ResourceRef> = match params.refs {
        Some(params) => params
            .into_iter()
            .map(convert_ref_param)
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };

    let cmd = SendMessageCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        from_agent_id: agent_id.to_string(),
        to,
        body: params.body,
        reply_to: params.reply_to,
        refs,
    };

    match h.container.app.send_message.execute(cmd).await {
        Ok(message) => Ok(to_json(&message)),
        Err(e) => Err(mcp_error(e)),
    }
}
