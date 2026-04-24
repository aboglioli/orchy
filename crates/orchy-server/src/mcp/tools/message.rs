use std::str::FromStr;

use orchy_application::SendMessageCommand;
use orchy_core::message::MessageId;
use orchy_core::resource_ref::ResourceRef;

use crate::mcp::handler::{NamespacePolicy, OrchyHandler, mcp_error, to_json};
use crate::mcp::params::{ClaimMessageParams, RefParam, SendMessageParams, UnclaimMessageParams};

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
    let (agent_id, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::RegisterIfNew)
        .await?;

    let to = if let Some(alias_str) = params.to.strip_prefix('@') {
        let alias =
            orchy_core::agent::Alias::new(alias_str).map_err(|e| format!("invalid alias: {e}"))?;
        let (_, project, _) = h.require_session().await?;
        let org = h.org();
        let agent = h
            .container
            .agent_store
            .find_by_alias(&org, &project, &alias)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("agent alias @{alias_str} not found"))?;
        agent.id().to_string()
    } else {
        match orchy_core::message::MessageTarget::parse(&params.to) {
            Ok(_) => params.to.clone(),
            Err(_) => match h.resolve_agent_id(&params.to).await {
                Ok(id) => id.to_string(),
                Err(_) => {
                    return Err(format!(
                        "invalid target: '{}' (not a UUID, @alias, role:name, ns:/path, broadcast)",
                        params.to
                    ));
                }
            },
        }
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

pub(super) async fn claim_message(
    h: &OrchyHandler,
    params: ClaimMessageParams,
) -> Result<String, String> {
    let (agent_id, _, _) = h.require_session().await?;
    let message_id =
        MessageId::from_str(&params.message_id).map_err(|e| format!("invalid message_id: {e}"))?;

    h.container
        .app
        .claim_message
        .execute(agent_id, message_id)
        .await
        .map_err(mcp_error)?;
    Ok(to_json(&serde_json::json!({"ok": true})))
}

pub(super) async fn unclaim_message(
    h: &OrchyHandler,
    params: UnclaimMessageParams,
) -> Result<String, String> {
    let (agent_id, _, _) = h.require_session().await?;
    let message_id =
        MessageId::from_str(&params.message_id).map_err(|e| format!("invalid message_id: {e}"))?;

    h.container
        .app
        .unclaim_message
        .execute(agent_id, message_id)
        .await
        .map_err(mcp_error)?;
    Ok(to_json(&serde_json::json!({"ok": true})))
}
