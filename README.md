# mikrus-cli [![Rust](https://github.com/pwittchen/mikrus-cli/actions/workflows/rust.yml/badge.svg?branch=master)](https://github.com/pwittchen/mikrus-cli/actions/workflows/rust.yml)

[mikrus](https://mikr.us/) VPS CLI written in Rust

## Configuration

Set your credentials via environment variables or pass them as flags:

```bash
export MIKRUS_SRV=srv12345
export MIKRUS_KEY=your-api-key
```

Or use `--srv` and `--key` flags with each command.

## Usage

```bash
mikrus-cli [--srv <SRV>] [--key <KEY>] <COMMAND>
```

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
| `stats` | Show disk/memory/uptime statistics |
| `ports` | Show TCP/UDP ports |
| `cloud` | Show cloud services & stats |
| `domain <PORT> <DOMAIN>` | Assign domain to server |

## Building

```bash
cargo build --verbose
cargo test --verbose
cargo run
```

## API docs

https://api.mikr.us/
