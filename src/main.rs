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

    #[command(subcommand)]
    command: Command,
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
    },
    /// Show TCP/UDP ports
    Ports,
    /// Show cloud services & stats
    Cloud,
    /// Assign domain to server
    Domain {
        /// Port number
        port: String,
        /// Domain name
        domain: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let srv = cli
        .srv
        .ok_or_else(|| anyhow::anyhow!("Server name is required. Use --srv or set MIKRUS_SRV"))?;
    let key = cli
        .key
        .ok_or_else(|| anyhow::anyhow!("API key is required. Use --key or set MIKRUS_KEY"))?;

    let client = MikrusClient::new(srv, key);

    let truncate_width = match &cli.command {
        Command::Stats { truncate } => Some(*truncate),
        _ => None,
    };

    let result = match cli.command {
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
    };

    match result {
        Ok(value) => {
            if let Some(trunc) = truncate_width {
                print!("{}", format::format_stats(&value, trunc));
            } else {
                println!("{}", serde_json::to_string_pretty(&value)?);
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
        assert!(matches!(cli.command, Command::Info));
    }

    #[test]
    fn test_parse_exec_command() {
        let cli = Cli::parse_from([
            "mikrus", "--srv", "srv12345", "--key", "mykey", "exec", "uptime",
        ]);
        match cli.command {
            Command::Exec { cmd } => assert_eq!(cmd, "uptime"),
            _ => panic!("expected Exec command"),
        }
    }

    #[test]
    fn test_parse_domain_command() {
        let cli = Cli::parse_from([
            "mikrus", "--srv", "srv12345", "--key", "mykey", "domain", "8080", "example.com",
        ]);
        match cli.command {
            Command::Domain { port, domain } => {
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
            Command::Logs { id } => assert_eq!(id.unwrap(), "42"),
            _ => panic!("expected Logs command"),
        }
    }

    #[test]
    fn test_parse_logs_without_id() {
        let cli = Cli::parse_from(["mikrus", "--srv", "srv12345", "--key", "mykey", "logs"]);
        match cli.command {
            Command::Logs { id } => assert!(id.is_none()),
            _ => panic!("expected Logs command"),
        }
    }

    #[test]
    fn test_parse_stats_default_truncate() {
        let cli = Cli::parse_from(["mikrus", "--srv", "srv12345", "--key", "mykey", "stats"]);
        match cli.command {
            Command::Stats { truncate } => assert_eq!(truncate, 0),
            _ => panic!("expected Stats command"),
        }
    }

    #[test]
    fn test_parse_stats_custom_truncate() {
        let cli = Cli::parse_from([
            "mikrus", "--srv", "srv12345", "--key", "mykey", "stats", "--truncate", "120",
        ]);
        match cli.command {
            Command::Stats { truncate } => assert_eq!(truncate, 120),
            _ => panic!("expected Stats command"),
        }
    }

    #[test]
    fn test_missing_subcommand() {
        let result = Cli::try_parse_from(["mikrus", "--srv", "srv12345", "--key", "mykey"]);
        assert!(result.is_err());
    }
}
