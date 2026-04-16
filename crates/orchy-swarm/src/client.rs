use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentDto {
    pub id: String,
    pub alias: Option<String>,
    pub description: String,
    pub status: String,
    pub agent_type: Option<String>,
    pub namespace: String,
    pub last_heartbeat: String,
}

pub struct OrchyClient {
    base_url: String,
    http: reqwest::Client,
}

impl OrchyClient {
    pub fn new(mcp_url: &str) -> Self {
        let base_url = base_url(mcp_url);
        Self {
            base_url,
            http: reqwest::Client::new(),
        }
    }

    pub async fn list_agents(&self, project: &str) -> anyhow::Result<Vec<AgentDto>> {
        let url = format!("{}/api/agents?project={}", self.base_url, project);
        let agents = self.http.get(&url).send().await?.json().await?;
        Ok(agents)
    }
}

fn base_url(mcp_url: &str) -> String {
    // Strip path: http://host:port/mcp -> http://host:port
    match reqwest::Url::parse(mcp_url) {
        Ok(mut u) => {
            u.set_path("");
            u.set_query(None);
            u.to_string().trim_end_matches('/').to_string()
        }
        Err(_) => mcp_url.to_string(),
    }
}
