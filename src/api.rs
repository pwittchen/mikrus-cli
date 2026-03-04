use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;

const BASE_URL: &str = "https://api.mikr.us";

pub struct MikrusClient {
    srv: String,
    key: String,
    client: Client,
    base_url: String,
}

impl MikrusClient {
    pub fn new(srv: String, key: String) -> Self {
        Self {
            srv,
            key,
            client: Client::new(),
            base_url: BASE_URL.to_string(),
        }
    }

    #[cfg(test)]
    pub fn with_base_url(srv: String, key: String, base_url: String) -> Self {
        Self {
            srv,
            key,
            client: Client::new(),
            base_url,
        }
    }

    async fn post(&self, endpoint: &str, extra_params: &[(&str, &str)]) -> Result<Value> {
        let url = format!("{}{endpoint}", self.base_url);
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

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};

    fn test_client(server: &Server) -> MikrusClient {
        MikrusClient::with_base_url(
            "srv12345".to_string(),
            "testkey".to_string(),
            server.url(),
        )
    }

    #[tokio::test]
    async fn test_info() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/info")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("srv".to_string(), "srv12345".to_string()),
                Matcher::UrlEncoded("key".to_string(), "testkey".to_string()),
            ]))
            .with_status(200)
            .with_body(r#"{"server_id":"12345"}"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.info().await.unwrap();

        assert_eq!(result["server_id"], "12345");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_servers() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/serwery")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("srv".to_string(), "srv12345".to_string()),
                Matcher::UrlEncoded("key".to_string(), "testkey".to_string()),
            ]))
            .with_status(200)
            .with_body(r#"[{"id":"1"},{"id":"2"}]"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.servers().await.unwrap();

        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 2);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_restart() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/restart")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("srv".to_string(), "srv12345".to_string()),
                Matcher::UrlEncoded("key".to_string(), "testkey".to_string()),
            ]))
            .with_status(200)
            .with_body(r#"{"status":"ok"}"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.restart().await.unwrap();

        assert_eq!(result["status"], "ok");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_logs_without_id() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/logs")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("srv".to_string(), "srv12345".to_string()),
                Matcher::UrlEncoded("key".to_string(), "testkey".to_string()),
            ]))
            .with_status(200)
            .with_body(r#"{"logs":[]}"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.logs(None).await.unwrap();

        assert_eq!(result["logs"], serde_json::json!([]));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_logs_with_id() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/logs/42")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("srv".to_string(), "srv12345".to_string()),
                Matcher::UrlEncoded("key".to_string(), "testkey".to_string()),
            ]))
            .with_status(200)
            .with_body(r#"{"log_id":"42","content":"log data"}"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.logs(Some("42")).await.unwrap();

        assert_eq!(result["log_id"], "42");
        assert_eq!(result["content"], "log data");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_exec() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/exec")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("srv".to_string(), "srv12345".to_string()),
                Matcher::UrlEncoded("key".to_string(), "testkey".to_string()),
                Matcher::UrlEncoded("cmd".to_string(), "uptime".to_string()),
            ]))
            .with_status(200)
            .with_body(r#"{"output":"up 10 days"}"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.exec("uptime").await.unwrap();

        assert_eq!(result["output"], "up 10 days");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_domain() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/domain")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("srv".to_string(), "srv12345".to_string()),
                Matcher::UrlEncoded("key".to_string(), "testkey".to_string()),
                Matcher::UrlEncoded("port".to_string(), "8080".to_string()),
                Matcher::UrlEncoded("domain".to_string(), "example.com".to_string()),
            ]))
            .with_status(200)
            .with_body(r#"{"status":"assigned"}"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.domain("8080", "example.com").await.unwrap();

        assert_eq!(result["status"], "assigned");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_api_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/info")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.info().await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("500"), "error should mention status code: {err}");
        mock.assert_async().await;
    }
}
