use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;

const STATUS_BASE_URL: &str = "https://status.mikr.us";
const STATUS_PAGE_SLUG: &str = "mikrus";
const HOST_LOOKUP_TEMPLATE: &str = "https://{srv}.mikrus.xyz";

pub struct StatusClient {
    client: Client,
    base_url: String,
    host_lookup_template: String,
}

impl StatusClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: STATUS_BASE_URL.to_string(),
            host_lookup_template: HOST_LOOKUP_TEMPLATE.to_string(),
        }
    }

    #[cfg(test)]
    pub fn with_urls(base_url: String, host_lookup_template: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            host_lookup_template,
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

    /// Resolve the physical hosting server name (e.g. `srv07`) for a user srv (e.g. `srv12345`)
    /// by fetching the user's default subdomain page and extracting the `srvNN` prefix from the
    /// first `<h1>` tag (which contains text like `srv07.mikr.us`).
    ///
    /// `<srv>.mikrus.xyz` serves the hosting server's certificate (CN=srvNN.mikr.us), so the
    /// TLS hostname check would normally fail; we relax it for this lookup only. The cert
    /// chain is still validated.
    pub async fn resolve_hosting_server(&self, user_srv: &str) -> Result<String> {
        let url = self.host_lookup_template.replace("{srv}", user_srv);
        let client = Client::builder()
            .danger_accept_invalid_hostnames(true)
            .build()
            .context("Failed to build HTTP client for host lookup")?;
        let response = client
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
            anyhow::bail!("{url} returned {status}");
        }
        extract_hosting_server_from_html(&body)
            .ok_or_else(|| anyhow::anyhow!("Could not parse hosting server from <h1> at {url}"))
    }
}

/// Extract the text content of the first `<h1>` element, stripping any nested tags.
fn extract_first_h1(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open = lower.find("<h1")?;
    let after_open = open + lower[open..].find('>')? + 1;
    let close_rel = lower[after_open..].find("</h1>")?;
    let raw = html.get(after_open..after_open + close_rel)?;

    let mut out = String::new();
    let mut in_tag = false;
    for c in raw.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    Some(out.trim().to_string())
}

/// From an `<h1>` like `srv07.mikr.us`, extract the leading `srvNN` token.
fn extract_hosting_server_from_html(html: &str) -> Option<String> {
    let h1 = extract_first_h1(html)?;
    let candidate: String = h1
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    if candidate.starts_with("srv") && candidate.len() > 3 {
        Some(candidate)
    } else {
        None
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

        let client = StatusClient::with_urls(server.url(), String::new());
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

        let client = StatusClient::with_urls(server.url(), String::new());
        let err = client.fetch().await.unwrap_err().to_string();
        assert!(err.contains("500"), "error should mention status: {err}");
    }

    #[test]
    fn test_extract_first_h1_simple() {
        let html = "<html><body><h1>srv07.mikr.us</h1></body></html>";
        assert_eq!(extract_first_h1(html).as_deref(), Some("srv07.mikr.us"));
    }

    #[test]
    fn test_extract_first_h1_with_attributes() {
        let html = r#"<h1 class="title" id="hdr">srv12.mikr.us</h1>"#;
        assert_eq!(extract_first_h1(html).as_deref(), Some("srv12.mikr.us"));
    }

    #[test]
    fn test_extract_first_h1_strips_nested_tags() {
        let html = "<h1>srv07.<span>mikr.us</span></h1>";
        assert_eq!(extract_first_h1(html).as_deref(), Some("srv07.mikr.us"));
    }

    #[test]
    fn test_extract_first_h1_case_insensitive() {
        let html = "<HTML><BODY><H1>srv07.mikr.us</H1></BODY></HTML>";
        assert_eq!(extract_first_h1(html).as_deref(), Some("srv07.mikr.us"));
    }

    #[test]
    fn test_extract_first_h1_returns_first() {
        let html = "<h1>srv07.mikr.us</h1><h1>other</h1>";
        assert_eq!(extract_first_h1(html).as_deref(), Some("srv07.mikr.us"));
    }

    #[test]
    fn test_extract_first_h1_missing() {
        assert_eq!(extract_first_h1("<html><body>no header</body></html>"), None);
    }

    #[test]
    fn test_extract_hosting_server_from_html_typical() {
        let html = "<h1>srv07.mikr.us</h1>";
        assert_eq!(extract_hosting_server_from_html(html).as_deref(), Some("srv07"));
    }

    #[test]
    fn test_extract_hosting_server_from_html_with_whitespace() {
        let html = "<h1>  srv19.mikr.us  </h1>";
        assert_eq!(extract_hosting_server_from_html(html).as_deref(), Some("srv19"));
    }

    #[test]
    fn test_extract_hosting_server_from_html_unrelated() {
        // Some other H1 that does not start with srvNN.
        let html = "<h1>Welcome to mikr.us</h1>";
        assert_eq!(extract_hosting_server_from_html(html), None);
    }

    #[test]
    fn test_extract_hosting_server_from_html_too_short() {
        let html = "<h1>srv.mikr.us</h1>";
        assert_eq!(extract_hosting_server_from_html(html), None);
    }

    #[tokio::test]
    async fn test_resolve_hosting_server_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/")
            .with_status(200)
            .with_body("<html><body><h1>srv07.mikr.us</h1></body></html>")
            .create_async()
            .await;

        let client = StatusClient::with_urls(String::new(), server.url());
        let host = client.resolve_hosting_server("srv12345").await.unwrap();
        mock.assert_async().await;
        assert_eq!(host, "srv07");
    }

    #[tokio::test]
    async fn test_resolve_hosting_server_substitutes_srv() {
        // The template includes `{srv}` — the user srv must be substituted into the URL path.
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/srv12345")
            .with_status(200)
            .with_body("<h1>srv07.mikr.us</h1>")
            .create_async()
            .await;

        let template = format!("{}/{{srv}}", server.url());
        let client = StatusClient::with_urls(String::new(), template);
        let host = client.resolve_hosting_server("srv12345").await.unwrap();
        mock.assert_async().await;
        assert_eq!(host, "srv07");
    }

    #[tokio::test]
    async fn test_resolve_hosting_server_http_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/")
            .with_status(404)
            .with_body("not found")
            .create_async()
            .await;

        let client = StatusClient::with_urls(String::new(), server.url());
        let err = client
            .resolve_hosting_server("srv12345")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("404"), "expected 404 in error: {err}");
    }

    #[tokio::test]
    async fn test_resolve_hosting_server_no_h1() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/")
            .with_status(200)
            .with_body("<html><body>no header here</body></html>")
            .create_async()
            .await;

        let client = StatusClient::with_urls(String::new(), server.url());
        let err = client
            .resolve_hosting_server("srv12345")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("Could not parse"), "expected parse error: {err}");
    }
}
