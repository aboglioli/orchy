use crate::client::OrchyClient;
use crate::config::Config;
use crate::output;

pub async fn run(
    client: &OrchyClient,
    config: &Config,
    verbose: bool,
) -> crate::client::CliResult<()> {
    let agent_id = client
        .agent_id
        .clone()
        .ok_or(crate::client::CliError::MissingAgentId)?;
    let path = format!("/agents/{agent_id}/context");
    let (agent_v, project_v) =
        tokio::try_join!(client.get_json(&path), client.get_project_json(""),)?;

    if config.json {
        output::print_json(
            config,
            &serde_json::json!({
                "agent": agent_v,
                "project": project_v,
            }),
        );
    } else {
        print!(
            "{}",
            output::format_bootstrap(&agent_v, &project_v, &config.org, &config.project, verbose,)
        );
    }
    Ok(())
}
