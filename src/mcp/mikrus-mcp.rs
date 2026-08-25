//! `mikrus-mcp` — Model Context Protocol server for the mikr.us VPS API.
//!
//! Exposes the same operations as the `mikrus` CLI as MCP tools over stdio,
//! so MCP-aware clients (Claude Desktop, Claude Code, etc.) can manage a
//! mikr.us VPS via natural language.
//!
//! Credentials are resolved exactly like the CLI: `MIKRUS_SRV` / `MIKRUS_KEY`
//! env vars, then a profile from `~/.mikrus`. Each tool also accepts an
//! optional `profile` argument to pick a specific entry from `~/.mikrus`.

use std::sync::{Arc, Mutex};

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
use mikrus_cli::config::{self, LoadedConfig};
use mikrus_cli::status::StatusClient;

#[derive(Clone)]
struct MikrusServer {
    /// Config as last read from disk. `ctx` and `ctx_switch` refresh it, so a
    /// long-running server picks up edits to `~/.mikrus` without a restart.
    config: Arc<Mutex<Arc<LoadedConfig>>>,
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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct CtxSwitchArgs {
    /// Name of the profile to make the default one. Must be one of the names
    /// reported by `ctx`.
    name: String,
}

#[tool_router]
impl MikrusServer {
    fn new() -> Self {
        Self {
            config: Arc::new(Mutex::new(Arc::new(load_config()))),
            env_srv: std::env::var("MIKRUS_SRV").ok(),
            env_key: std::env::var("MIKRUS_KEY").ok(),
            tool_router: Self::tool_router(),
        }
    }

    /// Config as last read from disk.
    fn loaded(&self) -> Arc<LoadedConfig> {
        self.config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Re-read `~/.mikrus` and `./.mikrus` and return the fresh config.
    fn reload(&self) -> Arc<LoadedConfig> {
        let fresh = Arc::new(load_config());
        *self
            .config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = fresh.clone();
        fresh
    }

    /// Resolve `(srv, key)` using the same priority as the CLI:
    /// 1. `MIKRUS_SRV`/`MIKRUS_KEY` env vars (both must be set)
    /// 2. Named profile from `~/.mikrus`
    /// 3. Auto-select if `~/.mikrus` has exactly one profile, or one is marked
    ///    `default = true` (see `mikrus ctx`)
    fn resolve_creds(&self, profile: Option<&str>) -> Result<(String, String), McpError> {
        if let (Some(srv), Some(key)) = (&self.env_srv, &self.env_key) {
            return Ok((srv.clone(), key.clone()));
        }
        let loaded = self.loaded();
        let servers = &loaded.merged.servers;
        if let Some(name) = profile {
            let p = servers.get(name).ok_or_else(|| {
                McpError::invalid_params(
                    format!("profile '{name}' not found in ~/.mikrus"),
                    None,
                )
            })?;
            return Ok((p.srv.clone(), p.key.clone()));
        }
        if servers.len() == 1 {
            let (_, p) = servers.iter().next().unwrap();
            return Ok((p.srv.clone(), p.key.clone()));
        }
        // Unlike the CLI, only an explicit `default = true` is auto-selected here —
        // never the first profile, so a tool call can't silently hit the wrong server.
        if let Some(p) = loaded
            .effective_default()
            .filter(|(_, explicit)| *explicit)
            .and_then(|(name, _)| servers.get(name))
        {
            return Ok((p.srv.clone(), p.key.clone()));
        }
        if servers.is_empty() {
            return Err(McpError::invalid_params(
                "no credentials available — set MIKRUS_SRV/MIKRUS_KEY env vars or configure ~/.mikrus",
                None,
            ));
        }
        let names: Vec<&str> = servers.keys().map(String::as_str).collect();
        Err(McpError::invalid_params(
            format!(
                "multiple profiles configured ({}); pass `profile` argument to select one, \
                 or mark one as the default with `ctx_switch`",
                names.join(", ")
            ),
            None,
        ))
    }

    fn client(&self, profile: Option<&str>) -> Result<MikrusClient, McpError> {
        let (srv, key) = self.resolve_creds(profile)?;
        Ok(MikrusClient::new(srv, key))
    }

    #[tool(
        description = "Show information about the mikr.us VPS (server ID, expiry, parameters).",
        annotations(read_only_hint = true)
    )]
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

    #[tool(
        description = "List all VPS servers owned by the user on mikr.us.",
        annotations(read_only_hint = true)
    )]
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
        description = "Restart the mikr.us VPS. Side-effectful: causes a brief outage. Confirm with the user before invoking.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false)
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
        description = "Show server log entries. Omit `id` for the recent list, or pass a specific log entry ID.",
        annotations(read_only_hint = true)
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
        description = "Trigger the mikr.us 'amfetamina' performance boost on the VPS (rate-limited by the provider).",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false)
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

    #[tool(
        description = "Show MySQL/MariaDB database credentials for the VPS.",
        annotations(read_only_hint = true)
    )]
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
        description = "Execute an arbitrary shell command on the VPS via the mikr.us API. Side-effectful: runs as the VPS user. Confirm with the user before invoking destructive commands.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false)
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

    #[tool(
        description = "Show disk, memory, and uptime statistics for the VPS.",
        annotations(read_only_hint = true)
    )]
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

    #[tool(
        description = "Show TCP/UDP ports configured on the VPS.",
        annotations(read_only_hint = true)
    )]
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

    #[tool(
        description = "Show mikr.us cloud services and their stats for the user.",
        annotations(read_only_hint = true)
    )]
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
        description = "Assign a domain to a port on the VPS. Omit `domain` for auto-assignment from `*.tojest.dev`, `*.bieda.it`, `*.toadres.pl`, `*.byst.re`. Side-effectful: changes routing.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true)
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
        description = "Show mikr.us infrastructure status from https://status.mikr.us — monitor groups, latest heartbeats, and uptime per host. Public endpoint, no credentials required.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn status(&self) -> Result<CallToolResult, McpError> {
        let value = StatusClient::new().fetch().await.map_err(api_err)?;
        Ok(json_result(&value))
    }

    #[tool(
        description = "List profiles defined in `~/.mikrus` (names and srv values). Useful before calling other tools that take a `profile` argument. Use `ctx` instead for the full picture (default server, which config file defines what).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn list_profiles(&self) -> Result<CallToolResult, McpError> {
        let loaded = self.loaded();
        let profiles: Vec<Value> = loaded
            .merged
            .servers
            .iter()
            .map(|(name, p)| {
                serde_json::json!({
                    "name": name,
                    "srv": p.srv,
                    "ssh": p.ssh,
                    "default": p.is_default(),
                })
            })
            .collect();
        let payload = serde_json::json!({
            "config_path": config::config_path().map(|p| p.display().to_string()),
            "local_config_path": config::local_config_path().map(|p| p.display().to_string()),
            "env_srv_set": self.env_srv.is_some(),
            "env_key_set": self.env_key.is_some(),
            "profiles": profiles,
        });
        Ok(json_result(&payload))
    }

    #[tool(
        name = "ctx",
        description = "Show the current mikr.us context, like `mikrus ctx`: every configured server profile, which config file defines it (global `~/.mikrus` or project-local `./.mikrus`), and which one is the default that credential-less calls use. Re-reads the config files, so it also reflects edits made since the server started.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn ctx(&self) -> Result<CallToolResult, McpError> {
        Ok(json_result(&ctx_payload(
            &self.reload(),
            self.env_srv.is_some(),
            self.env_key.is_some(),
            None,
        )))
    }

    #[tool(
        name = "ctx_switch",
        description = "Switch the default mikr.us server, like `mikrus ctx switch <name>`: marks the named profile with `default = true` in the config file that defines it (clearing the marker from the others) so later calls without a `profile` argument use it. Side-effectful: rewrites `~/.mikrus` or `./.mikrus` on disk and changes which server every other tool talks to by default — confirm with the user first. Call `ctx` for the available names.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true)
    )]
    async fn ctx_switch(
        &self,
        Parameters(args): Parameters<CtxSwitchArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Work off a fresh read: the file may have changed since startup, and the
        // marker must land in whichever file defines the profile right now.
        let loaded = self.reload();
        let previous = loaded
            .effective_default()
            .map(|(name, _)| name.to_string());

        let outcome = loaded
            .switch_default(&args.name)
            .map_err(|e| McpError::invalid_params(format!("{e:#}"), None))?;

        let mut notes = Vec::new();
        if outcome.wrote_local {
            notes.push(
                "Written to the project-local config — the global default is unchanged."
                    .to_string(),
            );
        }
        if let Some(cleared) = &outcome.cleared_local {
            notes.push(format!(
                "Removed the default marker from {} so this one applies here.",
                cleared.display()
            ));
        }
        if self.env_srv.is_some() && self.env_key.is_some() {
            notes.push(
                "MIKRUS_SRV/MIKRUS_KEY are set and take priority, so tool calls keep using \
                 those credentials regardless of the default profile."
                    .to_string(),
            );
        }

        let switched = serde_json::json!({
            "switched": true,
            "previous_default": previous,
            "default": args.name,
            "written_to": outcome.path.display().to_string(),
            "wrote_local_config": outcome.wrote_local,
            "cleared_default_in": outcome.cleared_local.map(|p| p.display().to_string()),
            "notes": notes,
        });

        // Report the context as it stands after the write.
        Ok(json_result(&ctx_payload(
            &self.reload(),
            self.env_srv.is_some(),
            self.env_key.is_some(),
            Some(switched),
        )))
    }
}

fn load_config() -> LoadedConfig {
    config::load_all().unwrap_or_else(|e| {
        tracing::warn!("failed to load config: {e:#}");
        LoadedConfig::default()
    })
}

/// Shared `ctx` / `ctx_switch` response: the configured servers and the default one.
fn ctx_payload(
    loaded: &LoadedConfig,
    env_srv_set: bool,
    env_key_set: bool,
    switched: Option<Value>,
) -> Value {
    let default = loaded.effective_default();
    let default_name = default.map(|(name, _)| name);

    let servers: Vec<Value> = loaded
        .merged
        .servers
        .iter()
        .map(|(name, p)| {
            let is_local = loaded.is_local(name);
            serde_json::json!({
                "name": name,
                "srv": p.srv,
                "ssh": p.ssh,
                "default": default_name == Some(name.as_str()),
                "source": if is_local { "local" } else { "global" },
                "defined_in": loaded.defining_path(name).map(|p| p.display().to_string()),
                "overrides_global": is_local && loaded.global.servers.contains_key(name),
            })
        })
        .collect();

    let mut payload = serde_json::json!({
        "global_config_path": loaded
            .global_path
            .as_ref()
            .map(|p| p.display().to_string()),
        "local_config_path": loaded.local_path.as_ref().map(|p| p.display().to_string()),
        "env_srv_set": env_srv_set,
        "env_key_set": env_key_set,
        // Env vars outrank every profile, so the default below is not what tools use.
        "env_credentials_take_priority": env_srv_set && env_key_set,
        "default": default.map(|(name, explicit)| {
            serde_json::json!({
                "name": name,
                // `false` = nothing is marked `default = true`; the CLI falls back to the
                // first profile, while this server asks for an explicit `profile` instead.
                "explicit": explicit,
            })
        }),
        "servers": servers,
    });

    if let Some(switched) = switched {
        payload["switch"] = switched;
    }
    payload
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
             cloud, domain, status, list_profiles, ctx, ctx_switch. Credentials \
             come from MIKRUS_SRV/MIKRUS_KEY env vars or `~/.mikrus`; pass \
             `profile` to pick a specific config entry for a single call. `ctx` \
             shows the configured servers and which one is the default, and \
             `ctx_switch` changes that default persistently. `restart`, `exec`, \
             `domain`, `amfetamina`, and `ctx_switch` are side-effectful — \
             confirm with the user first."
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
