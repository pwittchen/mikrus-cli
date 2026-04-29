use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;

const STATUS_BASE_URL: &str = "https://status.mikr.us";
const STATUS_PAGE_SLUG: &str = "mikrus";

pub struct StatusClient {
    client: Client,
    base_url: String,
}

impl StatusClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: STATUS_BASE_URL.to_string(),
        }
    }

    #[cfg(test)]
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    async fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Failed to fetch {url}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .with_context(|| format!("Failed to read response from {url}"))?;
        if !status.is_success() {
            anyhow::bail!("Status page returned {status} for {path}: {body}");
        }
        serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse JSON from {url}: {body}"))
    }

    /// Fetch monitor groups and the latest heartbeats, merged into one value.
    pub async fn fetch(&self) -> Result<Value> {
        let config = self
            .get(&format!("/api/status-page/{STATUS_PAGE_SLUG}"))
            .await?;
        let heartbeat = self
            .get(&format!("/api/status-page/heartbeat/{STATUS_PAGE_SLUG}"))
            .await?;

        Ok(serde_json::json!({
            "publicGroupList": config.get("publicGroupList").cloned().unwrap_or(Value::Null),
            "heartbeatList": heartbeat.get("heartbeatList").cloned().unwrap_or(Value::Null),
            "uptimeList": heartbeat.get("uptimeList").cloned().unwrap_or(Value::Null),
            "incident": config.get("incident").cloned().unwrap_or(Value::Null),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    #[tokio::test]
    async fn test_fetch_merges_config_and_heartbeats() {
        let mut server = Server::new_async().await;
        let cfg_mock = server
            .mock("GET", "/api/status-page/mikrus")
            .with_status(200)
            .with_body(
                r#"{"publicGroupList":[{"id":2,"name":"Serwery","monitorList":[{"id":9,"name":"srv07"}]}],"incident":null}"#,
            )
            .create_async()
            .await;
        let hb_mock = server
            .mock("GET", "/api/status-page/heartbeat/mikrus")
            .with_status(200)
            .with_body(
                r#"{"heartbeatList":{"9":[{"status":1,"time":"2026-01-01","msg":"","ping":25}]},"uptimeList":{"9_24":1}}"#,
            )
            .create_async()
            .await;

        let client = StatusClient::with_base_url(server.url());
        let value = client.fetch().await.unwrap();

        cfg_mock.assert_async().await;
        hb_mock.assert_async().await;

        assert!(value.get("publicGroupList").unwrap().is_array());
        assert!(
            value
                .get("heartbeatList")
                .unwrap()
                .get("9")
                .unwrap()
                .is_array()
        );
        assert_eq!(value.get("uptimeList").unwrap().get("9_24").unwrap(), 1);
    }

    #[tokio::test]
    async fn test_fetch_propagates_http_errors() {
        let mut server = Server::new_async().await;
        let _cfg_mock = server
            .mock("GET", "/api/status-page/mikrus")
            .with_status(500)
            .with_body("oops")
            .create_async()
            .await;

        let client = StatusClient::with_base_url(server.url());
        let err = client.fetch().await.unwrap_err().to_string();
        assert!(err.contains("500"), "error should mention status: {err}");
    }
}
