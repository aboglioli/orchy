use reqwest::{Client, Method, RequestBuilder, Response};
use secrecy::{ExposeSecret, SecretString};

use crate::config::Config;

pub struct OrchyClient {
    client: Client,
    base_url: String,
    api_key: SecretString,
    pub project: String,
    pub namespace: Option<String>,
    pub alias: Option<String>,
}

impl OrchyClient {
    pub fn new(config: &Config) -> Self {
        let base_url = config.url.trim_end_matches('/').to_owned();
        Self {
            client: Client::new(),
            base_url,
            api_key: config.api_key.clone(),
            project: config.project.clone(),
            namespace: Some(config.namespace.clone()).filter(|s| !s.is_empty() && s != "/"),
            alias: config.alias.clone(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api{}", self.base_url, path)
    }

    fn project_url(&self, path: &str) -> String {
        format!("{}/api/projects/{}{}", self.base_url, self.project, path)
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
        let req = self.client.request(method, url);
        let key = self.api_key.expose_secret();
        if key.is_empty() {
            return Ok(req);
        }
        Ok(req.bearer_auth(key))
    }

    async fn send(&self, req: RequestBuilder) -> CliResult<Response> {
        let resp = req.send().await?;
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
        let v = resp.json::<serde_json::Value>().await?;
        Ok(v)
    }

    /// Convenience: GET a project-scoped URL and return parsed JSON value.
    pub async fn get_project_json(&self, path: &str) -> CliResult<serde_json::Value> {
        let resp = self.get_project(path).await?;
        let v = resp.json::<serde_json::Value>().await?;
        Ok(v)
    }

    pub async fn create_organization_json(
        &self,
        id: &str,
        name: &str,
    ) -> CliResult<serde_json::Value> {
        let body = serde_json::json!({ "id": id, "name": name });
        self.post_json("/organizations", Some(&body)).await
    }

    pub async fn generate_api_key_json(&self, name: &str) -> CliResult<serde_json::Value> {
        let body = serde_json::json!({ "name": name });
        self.post_json("/api-keys", Some(&body)).await
    }

    pub async fn list_api_keys_json(&self) -> CliResult<serde_json::Value> {
        self.get_json("/api-keys").await
    }

    /// Convenience: POST to org-scoped URL and return parsed JSON value.
    pub async fn post_json(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<serde_json::Value> {
        let resp = self.post(path, body).await?;
        let v = resp.json::<serde_json::Value>().await?;
        Ok(v)
    }

    /// Convenience: POST to project-scoped URL and return parsed JSON value.
    pub async fn post_project_json(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<serde_json::Value> {
        let resp = self.post_project(path, body).await?;
        let v = resp.json::<serde_json::Value>().await?;
        Ok(v)
    }

    /// Convenience: PATCH to org-scoped URL and return parsed JSON value.
    pub async fn patch_json(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<serde_json::Value> {
        let resp = self.patch(path, body).await?;
        let v = resp.json::<serde_json::Value>().await?;
        Ok(v)
    }

    /// Convenience: PATCH to project-scoped URL and return parsed JSON value.
    pub async fn patch_project_json(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<serde_json::Value> {
        let resp = self.patch_project(path, body).await?;
        let v = resp.json::<serde_json::Value>().await?;
        Ok(v)
    }

    /// Convenience: PUT to project-scoped URL and return parsed JSON value.
    pub async fn put_project_json(
        &self,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> CliResult<serde_json::Value> {
        let resp = self.put_project(path, body).await?;
        let v = resp.json::<serde_json::Value>().await?;
        Ok(v)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error(transparent)]
    Request(#[from] reqwest::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },

    #[error("alias is required — set it in config or pass --agent <id>")]
    MissingAgentId,
}

pub type CliResult<T> = Result<T, CliError>;
