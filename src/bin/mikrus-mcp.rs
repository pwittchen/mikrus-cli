//! `mikrus-mcp` — Model Context Protocol server for the mikr.us VPS API.
//!
//! Exposes the same operations as the `mikrus` CLI as MCP tools over stdio,
//! so MCP-aware clients (Claude Desktop, Claude Code, etc.) can manage a
//! mikr.us VPS via natural language.
//!
//! Credentials are resolved exactly like the CLI: `MIKRUS_SRV` / `MIKRUS_KEY`
//! env vars, then a profile from `~/.mikrus`. Each tool also accepts an
//! optional `profile` argument to pick a specific entry from `~/.mikrus`.

use std::sync::Arc;

use anyhow::Result;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use serde_json::Value;
use tracing_subscriber::EnvFilter;

use mikrus_cli::api::MikrusClient;
use mikrus_cli::config::{self, Config};
use mikrus_cli::status::StatusClient;

#[derive(Clone)]
struct MikrusServer {
    config: Arc<Config>,
    env_srv: Option<String>,
    env_key: Option<String>,
    // Used by the `#[tool_handler]` macro to dispatch tool calls.
    #[allow(dead_code)]
    tool_router: ToolRouter<MikrusServer>,
}

#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
struct ProfileArg {
    /// Profile name from `~/.mikrus` to use for this call. Omit to use
    /// `MIKRUS_SRV`/`MIKRUS_KEY` env vars or auto-select the only profile.
    #[serde(default)]
    profile: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
struct LogsArgs {
    #[serde(default)]
    profile: Option<String>,
    /// Optional log entry ID. Omit to list recent log entries.
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ExecArgs {
    #[serde(default)]
    profile: Option<String>,
    /// Shell command to execute on the server.
    cmd: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct DomainArgs {
    #[serde(default)]
    profile: Option<String>,
    /// Port number to attach the domain to (e.g. "8080").
    port: String,
    /// Domain name to assign. Omit for auto-assignment from one of
    /// `*.tojest.dev`, `*.bieda.it`, `*.toadres.pl`, `*.byst.re`.
    #[serde(default)]
    domain: Option<String>,
}

#[tool_router]
impl MikrusServer {
    fn new() -> Self {
        let config = config::load().unwrap_or_else(|e| {
            tracing::warn!("failed to load ~/.mikrus: {e:#}");
            Config::default()
        });
        Self {
            config: Arc::new(config),
            env_srv: std::env::var("MIKRUS_SRV").ok(),
            env_key: std::env::var("MIKRUS_KEY").ok(),
            tool_router: Self::tool_router(),
        }
    }

    /// Resolve `(srv, key)` using the same priority as the CLI:
    /// 1. `MIKRUS_SRV`/`MIKRUS_KEY` env vars (both must be set)
    /// 2. Named profile from `~/.mikrus`
    /// 3. Auto-select if `~/.mikrus` has exactly one profile
    fn resolve_creds(&self, profile: Option<&str>) -> Result<(String, String), McpError> {
        if let (Some(srv), Some(key)) = (&self.env_srv, &self.env_key) {
            return Ok((srv.clone(), key.clone()));
        }
        if let Some(name) = profile {
            let p = self.config.servers.get(name).ok_or_else(|| {
                McpError::invalid_params(
                    format!("profile '{name}' not found in ~/.mikrus"),
                    None,
                )
            })?;
            return Ok((p.srv.clone(), p.key.clone()));
        }
        if self.config.servers.len() == 1 {
            let (_, p) = self.config.servers.iter().next().unwrap();
            return Ok((p.srv.clone(), p.key.clone()));
        }
        if self.config.servers.is_empty() {
            return Err(McpError::invalid_params(
                "no credentials available — set MIKRUS_SRV/MIKRUS_KEY env vars or configure ~/.mikrus",
                None,
            ));
        }
        let names: Vec<&str> = self.config.servers.keys().map(String::as_str).collect();
        Err(McpError::invalid_params(
            format!(
                "multiple profiles configured ({}); pass `profile` argument to select one",
                names.join(", ")
            ),
            None,
        ))
    }

    fn client(&self, profile: Option<&str>) -> Result<MikrusClient, McpError> {
        let (srv, key) = self.resolve_creds(profile)?;
        Ok(MikrusClient::new(srv, key))
    }

    #[tool(description = "Show information about the mikr.us VPS (server ID, expiry, parameters).")]
    async fn info(
        &self,
        Parameters(args): Parameters<ProfileArg>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client(args.profile.as_deref())?
            .info()
            .await
            .map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(description = "List all VPS servers owned by the user on mikr.us.")]
    async fn servers(
        &self,
        Parameters(args): Parameters<ProfileArg>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client(args.profile.as_deref())?
            .servers()
            .await
            .map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(
        description = "Restart the mikr.us VPS. Side-effectful: causes a brief outage. Confirm with the user before invoking."
    )]
    async fn restart(
        &self,
        Parameters(args): Parameters<ProfileArg>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client(args.profile.as_deref())?
            .restart()
            .await
            .map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(
        description = "Show server log entries. Omit `id` for the recent list, or pass a specific log entry ID."
    )]
    async fn logs(
        &self,
        Parameters(args): Parameters<LogsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client(args.profile.as_deref())?
            .logs(args.id.as_deref())
            .await
            .map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(
        description = "Trigger the mikr.us 'amfetamina' performance boost on the VPS (rate-limited by the provider)."
    )]
    async fn amfetamina(
        &self,
        Parameters(args): Parameters<ProfileArg>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client(args.profile.as_deref())?
            .amfetamina()
            .await
            .map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(description = "Show MySQL/MariaDB database credentials for the VPS.")]
    async fn db(
        &self,
        Parameters(args): Parameters<ProfileArg>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client(args.profile.as_deref())?
            .db()
            .await
            .map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(
        description = "Execute an arbitrary shell command on the VPS via the mikr.us API. Side-effectful: runs as the VPS user. Confirm with the user before invoking destructive commands."
    )]
    async fn exec(
        &self,
        Parameters(args): Parameters<ExecArgs>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client(args.profile.as_deref())?
            .exec(&args.cmd)
            .await
            .map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(description = "Show disk, memory, and uptime statistics for the VPS.")]
    async fn stats(
        &self,
        Parameters(args): Parameters<ProfileArg>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client(args.profile.as_deref())?
            .stats()
            .await
            .map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(description = "Show TCP/UDP ports configured on the VPS.")]
    async fn ports(
        &self,
        Parameters(args): Parameters<ProfileArg>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client(args.profile.as_deref())?
            .ports()
            .await
            .map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(description = "Show mikr.us cloud services and their stats for the user.")]
    async fn cloud(
        &self,
        Parameters(args): Parameters<ProfileArg>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client(args.profile.as_deref())?
            .cloud()
            .await
            .map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(
        description = "Assign a domain to a port on the VPS. Omit `domain` for auto-assignment from `*.tojest.dev`, `*.bieda.it`, `*.toadres.pl`, `*.byst.re`. Side-effectful: changes routing."
    )]
    async fn domain(
        &self,
        Parameters(args): Parameters<DomainArgs>,
    ) -> Result<CallToolResult, McpError> {
        let domain = args.domain.as_deref().unwrap_or("-");
        let value = self
            .client(args.profile.as_deref())?
            .domain(&args.port, domain)
            .await
            .map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(
        description = "Show mikr.us infrastructure status from https://status.mikr.us — monitor groups, latest heartbeats, and uptime per host. Public endpoint, no credentials required."
    )]
    async fn status(&self) -> Result<CallToolResult, McpError> {
        let value = StatusClient::new().fetch().await.map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(
        description = "List profiles defined in `~/.mikrus` (names and srv values). Useful before calling other tools that take a `profile` argument."
    )]
    async fn list_profiles(&self) -> Result<CallToolResult, McpError> {
        let profiles: Vec<Value> = self
            .config
            .servers
            .iter()
            .map(|(name, p)| {
                serde_json::json!({
                    "name": name,
                    "srv": p.srv,
                    "ssh": p.ssh,
                })
            })
            .collect();
        let payload = serde_json::json!({
            "config_path": config::config_path().map(|p| p.display().to_string()),
            "env_srv_set": self.env_srv.is_some(),
            "env_key_set": self.env_key.is_some(),
            "profiles": profiles,
        });
        Ok(json_result(&payload))
    }
}

#[tool_handler]
impl ServerHandler for MikrusServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder().enable_tools().build(),
        )
        .with_server_info(Implementation::new(
            "mikrus-mcp",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_protocol_version(ProtocolVersion::V_2024_11_05)
        .with_instructions(
            "MCP server for the mikr.us VPS API. Tools mirror the `mikrus` CLI: \
             info, servers, restart, logs, amfetamina, db, exec, stats, ports, \
             cloud, domain, status, list_profiles. Credentials come from \
             MIKRUS_SRV/MIKRUS_KEY env vars or `~/.mikrus`; pass `profile` to \
             pick a specific config entry. `restart`, `exec`, `domain`, and \
             `amfetamina` are side-effectful — confirm with the user first."
                .to_string(),
        )
    }
}

fn api_err(e: anyhow::Error) -> McpError {
    McpError::internal_error(format!("{e:#}"), None)
}

fn json_result(value: &Value) -> CallToolResult {
    let text = serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| value.to_string());
    CallToolResult::success(vec![Content::text(text)])
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("starting mikrus-mcp");

    let server = MikrusServer::new();
    let service = server.serve(stdio()).await.inspect_err(|e| {
        tracing::error!("serve error: {e:?}");
    })?;
    service.waiting().await?;
    Ok(())
}
