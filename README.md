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
2. **Named profile** from the config file — passed as the first argument (e.g. `mikrus marek245 info`)
3. **Auto-selected profile** — when the config file contains exactly one profile

Profiles are read from `~/.mikrus` and, if present, from `.mikrus` in the
current directory, which overrides the global one — see
[Local config file](#local-config-file-per-project).

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

Run `mikrus config` to see the config file paths, configured profiles, and
currently active credentials.

### Local config file (per project)

If a `.mikrus` file exists in the current working directory, it is loaded on top
of the global `~/.mikrus`. Profiles are merged by name:

- a profile defined locally **overrides** the global profile with the same name
  (the whole entry is replaced — `srv`, `key` and `ssh`),
- profiles that exist only globally are still available,
- profiles that exist only locally are added.

This lets you keep shared credentials in `~/.mikrus` and point a specific
project at a different server:

```toml
# ./.mikrus — overrides the global "prod" profile in this directory only
[servers.prod]
srv = "srv99999"
key = "project-api-key"
ssh = "ssh root@srv99999.mikr.us -p 99999"
```

Remember to add `.mikrus` to `.gitignore` so project credentials don't end up in
the repository.

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

## MCP server

The repo also ships `mikrus-mcp`, a [Model Context
Protocol](https://modelcontextprotocol.io/) server that exposes the same
operations as the CLI as MCP tools over stdio, so MCP-aware clients can drive
your mikr.us VPS through it.

### Build & install

`cargo install --path .` installs both binaries: `mikrus` and `mikrus-mcp`
(into `~/.cargo/bin/`).

### Tools

| Tool | Description |
|------|-------------|
| `info` | Show server information |
| `servers` | List user's VPS servers |
| `restart` | Restart the VPS *(side-effectful)* |
| `logs` | Show log entries (optional `id`) |
| `amfetamina` | Performance boost *(side-effectful)* |
| `db` | Show database credentials |
| `exec` | Run a shell command on the VPS *(side-effectful)* |
| `stats` | Disk/memory/uptime statistics |
| `ports` | TCP/UDP ports |
| `cloud` | Cloud services & stats |
| `domain` | Assign domain to port *(side-effectful)* |
| `status` | mikr.us infrastructure status (public, no auth) |
| `list_profiles` | List profiles defined in `~/.mikrus` |

Tools that talk to the authenticated API accept an optional `profile` argument
naming an entry in `~/.mikrus`. Credentials are resolved with the same priority
as the CLI: `MIKRUS_SRV`/`MIKRUS_KEY` env vars first, then the named profile,
then the only profile if there's exactly one configured.

### Adding to Claude Code

After `cargo install --path .` (so `mikrus-mcp` is on your `PATH`), register
the server with Claude Code's `mcp add` command. Pick a scope:

```bash
# User scope — available in every Claude Code session on this machine.
claude mcp add -s user mikrus -- mikrus-mcp

# Project scope — written to ./.mcp.json so teammates pick it up via git.
claude mcp add -s project mikrus -- mikrus-mcp

# Local scope — only this project on this machine (the default).
claude mcp add mikrus -- mikrus-mcp
```

If `mikrus-mcp` isn't on your `PATH`, give the absolute path instead:

```bash
claude mcp add -s user mikrus -- /Users/you/.cargo/bin/mikrus-mcp
```

#### Using `~/.mikrus` for credentials (recommended)

The MCP server reads `~/.mikrus` on startup using the same logic as the CLI,
so the simplest setup is to put your profiles there and let the server pick
them up — no env vars needed in the Claude Code config.

1. Create `~/.mikrus` (TOML, see [Configuration](#configuration) above):

   ```toml
   [servers.marek245]
   srv = "srv12345"
   key = "your-api-key"

   [servers.prod]
   srv = "srv67890"
   key = "another-api-key"
   ```

2. Add the server with no env vars:

   ```bash
   claude mcp add -s user mikrus -- mikrus-mcp
   ```

3. In a Claude Code session:
   - **One profile in `~/.mikrus`** — it's auto-selected; just ask *"show my
     mikrus stats"*.
   - **Multiple profiles** — Claude Code passes the profile name as the
     `profile` tool argument. Tell Claude which one (*"use the prod profile
     and show stats"*) or run the `list_profiles` tool first to see them.

The server reads `~/.mikrus` at startup, so if you edit the file, restart
Claude Code (or run `claude mcp remove mikrus && claude mcp add ...` again)
to pick up the changes.

Like the CLI, the server also merges a `.mikrus` file from its working
directory (the directory Claude Code was started in) on top of `~/.mikrus`, so
a project-local config wins there too.

#### Using env vars instead

If you'd rather not keep credentials in `~/.mikrus`, pass them as env vars in
the `mcp add` command:

```bash
claude mcp add -s user mikrus \
  -e MIKRUS_SRV=srv12345 \
  -e MIKRUS_KEY=your-api-key \
  -- mikrus-mcp
```

Env vars take priority over `~/.mikrus`.

Verify the server is connected:

```bash
claude mcp list           # shows configured servers + status
claude mcp get mikrus     # shows full config for this server
```

Inside a Claude Code session, run `/mcp` to inspect the live connection and
the tools the server is advertising. Once it's green, you can ask things like
*"show my mikrus stats"*, *"what ports are open?"*, or *"restart my VPS"* and
Claude Code will call the corresponding tools.

To remove the server:

```bash
claude mcp remove mikrus
```

The server logs to stderr (controllable via `RUST_LOG`, default `info`); the
JSON-RPC protocol uses stdin/stdout, so don't pipe anything else into it.

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
