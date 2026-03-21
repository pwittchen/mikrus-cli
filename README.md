# mikrus-cli [![Rust](https://github.com/pwittchen/mikrus-cli/actions/workflows/rust.yml/badge.svg?branch=master)](https://github.com/pwittchen/mikrus-cli/actions/workflows/rust.yml)

[mikrus](https://mikr.us/) VPS CLI written in Rust

## Installation

### Prerequisites

- **Rust toolchain** (1.85+) — install via [rustup](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

### From source

```bash
git clone https://github.com/pwittchen/mikrus-cli.git
cd mikrus-cli
cargo install --path .
```

### From GitHub directly

```bash
cargo install --git https://github.com/pwittchen/mikrus-cli.git
```

The binary will be installed to `~/.cargo/bin/mikrus`.

### Uninstallation

```bash
cargo uninstall mikrus-cli
```

## Configuration

Set your credentials via environment variables or pass them as flags:

```bash
export MIKRUS_SRV=srv12345
export MIKRUS_KEY=your-api-key
```

Or use `--srv` and `--key` flags with each command.

## Usage

```bash
mikrus [--srv <SRV>] [--key <KEY>] [--json] <COMMAND>
```

Use `--json` to output raw JSON instead of formatted text.

## Commands

| Command | Description |
|---------|-------------|
| `info` | Show server information |
| `servers` | List all user servers |
| `restart` | Restart the server |
| `logs [ID]` | Show log entries (optional: specific log ID) |
| `amfetamina` | Performance boost |
| `db` | Show database credentials |
| `exec <CMD>` | Execute a command on the server |
| `stats [--truncate <WIDTH>] [short]` | Show disk/memory/uptime statistics (truncate long lines at WIDTH, adding "..."; 0 = no truncation; `short` is a shortcut for `--truncate 100`) |
| `ports` | Show TCP/UDP ports |
| `cloud` | Show cloud services & stats |
| `domain <PORT> [DOMAIN]` | Assign domain to server (omit domain for auto-assignment; available: `*.tojest.dev`, `*.bieda.it`, `*.toadres.pl`, `*.byst.re`) |
| `config` | Show current configuration (MIKRUS_SRV and MIKRUS_KEY) |

## Building

```bash
cargo build --verbose
cargo test --verbose
cargo run
```

## API docs

https://api.mikr.us/
