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

Credentials can come from any of three sources (highest priority first):

1. **CLI flags / env vars** — `--srv`/`--key` or `MIKRUS_SRV`/`MIKRUS_KEY`
2. **Named profile** from `~/.mikrus` — passed as the first argument (e.g. `mikrus marek245 info`)
3. **Auto-selected profile** — when `~/.mikrus` contains exactly one profile

### Env vars / flags

```bash
export MIKRUS_SRV=srv12345
export MIKRUS_KEY=your-api-key
```

Or pass `--srv`/`--key` with each command.

### Config file (multiple servers)

Create `~/.mikrus` in TOML format:

```toml
[servers.marek245]
srv = "srv12345"
key = "your-api-key"
ssh = "ssh root@srv12345.mikr.us -p 12345"  # optional, enables `mikrus ssh`

[servers.prod]
srv = "srv67890"
key = "another-api-key"
```

The `ssh` field is optional. When present, `mikrus ssh` (or
`mikrus <profile> ssh`) runs that command via the system shell, so any flags,
ports, or identity files you put in the string are honored.

If only one profile is defined, commands run against it automatically. With
multiple profiles, pass the profile name as the first argument:

```bash
mikrus marek245 info
mikrus prod stats short
```

Run `mikrus config` to see the config file path, configured profiles, and
currently active credentials.

## Usage

```bash
mikrus [PROFILE] [--srv <SRV>] [--key <KEY>] [--json] <COMMAND>
```

Use `--json` to output raw JSON instead of formatted text.

## Commands

| Command | Description |
|---------|-------------|
| `info` | Show server information |
| `servers` | List all user servers |
| `restart` | Restart the server |
| `logs [ID]` | Show log entries (optional: specific log ID) |
| `logs short` | Show condensed one-line-per-entry log summary (max 100 chars, aligned columns) |
| `amfetamina` | Performance boost |
| `db` | Show database credentials |
| `exec <CMD>` | Execute a command on the server |
| `stats [--truncate <WIDTH>] [short]` | Show disk/memory/uptime statistics (truncate long lines at WIDTH, adding "..."; 0 = no truncation; `short` is a shortcut for `--truncate 100`) |
| `ports` | Show TCP/UDP ports |
| `cloud` | Show cloud services & stats |
| `domain <PORT> [DOMAIN]` | Assign domain to server (omit domain for auto-assignment; available: `*.tojest.dev`, `*.bieda.it`, `*.toadres.pl`, `*.byst.re`) |
| `config` | Show config file path, configured profiles, and active credentials |
| `ssh` | Connect to the server via SSH (uses optional `ssh` field from profile in `~/.mikrus`) |
| `status` | Show mikr.us infrastructure status from [status.mikr.us](https://status.mikr.us/status/mikrus) — colored dots per monitor (green=up, red=down, yellow=pending, blue=maintenance, gray=unknown). Your hosting server is auto-detected by reading the `<h1>` of `<srv>.mikrus.xyz` (e.g. `srv30.mikr.us`); a `Your server: …` header is printed and the matching monitor is marked with `→` |
| `status short` | Print one line per matched user server (e.g. `● srv30  up`) — skips the full grid |

## Building

```bash
cargo build --verbose
cargo test --verbose
cargo run
```

## Claude Code skill

This repo ships with a [Claude Code](https://claude.com/claude-code) skill at
[`.claude/skills/mikrus/SKILL.md`](.claude/skills/mikrus/SKILL.md). When you
open the repo in Claude Code, the skill teaches the assistant about the
`mikrus` commands, credential resolution, and profile handling, so you can ask
things like "restart my VPS" or "show mikrus stats short" and Claude will use
the CLI correctly.

### Examples

```
/mikrus show my server stats in the ascii table
/mikrus show my server stats in the ascii table and draw some cool ascii charts, but not too many
/mikrus what eats the most memory on my server?
/mikrus show me the logs of the varun.surf app running in the docker container
/mikrus what apps are tunneled via cloudflared?
/mikrus what is hosted on nginx?
```

## API docs

https://api.mikr.us/
