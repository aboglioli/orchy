use reqwest::{Client, Method, RequestBuilder, Response};

use crate::config::Config;

/// HTTP client for orchy REST API. Stateless — every call is a single request.
pub struct OrchyClient {
    client: Client,
    base_url: String,
    api_key: String,
    org: String,
    project: String,
    pub alias: Option<String>,
}

impl OrchyClient {
    pub fn new(config: &Config) -> Self {
        let base_url = config.url.trim_end_matches('/').to_string();
        Self {
            client: Client::new(),
            base_url,
            api_key: config.api_key.clone(),
            org: config.org.clone(),
            project: config.project.clone(),
            alias: config.alias.clone(),
        }
    }

    /// Build an org-scoped URL: /organizations/{org}/...
    fn url(&self, path: &str) -> String {
        format!("{}/api/organizations/{}{}", self.base_url, self.org, path)
    }

    /// Build a project-scoped URL: /organizations/{org}/projects/{project}/...
    fn project_url(&self, path: &str) -> String {
        format!(
            "{}/api/organizations/{}/projects/{}{}",
            self.base_url, self.org, self.project, path
        )
    }

    /// GET request.
    pub async fn get(&self, path: &str) -> CliResult<Response> {
        self.request(Method::GET, path).await
    }

    /// GET request to a project-scoped path.
    pub async fn get_project(&self, path: &str) -> CliResult<Response> {
        let url = self.project_url(path);
        let req = self.request_url(Method::GET, &url).await?;
        self.send(req).await
    }

    /// POST request with optional JSON body.
    pub async fn post(&self, path: &str, body: Option<&serde_json::Value>) -> CliResult<Response> {
        let mut req = self.request_url(Method::POST, &self.url(path)).await?;
        if let Some(b) = body {
            req = req.json(b);
        }
        self.send(req).await
    }

    /// POST request to a project-scoped path with optional JSON body.
    pub async fn post_project(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<Response> {
        let mut req = self
            .request_url(Method::POST, &self.project_url(path))
            .await?;
        if let Some(b) = body {
            req = req.json(b);
        }
        self.send(req).await
    }

    /// PATCH request with optional JSON body.
    pub async fn patch(&self, path: &str, body: Option<&serde_json::Value>) -> CliResult<Response> {
        let mut req = self.request_url(Method::PATCH, &self.url(path)).await?;
        if let Some(b) = body {
            req = req.json(b);
        }
        self.send(req).await
    }

    /// PATCH request to a project-scoped path with optional JSON body.
    pub async fn patch_project(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<Response> {
        let mut req = self
            .request_url(Method::PATCH, &self.project_url(path))
            .await?;
        if let Some(b) = body {
            req = req.json(b);
        }
        self.send(req).await
    }

    /// DELETE request.
    pub async fn delete(&self, path: &str) -> CliResult<Response> {
        self.request(Method::DELETE, path).await
    }

    /// DELETE request to a project-scoped path.
    pub async fn delete_project(&self, path: &str) -> CliResult<Response> {
        let url = self.project_url(path);
        let req = self.request_url(Method::DELETE, &url).await?;
        self.send(req).await
    }

    /// PUT request to a project-scoped path with optional JSON body.
    pub async fn put_project(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<Response> {
        let mut req = self
            .request_url(Method::PUT, &self.project_url(path))
            .await?;
        if let Some(b) = body {
            req = req.json(b);
        }
        self.send(req).await
    }

    async fn request(&self, method: Method, path: &str) -> CliResult<Response> {
        let url = self.url(path);
        let req = self.request_url(method, &url).await?;
        self.send(req).await
    }

    async fn request_url(&self, method: Method, url: &str) -> CliResult<RequestBuilder> {
        Ok(self.client.request(method, url).bearer_auth(&self.api_key))
    }

    async fn send(&self, req: RequestBuilder) -> CliResult<Response> {
        let resp = req
            .send()
            .await
            .map_err(|e| CliError::Request(e.to_string()))?;
        if resp.status().is_client_error() || resp.status().is_server_error() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::Http {
                status: status.as_u16(),
                body,
            });
        }
        Ok(resp)
    }

    /// Convenience: GET an org-scoped URL and return parsed JSON value.
    pub async fn get_json(&self, path: &str) -> CliResult<serde_json::Value> {
        let resp = self.get(path).await?;
        let v = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| CliError::Request(e.to_string()))?;
        Ok(v)
    }

    /// Convenience: GET a project-scoped URL and return parsed JSON value.
    pub async fn get_project_json(&self, path: &str) -> CliResult<serde_json::Value> {
        let resp = self.get_project(path).await?;
        let v = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| CliError::Request(e.to_string()))?;
        Ok(v)
    }

    /// Convenience: POST to org-scoped URL and return parsed JSON value.
    pub async fn post_json(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<serde_json::Value> {
        let resp = self.post(path, body).await?;
        let v = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| CliError::Request(e.to_string()))?;
        Ok(v)
    }

    /// Convenience: POST to project-scoped URL and return parsed JSON value.
    pub async fn post_project_json(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<serde_json::Value> {
        let resp = self.post_project(path, body).await?;
        let v = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| CliError::Request(e.to_string()))?;
        Ok(v)
    }

    /// Convenience: PATCH to org-scoped URL and return parsed JSON value.
    pub async fn patch_json(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<serde_json::Value> {
        let resp = self.patch(path, body).await?;
        let v = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| CliError::Request(e.to_string()))?;
        Ok(v)
    }

    /// Convenience: PATCH to project-scoped URL and return parsed JSON value.
    pub async fn patch_project_json(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<serde_json::Value> {
        let resp = self.patch_project(path, body).await?;
        let v = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| CliError::Request(e.to_string()))?;
        Ok(v)
    }

    /// Convenience: PUT to project-scoped URL and return parsed JSON value.
    pub async fn put_project_json(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<serde_json::Value> {
        let resp = self.put_project(path, body).await?;
        let v = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| CliError::Request(e.to_string()))?;
        Ok(v)
    }
}

#[derive(Debug)]
pub enum CliError {
    Request(String),
    Http { status: u16, body: String },
    MissingAgentId,
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Request(e) => write!(f, "request error: {e}"),
            CliError::Http { status, body } => write!(f, "HTTP {status}: {body}"),
            CliError::MissingAgentId => write!(
                f,
                "alias is required — set it in config or pass --agent <id>"
            ),
        }
    }
}

pub type CliResult<T> = Result<T, CliError>;
