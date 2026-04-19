mod api;
mod config;
mod format;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use api::MikrusClient;
use config::{Config, Profile};

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
    /// Connect to the server via SSH (uses `ssh` command from profile in ~/.mikrus)
    Ssh,
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
    let config = config::load().unwrap_or_else(|e| {
        eprintln!("Warning: {e:#}");
        Config::default()
    });

    let raw_args: Vec<String> = std::env::args().collect();
    let (selected_profile, args) = config::extract_profile_arg(&raw_args, &config);

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
        print_config(&cli, &config, selected_profile.as_deref());
        return Ok(());
    }

    if matches!(command, Command::Ssh) {
        return run_ssh(&config, selected_profile.as_deref());
    }

    let (srv, key) = resolve_credentials(&cli, &config, selected_profile.as_deref())?;

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
        Command::Ssh => unreachable!(),
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
        Command::Ssh => unreachable!(),
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
/// 3. Config file has exactly one profile → auto-select it
fn resolve_credentials(
    cli: &Cli,
    config: &Config,
    selected_profile: Option<&str>,
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

    if config.servers.len() == 1 {
        let (_, profile) = config.servers.iter().next().unwrap();
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
) -> Result<(&'a str, &'a Profile)> {
    if let Some(name) = selected_profile {
        let (key, profile) = config
            .servers
            .get_key_value(name)
            .ok_or_else(|| anyhow::anyhow!("Profile '{name}' not found in config file"))?;
        return Ok((key.as_str(), profile));
    }
    if config.servers.len() == 1 {
        let (name, profile) = config.servers.iter().next().unwrap();
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

fn run_ssh(config: &Config, selected_profile: Option<&str>) -> Result<()> {
    let (name, profile) = resolve_profile(config, selected_profile)?;
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

fn print_config(cli: &Cli, config: &Config, selected_profile: Option<&str>) {
    match config::config_path() {
        Some(p) => println!("Config file: {}", p.display()),
        None => println!("Config file: unknown (HOME not set)"),
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
            println!("  {name} -> srv={}{ssh}", profile.srv);
        }
    }

    println!();
    if let Some(name) = selected_profile {
        println!("Selected profile: {name}");
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
        let (srv, key) = resolve_credentials(&cli, &cfg, Some("marek245")).unwrap();
        assert_eq!(srv, "srvA");
        assert_eq!(key, "keyA");
    }

    #[test]
    fn resolve_uses_named_profile() {
        let cli = cli_with(None, None);
        let cfg = make_config(&[("marek245", "srvB", "keyB"), ("prod", "srvC", "keyC")]);
        let (srv, key) = resolve_credentials(&cli, &cfg, Some("prod")).unwrap();
        assert_eq!(srv, "srvC");
        assert_eq!(key, "keyC");
    }

    #[test]
    fn resolve_auto_selects_single_profile() {
        let cli = cli_with(None, None);
        let cfg = make_config(&[("only", "srvX", "keyX")]);
        let (srv, key) = resolve_credentials(&cli, &cfg, None).unwrap();
        assert_eq!(srv, "srvX");
        assert_eq!(key, "keyX");
    }

    #[test]
    fn resolve_errors_when_multiple_profiles_and_none_selected() {
        let cli = cli_with(None, None);
        let cfg = make_config(&[("marek245", "srvB", "keyB"), ("prod", "srvC", "keyC")]);
        let err = resolve_credentials(&cli, &cfg, None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("marek245"), "error should list profiles: {msg}");
        assert!(msg.contains("prod"));
    }

    #[test]
    fn resolve_errors_when_no_profile_and_no_flags() {
        let cli = cli_with(None, None);
        let cfg = Config::default();
        let err = resolve_credentials(&cli, &cfg, None).unwrap_err();
        assert!(err.to_string().contains("Server name is required"));
    }

    #[test]
    fn resolve_errors_for_unknown_named_profile() {
        let cli = cli_with(None, None);
        let cfg = make_config(&[("marek245", "srvB", "keyB")]);
        let err = resolve_credentials(&cli, &cfg, Some("ghost")).unwrap_err();
        assert!(err.to_string().contains("ghost"));
    }

    #[test]
    fn test_parse_ssh_command() {
        let cli = Cli::parse_from(["mikrus", "ssh"]);
        assert!(matches!(cli.command, Some(Command::Ssh)));
    }

    #[test]
    fn resolve_profile_uses_named() {
        let cfg = make_config(&[("marek245", "srvB", "keyB"), ("prod", "srvC", "keyC")]);
        let (name, profile) = resolve_profile(&cfg, Some("prod")).unwrap();
        assert_eq!(name, "prod");
        assert_eq!(profile.srv, "srvC");
    }

    #[test]
    fn resolve_profile_auto_selects_single() {
        let cfg = make_config(&[("only", "srvX", "keyX")]);
        let (name, _) = resolve_profile(&cfg, None).unwrap();
        assert_eq!(name, "only");
    }

    #[test]
    fn resolve_profile_errors_when_multiple_and_none_selected() {
        let cfg = make_config(&[("marek245", "srvB", "keyB"), ("prod", "srvC", "keyC")]);
        let err = resolve_profile(&cfg, None).unwrap_err();
        assert!(err.to_string().contains("Multiple profiles"));
    }

    #[test]
    fn resolve_profile_errors_when_empty() {
        let cfg = Config::default();
        let err = resolve_profile(&cfg, None).unwrap_err();
        assert!(err.to_string().contains("No profiles"));
    }
}
