use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;

const BASE_URL: &str = "https://api.mikr.us";

pub struct MikrusClient {
    srv: String,
    key: String,
    client: Client,
}

impl MikrusClient {
    pub fn new(srv: String, key: String) -> Self {
        Self {
            srv,
            key,
            client: Client::new(),
        }
    }

    async fn post(&self, endpoint: &str, extra_params: &[(&str, &str)]) -> Result<Value> {
        let url = format!("{BASE_URL}{endpoint}");
        let mut params = vec![("srv", self.srv.as_str()), ("key", self.key.as_str())];
        params.extend_from_slice(extra_params);

        let response = self
            .client
            .post(&url)
            .form(&params)
            .send()
            .await
            .with_context(|| format!("Failed to send request to {endpoint}"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .with_context(|| format!("Failed to read response from {endpoint}"))?;

        if !status.is_success() {
            anyhow::bail!("API returned {status} for {endpoint}: {body}");
        }

        serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse JSON from {endpoint}: {body}"))
    }

    pub async fn info(&self) -> Result<Value> {
        self.post("/info", &[]).await
    }

    pub async fn servers(&self) -> Result<Value> {
        self.post("/serwery", &[]).await
    }

    pub async fn restart(&self) -> Result<Value> {
        self.post("/restart", &[]).await
    }

    pub async fn logs(&self, id: Option<&str>) -> Result<Value> {
        match id {
            Some(id) => self.post(&format!("/logs/{id}"), &[]).await,
            None => self.post("/logs", &[]).await,
        }
    }

    pub async fn amfetamina(&self) -> Result<Value> {
        self.post("/amfetamina", &[]).await
    }

    pub async fn db(&self) -> Result<Value> {
        self.post("/db", &[]).await
    }

    pub async fn exec(&self, cmd: &str) -> Result<Value> {
        self.post("/exec", &[("cmd", cmd)]).await
    }

    pub async fn stats(&self) -> Result<Value> {
        self.post("/stats", &[]).await
    }

    pub async fn ports(&self) -> Result<Value> {
        self.post("/porty", &[]).await
    }

    pub async fn cloud(&self) -> Result<Value> {
        self.post("/cloud", &[]).await
    }

    pub async fn domain(&self, port: &str, domain: &str) -> Result<Value> {
        self.post("/domain", &[("port", port), ("domain", domain)])
            .await
    }
}
