use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use mikrus_cli::api::MikrusClient;
use mikrus_cli::config::{Config, LoadedConfig, Profile};
use mikrus_cli::{config, format, status};

#[derive(Parser)]
#[command(name = "mikrus-cli", about = "CLI tool for managing mikr.us VPS")]
struct Cli {
    /// Server name (e.g. srv12345)
    #[arg(long, env = "MIKRUS_SRV", global = true)]
    srv: Option<String>,

    /// API key
    #[arg(long, env = "MIKRUS_KEY", global = true)]
    key: Option<String>,

    /// Output raw JSON instead of formatted text
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show server information
    Info,
    /// List all user servers
    Servers,
    /// Restart the server
    Restart,
    /// Show log entries (optional: specific log ID)
    Logs(LogsArgs),
    /// Performance boost
    Amfetamina,
    /// Show database credentials
    Db,
    /// Execute a command on the server
    Exec {
        /// Command to execute
        cmd: String,
    },
    /// Show disk/memory/uptime statistics
    Stats {
        /// Truncate long lines at this width, adding "..." (0 = no truncation)
        #[arg(long, default_value_t = 0)]
        truncate: usize,

        #[command(subcommand)]
        sub: Option<StatsCommand>,
    },
    /// Show TCP/UDP ports
    Ports,
    /// Show cloud services & stats
    Cloud,
    /// Assign domain to server [auto, *.tojest.dev, *.bieda.it, *.toadres.pl, *.byst.re].
    Domain {
        /// Port number
        port: String,
        /// Domain name (omit to auto-assign)
        domain: Option<String>,
    },
    /// Show current configuration (profiles from ~/.mikrus, active credentials)
    Config,
    /// List configured servers and show the default one (`ctx switch` changes it)
    Ctx {
        #[command(subcommand)]
        sub: Option<CtxCommand>,
    },
    /// Connect to the server via SSH (uses `ssh` command from profile in ~/.mikrus)
    Ssh,
    /// Show mikr.us infrastructure status (https://status.mikr.us)
    Status {
        #[command(subcommand)]
        sub: Option<StatusCommand>,
    },
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
struct LogsArgs {
    /// Specific log ID
    id: Option<String>,

    #[command(subcommand)]
    sub: Option<LogsCommand>,
}

#[derive(Subcommand, Debug)]
enum LogsCommand {
    /// Show condensed one-line-per-entry log summary
    Short,
}

#[derive(Subcommand, Debug)]
enum StatsCommand {
    /// Shortcut for --truncate 100
    Short,
}

#[derive(Subcommand, Debug)]
enum CtxCommand {
    /// Switch the default server (omit NAME to pick one interactively)
    Switch {
        /// Server (profile) name to make default
        name: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum StatusCommand {
    /// Show only the user's own server status (one line per matched server)
    Short,
}

const ASCII_LOGO: &str = r#"
           _ _
          (_) |
 _ __ ___  _| | ___ __ _   _ ___
| '_ ` _ \| | |/ / '__| | | / __|
| | | | | | |   <| |  | |_| \__ \
|_| |_| |_|_|_|\_\_|   \__,_|___/
"#;

#[tokio::main]
async fn main() -> Result<()> {
    let loaded = config::load_all().unwrap_or_else(|e| {
        eprintln!("Warning: {e:#}");
        LoadedConfig::default()
    });
    let config = &loaded.merged;
    let default_profile = loaded.effective_default().map(|(name, _)| name);

    let raw_args: Vec<String> = std::env::args().collect();
    let (selected_profile, args) = config::extract_profile_arg(&raw_args, config);

    let mut cli = Cli::parse_from(args);

    let command = match cli.command.take() {
        Some(cmd) => cmd,
        None => {
            print!("{ASCII_LOGO}\n");
            println!("Welcome to mikrus\n");
            Cli::parse_from(["mikrus", "--help"]);
            unreachable!();
        }
    };

    if matches!(command, Command::Config) {
        print_config(&cli, config, selected_profile.as_deref(), default_profile);
        return Ok(());
    }

    if let Command::Ctx { sub } = &command {
        return match sub {
            Some(CtxCommand::Switch { name }) => run_ctx_switch(&loaded, name.as_deref()),
            None => {
                print_ctx(&loaded);
                Ok(())
            }
        };
    }

    if matches!(command, Command::Ssh) {
        return run_ssh(config, selected_profile.as_deref(), default_profile);
    }

    if let Command::Status { sub } = &command {
        let short = matches!(sub, Some(StatusCommand::Short));
        return run_status(&cli, config, selected_profile.as_deref(), short).await;
    }

    let (srv, key) = resolve_credentials(&cli, config, selected_profile.as_deref(), default_profile)?;

    let client = MikrusClient::new(srv, key);

    let logs_short = matches!(&command, Command::Logs(args) if args.sub.is_some());

    let truncate_width = match &command {
        Command::Stats { truncate, sub } => {
            if matches!(sub, Some(StatsCommand::Short)) {
                Some(100)
            } else {
                Some(*truncate)
            }
        }
        _ => None,
    };

    let command_name = match &command {
        Command::Info => "info",
        Command::Servers => "servers",
        Command::Restart => "restart",
        Command::Logs(_) => "logs",
        Command::Amfetamina => "amfetamina",
        Command::Db => "db",
        Command::Exec { .. } => "exec",
        Command::Stats { .. } => "stats",
        Command::Ports => "ports",
        Command::Cloud => "cloud",
        Command::Domain { .. } => "domain",
        Command::Config => unreachable!(),
        Command::Ctx { .. } => unreachable!(),
        Command::Ssh => unreachable!(),
        Command::Status { .. } => unreachable!(),
    };

    let result = match command {
        Command::Info => client.info().await,
        Command::Servers => client.servers().await,
        Command::Restart => client.restart().await,
        Command::Logs(args) => client.logs(args.id.as_deref()).await,
        Command::Amfetamina => client.amfetamina().await,
        Command::Db => client.db().await,
        Command::Exec { cmd } => client.exec(&cmd).await,
        Command::Stats { .. } => client.stats().await,
        Command::Ports => client.ports().await,
        Command::Cloud => client.cloud().await,
        Command::Domain { port, domain } => {
            let domain = domain.as_deref().unwrap_or("-");
            client.domain(&port, domain).await
        }
        Command::Config => unreachable!(),
        Command::Ctx { .. } => unreachable!(),
        Command::Ssh => unreachable!(),
        Command::Status { .. } => unreachable!(),
    };

    match result {
        Ok(value) => {
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else if let Some(trunc) = truncate_width {
                print!("{}", format::format_stats(&value, trunc));
            } else if command_name == "db" {
                print!("{}", format::format_db(&value));
            } else if logs_short {
                print!("{}", format::format_logs_short(&value));
            } else {
                print!("{}", format::format_value(&value, command_name));
            }
        }
        Err(e) => {
            eprintln!("Error: {e:#}");
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Resolve `(srv, key)` using priority:
/// 1. `--srv`/`--key` flags or `MIKRUS_SRV`/`MIKRUS_KEY` env vars (clap already merged these)
/// 2. Profile named as positional arg (e.g. `mikrus marek245 info`)
/// 3. Default profile (`default = true`, or the first one — see `mikrus ctx`)
fn resolve_credentials(
    cli: &Cli,
    config: &Config,
    selected_profile: Option<&str>,
    default_profile: Option<&str>,
) -> Result<(String, String)> {
    if let (Some(srv), Some(key)) = (cli.srv.clone(), cli.key.clone()) {
        return Ok((srv, key));
    }

    if let Some(name) = selected_profile {
        let profile = config.servers.get(name).ok_or_else(|| {
            anyhow::anyhow!("Profile '{name}' not found in config file")
        })?;
        let srv = cli.srv.clone().unwrap_or_else(|| profile.srv.clone());
        let key = cli.key.clone().unwrap_or_else(|| profile.key.clone());
        return Ok((srv, key));
    }

    if let Some(profile) = default_profile.and_then(|name| config.servers.get(name)) {
        let srv = cli.srv.clone().unwrap_or_else(|| profile.srv.clone());
        let key = cli.key.clone().unwrap_or_else(|| profile.key.clone());
        return Ok((srv, key));
    }

    if config.servers.is_empty() {
        if cli.srv.is_none() {
            anyhow::bail!("Server name is required. Use --srv, set MIKRUS_SRV, or configure ~/.mikrus");
        }
        if cli.key.is_none() {
            anyhow::bail!("API key is required. Use --key, set MIKRUS_KEY, or configure ~/.mikrus");
        }
        unreachable!();
    }

    let names: Vec<&str> = config.servers.keys().map(|s| s.as_str()).collect();
    anyhow::bail!(
        "Multiple profiles configured in ~/.mikrus ({}). Specify one: mikrus <profile> <command>",
        names.join(", ")
    );
}

fn resolve_profile<'a>(
    config: &'a Config,
    selected_profile: Option<&str>,
    default_profile: Option<&str>,
) -> Result<(&'a str, &'a Profile)> {
    if let Some(name) = selected_profile {
        let (key, profile) = config
            .servers
            .get_key_value(name)
            .ok_or_else(|| anyhow::anyhow!("Profile '{name}' not found in config file"))?;
        return Ok((key.as_str(), profile));
    }
    if let Some((name, profile)) = default_profile.and_then(|n| config.servers.get_key_value(n)) {
        return Ok((name.as_str(), profile));
    }
    if config.servers.is_empty() {
        anyhow::bail!("No profiles configured in ~/.mikrus");
    }
    let names: Vec<&str> = config.servers.keys().map(|s| s.as_str()).collect();
    anyhow::bail!(
        "Multiple profiles configured in ~/.mikrus ({}). Specify one: mikrus <profile> ssh",
        names.join(", ")
    );
}

fn run_ssh(
    config: &Config,
    selected_profile: Option<&str>,
    default_profile: Option<&str>,
) -> Result<()> {
    let (name, profile) = resolve_profile(config, selected_profile, default_profile)?;
    let ssh_cmd = profile.ssh.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "No 'ssh' command defined for profile '{name}' in ~/.mikrus. \
             Add e.g. ssh = \"ssh root@example.com -p 12345\" under [servers.{name}]."
        )
    })?;

    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(ssh_cmd)
        .status()
        .with_context(|| format!("Failed to execute ssh command: {ssh_cmd}"))?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

async fn run_status(
    cli: &Cli,
    config: &Config,
    selected_profile: Option<&str>,
    short: bool,
) -> Result<()> {
    let client = status::StatusClient::new();
    let value = client.fetch().await?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }

    let user_srvs = collect_user_srvs(cli, config, selected_profile);

    // Resolve each user srv (e.g. "srv12345") to its physical hosting server (e.g. "srv07")
    // by reading the H1 of the user's default subdomain page. Failures are non-fatal.
    let mut resolved: Vec<(String, Option<String>)> = Vec::new();
    for user_srv in &user_srvs {
        match client.resolve_hosting_server(user_srv).await {
            Ok(host) => resolved.push((user_srv.clone(), Some(host))),
            Err(e) => {
                eprintln!("Warning: could not resolve hosting server for {user_srv}: {e:#}");
                resolved.push((user_srv.clone(), None));
            }
        }
    }

    let mut highlight: Vec<String> = Vec::with_capacity(user_srvs.len() * 2);
    for (user_srv, host) in &resolved {
        highlight.push(user_srv.clone());
        if let Some(h) = host {
            highlight.push(h.clone());
        }
    }

    let colorize = std::io::IsTerminal::is_terminal(&std::io::stdout());

    if short {
        print!("{}", format::format_status_short(&value, &highlight, colorize));
        return Ok(());
    }

    for (user_srv, host) in &resolved {
        if let Some(h) = host {
            println!("Your server: {h}.mikr.us ({user_srv})");
        }
    }
    if resolved.iter().any(|(_, h)| h.is_some()) {
        println!();
    }

    print!("{}", format::format_status(&value, &highlight, colorize));
    Ok(())
}

/// Gather every srv name we should highlight on the status page:
/// the explicit `--srv` flag, the selected profile, or — if none was selected —
/// every profile in the config file.
fn collect_user_srvs(cli: &Cli, config: &Config, selected_profile: Option<&str>) -> Vec<String> {
    let mut srvs: Vec<String> = Vec::new();
    if let Some(srv) = &cli.srv {
        srvs.push(srv.clone());
    }
    match selected_profile {
        Some(name) => {
            if let Some(p) = config.servers.get(name) {
                srvs.push(p.srv.clone());
            }
        }
        None => {
            for p in config.servers.values() {
                srvs.push(p.srv.clone());
            }
        }
    }
    srvs
}

fn print_config(
    cli: &Cli,
    config: &Config,
    selected_profile: Option<&str>,
    default_profile: Option<&str>,
) {
    match config::config_path() {
        Some(p) => println!("Config file: {}", p.display()),
        None => println!("Config file: unknown (HOME not set)"),
    }
    if let Some(p) = config::local_config_path() {
        println!("Local config file: {} (overrides global profiles)", p.display());
    }

    if config.servers.is_empty() {
        println!("Profiles: (none)");
    } else {
        println!("Profiles:");
        for (name, profile) in &config.servers {
            let ssh = match &profile.ssh {
                Some(s) => format!(", ssh=\"{s}\""),
                None => String::new(),
            };
            let default = if default_profile == Some(name.as_str()) {
                " (default)"
            } else {
                ""
            };
            println!("  {name} -> srv={}{ssh}{default}", profile.srv);
        }
    }

    println!();
    if let Some(name) = selected_profile {
        println!("Selected profile: {name}");
    } else if let Some(name) = default_profile {
        println!("Default profile: {name} (change it with `mikrus ctx switch`)");
    }
    match &cli.srv {
        Some(srv) => println!("MIKRUS_SRV: {srv}"),
        None => println!("MIKRUS_SRV: not set"),
    }
    match &cli.key {
        Some(key) => println!("MIKRUS_KEY: {key}"),
        None => println!("MIKRUS_KEY: not set"),
    }
}

/// `mikrus ctx` — list configured servers (local config first, when present) and
/// point out which one is the default.
fn print_ctx(loaded: &LoadedConfig) {
    if loaded.merged.servers.is_empty() {
        println!("No servers configured.");
        match config::config_path() {
            Some(p) => println!(
                "Add a [servers.<name>] entry to {} — see `mikrus config`.",
                p.display()
            ),
            None => println!("Add a [servers.<name>] entry to ~/.mikrus — see `mikrus config`."),
        }
        return;
    }

    let default = loaded.effective_default();
    let default_name = default.map(|(name, _)| name);
    let width = loaded
        .merged
        .servers
        .keys()
        .map(|n| n.chars().count())
        .max()
        .unwrap_or(0);

    if let (Some(local), Some(path)) = (&loaded.local, &loaded.local_path) {
        println!(
            "Local config: {} (overrides the global config)",
            path.display()
        );
        for (name, profile) in &local.servers {
            let is_default = default_name == Some(name.as_str());
            println!("{}", format_ctx_row(name, profile, width, is_default, None));
        }
        println!();
    }

    match config::config_path() {
        Some(p) => println!("Global config: {}", p.display()),
        None => println!("Global config: unknown (HOME not set)"),
    }
    if loaded.global.servers.is_empty() {
        println!("    (no servers)");
    }
    for (name, profile) in &loaded.global.servers {
        let shadowed = loaded
            .local
            .as_ref()
            .is_some_and(|l| l.servers.contains_key(name));
        // A shadowed entry is never the active one — the local definition replaced it.
        let is_default = !shadowed && default_name == Some(name.as_str());
        let note = if shadowed {
            Some("overridden by local config")
        } else if profile.is_default() && !is_default {
            Some("default = true, overridden by local config")
        } else {
            None
        };
        println!("{}", format_ctx_row(name, profile, width, is_default, note));
    }

    println!();
    match default {
        Some((name, true)) => println!("Default server: {name} (marked with default = true)"),
        Some((name, false)) => println!(
            "Default server: {name} (no server has default = true, so the first one is used)"
        ),
        None => println!("Default server: (none)"),
    }
    println!("Switch it with: mikrus ctx switch [<name>]");
}

fn format_ctx_row(
    name: &str,
    profile: &Profile,
    width: usize,
    is_default: bool,
    note: Option<&str>,
) -> String {
    let marker = if is_default { "*" } else { " " };
    let mut row = format!("  {marker} {name:<width$}  {}", profile.srv);
    if is_default {
        row.push_str("  (default)");
    }
    if let Some(note) = note {
        row.push_str(&format!("  ({note})"));
    }
    row
}

/// `mikrus ctx switch [NAME]` — mark another server as the default one and persist it.
fn run_ctx_switch(loaded: &LoadedConfig, name: Option<&str>) -> Result<()> {
    let names: Vec<&str> = loaded.merged.servers.keys().map(|s| s.as_str()).collect();

    if names.is_empty() {
        anyhow::bail!("No servers configured — nothing to switch. Add a [servers.<name>] entry to ~/.mikrus.");
    }
    if names.len() == 1 {
        println!(
            "There's only one server configured ('{}'), so there's nothing to switch.",
            names[0]
        );
        return Ok(());
    }

    let current = loaded.effective_default().map(|(name, _)| name);

    let target = match name {
        Some(name) => {
            if !loaded.merged.servers.contains_key(name) {
                anyhow::bail!(
                    "Server '{name}' is not defined in the config. Available: {}",
                    names.join(", ")
                );
            }
            name.to_string()
        }
        None => match prompt_for_server(loaded, &names, current)? {
            Some(name) => name,
            None => {
                println!("Cancelled — the default server is unchanged.");
                return Ok(());
            }
        },
    };

    let path = loaded
        .defining_path(&target)
        .ok_or_else(|| anyhow::anyhow!("Cannot determine the config file for '{target}'"))?
        .to_path_buf();

    if !config::write_default_flag(&path, Some(&target))? {
        anyhow::bail!(
            "Server '{target}' is not defined in {} — config file changed in the meantime?",
            path.display()
        );
    }

    let local_path = loaded.local_path.as_deref();
    let wrote_local = local_path == Some(path.as_path());

    // Marked the global file while the local one still marks another server: the local
    // marker wins here, so clear it — otherwise the switch would have no visible effect.
    let mut cleared_local = None;
    if !wrote_local {
        if let (Some(local), Some(local_path)) = (&loaded.local, local_path) {
            if local.explicit_default().is_some() {
                config::write_default_flag(local_path, None)?;
                cleared_local = Some(local_path.to_path_buf());
            }
        }
    }

    let srv = &loaded.merged.servers[&target].srv;
    println!("Default server switched to '{target}' ({srv}).");
    println!("Saved in {}", path.display());
    if wrote_local {
        println!("This is a project-local config — the global default is unchanged.");
    }
    if let Some(cleared) = cleared_local {
        println!(
            "Removed the default marker from {} so this one applies here.",
            cleared.display()
        );
    }
    Ok(())
}

/// Interactive picker for `mikrus ctx switch` without an explicit name.
/// Returns `None` when the user cancels with an empty line.
fn prompt_for_server(
    loaded: &LoadedConfig,
    names: &[&str],
    current: Option<&str>,
) -> Result<Option<String>> {
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        anyhow::bail!(
            "Not running interactively. Pass the name: mikrus ctx switch <{}>",
            names.join("|")
        );
    }

    let width = names.iter().map(|n| n.chars().count()).max().unwrap_or(0);
    println!("Available servers:");
    for (i, name) in names.iter().enumerate() {
        let srv = &loaded.merged.servers[*name].srv;
        let note = if current == Some(*name) {
            "  (current default)"
        } else {
            ""
        };
        println!("  {}) {name:<width$}  {srv}{note}", i + 1);
    }
    print!("Select server [1-{}] (empty to cancel): ", names.len());
    std::io::Write::flush(&mut std::io::stdout()).ok();

    let mut line = String::new();
    std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line)
        .context("Failed to read from stdin")?;
    let input = line.trim();

    if input.is_empty() {
        return Ok(None);
    }
    if let Ok(index) = input.parse::<usize>() {
        let name = names
            .get(index.wrapping_sub(1))
            .ok_or_else(|| anyhow::anyhow!("Invalid selection: {index}"))?;
        return Ok(Some((*name).to_string()));
    }
    if names.contains(&input) {
        return Ok(Some(input.to_string()));
    }
    anyhow::bail!(
        "Invalid selection: '{input}'. Pick a number 1-{} or one of: {}",
        names.len(),
        names.join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_parse_info_command() {
        let cli = Cli::parse_from(["mikrus", "--srv", "srv12345", "--key", "mykey", "info"]);
        assert_eq!(cli.srv.unwrap(), "srv12345");
        assert_eq!(cli.key.unwrap(), "mykey");
        assert!(matches!(cli.command, Some(Command::Info)));
    }

    #[test]
    fn test_parse_exec_command() {
        let cli = Cli::parse_from([
            "mikrus", "--srv", "srv12345", "--key", "mykey", "exec", "uptime",
        ]);
        match cli.command {
            Some(Command::Exec { cmd }) => assert_eq!(cmd, "uptime"),
            _ => panic!("expected Exec command"),
        }
    }

    #[test]
    fn test_parse_domain_command() {
        let cli = Cli::parse_from([
            "mikrus", "--srv", "srv12345", "--key", "mykey", "domain", "8080", "example.com",
        ]);
        match cli.command {
            Some(Command::Domain { port, domain }) => {
                assert_eq!(port, "8080");
                assert_eq!(domain.unwrap(), "example.com");
            }
            _ => panic!("expected Domain command"),
        }
    }

    #[test]
    fn test_parse_domain_command_without_domain() {
        let cli = Cli::parse_from([
            "mikrus", "--srv", "srv12345", "--key", "mykey", "domain", "8080",
        ]);
        match cli.command {
            Some(Command::Domain { port, domain }) => {
                assert_eq!(port, "8080");
                assert!(domain.is_none());
            }
            _ => panic!("expected Domain command"),
        }
    }

    #[test]
    fn test_parse_logs_with_id() {
        let cli = Cli::parse_from([
            "mikrus", "--srv", "srv12345", "--key", "mykey", "logs", "42",
        ]);
        match cli.command {
            Some(Command::Logs(args)) => {
                assert_eq!(args.id.unwrap(), "42");
                assert!(args.sub.is_none());
            }
            _ => panic!("expected Logs command"),
        }
    }

    #[test]
    fn test_parse_logs_without_id() {
        let cli = Cli::parse_from(["mikrus", "--srv", "srv12345", "--key", "mykey", "logs"]);
        match cli.command {
            Some(Command::Logs(args)) => {
                assert!(args.id.is_none());
                assert!(args.sub.is_none());
            }
            _ => panic!("expected Logs command"),
        }
    }

    #[test]
    fn test_parse_logs_short() {
        let cli = Cli::parse_from([
            "mikrus", "--srv", "srv12345", "--key", "mykey", "logs", "short",
        ]);
        match cli.command {
            Some(Command::Logs(args)) => {
                assert!(args.id.is_none());
                assert!(matches!(args.sub, Some(LogsCommand::Short)));
            }
            _ => panic!("expected Logs command"),
        }
    }

    #[test]
    fn test_parse_stats_default_truncate() {
        let cli = Cli::parse_from(["mikrus", "--srv", "srv12345", "--key", "mykey", "stats"]);
        match cli.command {
            Some(Command::Stats { truncate, sub }) => {
                assert_eq!(truncate, 0);
                assert!(sub.is_none());
            }
            _ => panic!("expected Stats command"),
        }
    }

    #[test]
    fn test_parse_stats_custom_truncate() {
        let cli = Cli::parse_from([
            "mikrus", "--srv", "srv12345", "--key", "mykey", "stats", "--truncate", "120",
        ]);
        match cli.command {
            Some(Command::Stats { truncate, sub }) => {
                assert_eq!(truncate, 120);
                assert!(sub.is_none());
            }
            _ => panic!("expected Stats command"),
        }
    }

    #[test]
    fn test_parse_stats_short() {
        let cli = Cli::parse_from([
            "mikrus", "--srv", "srv12345", "--key", "mykey", "stats", "short",
        ]);
        match cli.command {
            Some(Command::Stats { truncate: _, sub }) => {
                assert!(matches!(sub, Some(StatsCommand::Short)));
            }
            _ => panic!("expected Stats command"),
        }
    }

    #[test]
    fn test_missing_subcommand() {
        let cli = Cli::parse_from(["mikrus", "--srv", "srv12345", "--key", "mykey"]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_parse_json_flag() {
        let cli = Cli::parse_from([
            "mikrus", "--srv", "srv12345", "--key", "mykey", "--json", "info",
        ]);
        assert!(cli.json);
        assert!(matches!(cli.command, Some(Command::Info)));
    }

    #[test]
    fn test_parse_json_flag_default_false() {
        let cli = Cli::parse_from(["mikrus", "--srv", "srv12345", "--key", "mykey", "info"]);
        assert!(!cli.json);
    }

    #[test]
    fn test_parse_json_flag_after_subcommand() {
        let cli = Cli::parse_from([
            "mikrus", "--srv", "srv12345", "--key", "mykey", "info", "--json",
        ]);
        assert!(cli.json);
    }

    fn make_config(profiles: &[(&str, &str, &str)]) -> Config {
        let mut servers = std::collections::BTreeMap::new();
        for (name, srv, key) in profiles {
            servers.insert(
                name.to_string(),
                config::Profile {
                    srv: srv.to_string(),
                    key: key.to_string(),
                    ssh: None,
                    default: None,
                },
            );
        }
        Config { servers }
    }

    fn cli_with(srv: Option<&str>, key: Option<&str>) -> Cli {
        Cli {
            srv: srv.map(String::from),
            key: key.map(String::from),
            json: false,
            command: None,
        }
    }

    #[test]
    fn resolve_uses_flags_first() {
        let cli = cli_with(Some("srvA"), Some("keyA"));
        let cfg = make_config(&[("marek245", "srvB", "keyB")]);
        let (srv, key) = resolve_credentials(&cli, &cfg, Some("marek245"), None).unwrap();
        assert_eq!(srv, "srvA");
        assert_eq!(key, "keyA");
    }

    #[test]
    fn resolve_uses_named_profile() {
        let cli = cli_with(None, None);
        let cfg = make_config(&[("marek245", "srvB", "keyB"), ("prod", "srvC", "keyC")]);
        let (srv, key) = resolve_credentials(&cli, &cfg, Some("prod"), Some("marek245")).unwrap();
        assert_eq!(srv, "srvC");
        assert_eq!(key, "keyC");
    }

    #[test]
    fn resolve_auto_selects_single_profile() {
        let cli = cli_with(None, None);
        let cfg = make_config(&[("only", "srvX", "keyX")]);
        let default = cfg.effective_default().map(|(n, _)| n.to_string());
        let (srv, key) = resolve_credentials(&cli, &cfg, None, default.as_deref()).unwrap();
        assert_eq!(srv, "srvX");
        assert_eq!(key, "keyX");
    }

    #[test]
    fn resolve_uses_default_profile_when_none_selected() {
        let cli = cli_with(None, None);
        let cfg = make_config(&[("marek245", "srvB", "keyB"), ("prod", "srvC", "keyC")]);
        let (srv, key) = resolve_credentials(&cli, &cfg, None, Some("prod")).unwrap();
        assert_eq!(srv, "srvC");
        assert_eq!(key, "keyC");
    }

    #[test]
    fn resolve_errors_when_multiple_profiles_and_no_default() {
        let cli = cli_with(None, None);
        let cfg = make_config(&[("marek245", "srvB", "keyB"), ("prod", "srvC", "keyC")]);
        let err = resolve_credentials(&cli, &cfg, None, None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("marek245"), "error should list profiles: {msg}");
        assert!(msg.contains("prod"));
    }

    #[test]
    fn resolve_errors_when_no_profile_and_no_flags() {
        let cli = cli_with(None, None);
        let cfg = Config::default();
        let err = resolve_credentials(&cli, &cfg, None, None).unwrap_err();
        assert!(err.to_string().contains("Server name is required"));
    }

    #[test]
    fn resolve_errors_for_unknown_named_profile() {
        let cli = cli_with(None, None);
        let cfg = make_config(&[("marek245", "srvB", "keyB")]);
        let err = resolve_credentials(&cli, &cfg, Some("ghost"), None).unwrap_err();
        assert!(err.to_string().contains("ghost"));
    }

    #[test]
    fn test_parse_ssh_command() {
        let cli = Cli::parse_from(["mikrus", "ssh"]);
        assert!(matches!(cli.command, Some(Command::Ssh)));
    }

    #[test]
    fn test_parse_status_command() {
        let cli = Cli::parse_from(["mikrus", "status"]);
        assert!(matches!(cli.command, Some(Command::Status { sub: None })));
    }

    #[test]
    fn test_parse_status_short_command() {
        let cli = Cli::parse_from(["mikrus", "status", "short"]);
        match cli.command {
            Some(Command::Status { sub }) => {
                assert!(matches!(sub, Some(StatusCommand::Short)));
            }
            _ => panic!("expected Status command with short"),
        }
    }

    #[test]
    fn collect_user_srvs_uses_flag_when_set() {
        let cli = cli_with(Some("srv99"), None);
        let cfg = make_config(&[("p", "srv01", "k")]);
        let srvs = collect_user_srvs(&cli, &cfg, Some("p"));
        assert!(srvs.contains(&"srv99".to_string()));
        assert!(srvs.contains(&"srv01".to_string()));
    }

    #[test]
    fn collect_user_srvs_named_profile_only() {
        let cli = cli_with(None, None);
        let cfg = make_config(&[("p1", "srv01", "k"), ("p2", "srv02", "k")]);
        let srvs = collect_user_srvs(&cli, &cfg, Some("p2"));
        assert_eq!(srvs, vec!["srv02".to_string()]);
    }

    #[test]
    fn collect_user_srvs_all_profiles_when_none_selected() {
        let cli = cli_with(None, None);
        let cfg = make_config(&[("p1", "srv01", "k"), ("p2", "srv02", "k")]);
        let srvs = collect_user_srvs(&cli, &cfg, None);
        assert!(srvs.contains(&"srv01".to_string()));
        assert!(srvs.contains(&"srv02".to_string()));
    }

    #[test]
    fn resolve_profile_uses_named() {
        let cfg = make_config(&[("marek245", "srvB", "keyB"), ("prod", "srvC", "keyC")]);
        let (name, profile) = resolve_profile(&cfg, Some("prod"), Some("marek245")).unwrap();
        assert_eq!(name, "prod");
        assert_eq!(profile.srv, "srvC");
    }

    #[test]
    fn resolve_profile_auto_selects_single() {
        let cfg = make_config(&[("only", "srvX", "keyX")]);
        let default = cfg.effective_default().map(|(n, _)| n.to_string());
        let (name, _) = resolve_profile(&cfg, None, default.as_deref()).unwrap();
        assert_eq!(name, "only");
    }

    #[test]
    fn resolve_profile_uses_default_when_none_selected() {
        let cfg = make_config(&[("marek245", "srvB", "keyB"), ("prod", "srvC", "keyC")]);
        let (name, profile) = resolve_profile(&cfg, None, Some("prod")).unwrap();
        assert_eq!(name, "prod");
        assert_eq!(profile.srv, "srvC");
    }

    #[test]
    fn resolve_profile_errors_when_multiple_and_no_default() {
        let cfg = make_config(&[("marek245", "srvB", "keyB"), ("prod", "srvC", "keyC")]);
        let err = resolve_profile(&cfg, None, None).unwrap_err();
        assert!(err.to_string().contains("Multiple profiles"));
    }

    #[test]
    fn resolve_profile_errors_when_empty() {
        let cfg = Config::default();
        let err = resolve_profile(&cfg, None, None).unwrap_err();
        assert!(err.to_string().contains("No profiles"));
    }

    #[test]
    fn test_parse_ctx_command() {
        let cli = Cli::parse_from(["mikrus", "ctx"]);
        assert!(matches!(cli.command, Some(Command::Ctx { sub: None })));
    }

    #[test]
    fn test_parse_ctx_switch_without_name() {
        let cli = Cli::parse_from(["mikrus", "ctx", "switch"]);
        match cli.command {
            Some(Command::Ctx { sub }) => {
                assert!(matches!(sub, Some(CtxCommand::Switch { name: None })));
            }
            _ => panic!("expected Ctx command"),
        }
    }

    #[test]
    fn test_parse_ctx_switch_with_name() {
        let cli = Cli::parse_from(["mikrus", "ctx", "switch", "prod"]);
        match cli.command {
            Some(Command::Ctx {
                sub: Some(CtxCommand::Switch { name }),
            }) => assert_eq!(name.unwrap(), "prod"),
            _ => panic!("expected Ctx switch command"),
        }
    }

    #[test]
    fn ctx_row_marks_the_default_server() {
        let cfg = make_config(&[("prod", "srv67890", "k")]);
        let row = format_ctx_row("prod", &cfg.servers["prod"], 8, true, None);
        assert!(row.starts_with("  * prod"), "row: {row}");
        assert!(row.contains("srv67890"));
        assert!(row.contains("(default)"));
    }

    #[test]
    fn ctx_row_renders_note_for_non_default_server() {
        let cfg = make_config(&[("prod", "srv67890", "k")]);
        let row = format_ctx_row("prod", &cfg.servers["prod"], 8, false, Some("overridden"));
        assert!(row.starts_with("    prod"), "row: {row}");
        assert!(!row.contains("(default)"));
        assert!(row.contains("(overridden)"));
    }
}
