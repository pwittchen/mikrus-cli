mod api;

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
    Stats,
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

    let result = match cli.command {
        Command::Info => client.info().await,
        Command::Servers => client.servers().await,
        Command::Restart => client.restart().await,
        Command::Logs { id } => client.logs(id.as_deref()).await,
        Command::Amfetamina => client.amfetamina().await,
        Command::Db => client.db().await,
        Command::Exec { cmd } => client.exec(&cmd).await,
        Command::Stats => client.stats().await,
        Command::Ports => client.ports().await,
        Command::Cloud => client.cloud().await,
        Command::Domain { port, domain } => client.domain(&port, &domain).await,
    };

    match result {
        Ok(value) => {
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Err(e) => {
            eprintln!("Error: {e:#}");
            std::process::exit(1);
        }
    }

    Ok(())
}
