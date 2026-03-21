mod api;
mod format;

use anyhow::Result;
use clap::{Parser, Subcommand};

use api::MikrusClient;

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
    Logs {
        /// Specific log ID
        id: Option<String>,
    },
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
    /// Assign domain to server (available subdomains: *.tojest.dev, *.bieda.it, *.toadres.pl, *.byst.re)
    Domain {
        /// Port number
        port: String,
        /// Domain name
        domain: String,
    },
    /// Show current configuration (MIKRUS_SRV and MIKRUS_KEY)
    Config,
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
    let cli = Cli::parse();

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            print!("{ASCII_LOGO}\n");
            println!("Welcome to mikrus\n");
            Cli::parse_from(["mikrus", "--help"]);
            unreachable!();
        }
    };

    if matches!(command, Command::Config) {
        match &cli.srv {
            Some(srv) => println!("MIKRUS_SRV: {srv}"),
            None => println!("MIKRUS_SRV: not set"),
        }
        match &cli.key {
            Some(key) => println!("MIKRUS_KEY: {key}"),
            None => println!("MIKRUS_KEY: not set"),
        }
        return Ok(());
    }

    let srv = cli
        .srv
        .ok_or_else(|| anyhow::anyhow!("Server name is required. Use --srv or set MIKRUS_SRV"))?;
    let key = cli
        .key
        .ok_or_else(|| anyhow::anyhow!("API key is required. Use --key or set MIKRUS_KEY"))?;

    let client = MikrusClient::new(srv, key);

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
        Command::Logs { .. } => "logs",
        Command::Amfetamina => "amfetamina",
        Command::Db => "db",
        Command::Exec { .. } => "exec",
        Command::Stats { .. } => "stats",
        Command::Ports => "ports",
        Command::Cloud => "cloud",
        Command::Domain { .. } => "domain",
        Command::Config => unreachable!(),
    };

    let result = match command {
        Command::Info => client.info().await,
        Command::Servers => client.servers().await,
        Command::Restart => client.restart().await,
        Command::Logs { id } => client.logs(id.as_deref()).await,
        Command::Amfetamina => client.amfetamina().await,
        Command::Db => client.db().await,
        Command::Exec { cmd } => client.exec(&cmd).await,
        Command::Stats { .. } => client.stats().await,
        Command::Ports => client.ports().await,
        Command::Cloud => client.cloud().await,
        Command::Domain { port, domain } => client.domain(&port, &domain).await,
        Command::Config => unreachable!(),
    };

    match result {
        Ok(value) => {
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else if let Some(trunc) = truncate_width {
                print!("{}", format::format_stats(&value, trunc));
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
                assert_eq!(domain, "example.com");
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
            Some(Command::Logs { id }) => assert_eq!(id.unwrap(), "42"),
            _ => panic!("expected Logs command"),
        }
    }

    #[test]
    fn test_parse_logs_without_id() {
        let cli = Cli::parse_from(["mikrus", "--srv", "srv12345", "--key", "mykey", "logs"]);
        match cli.command {
            Some(Command::Logs { id }) => assert!(id.is_none()),
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
}
