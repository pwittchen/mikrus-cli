---
name: mikrus
description: Manage mikr.us VPS servers via the `mikrus` CLI — show info/stats/ports, restart, view logs, execute remote commands, manage databases and domains, and SSH into the server. Use whenever the user asks to check, inspect, restart, or operate on their mikr.us VPS.
---

# mikrus CLI

`mikrus` is a Rust CLI for managing [mikr.us](https://mikr.us/) VPS servers via
the [mikr.us API](https://api.mikr.us/). Use it whenever the user wants to
inspect or operate on their mikr.us VPS.

## Invocation

```
mikrus [PROFILE] [--srv <SRV>] [--key <KEY>] [--json] <COMMAND>
```

Append `--json` when you need raw JSON for programmatic parsing instead of
formatted text.

## Credential resolution

Credentials are resolved in this order (highest first):

1. `--srv`/`--key` flags, or `MIKRUS_SRV`/`MIKRUS_KEY` env vars
2. Named profile from `~/.mikrus` passed as first positional arg
   (`mikrus marek245 info`)
3. Auto-selected profile when `~/.mikrus` contains exactly one entry

If you are not sure which profile to use, run `mikrus config` first — it prints
the config file path, configured profiles, and currently active credentials.
When the config file has multiple profiles and the user hasn't specified one,
ask which profile to target rather than guessing.

## Commands

| Command | Purpose |
|---|---|
| `info` | Server information |
| `servers` | List all user servers |
| `restart` | Restart the server |
| `logs [ID]` | Log entries (optional specific ID) |
| `logs short` | Condensed one-line log summary (aligned columns, 100 char cap) |
| `amfetamina` | Performance boost |
| `db` | Database credentials |
| `exec <CMD>` | Run a shell command on the server |
| `stats` | Disk / memory / uptime stats |
| `stats short` | Shortcut for `stats --truncate 100` |
| `stats --truncate <N>` | Truncate long lines at N chars (0 = no truncation) |
| `ports` | TCP / UDP ports |
| `cloud` | Cloud services & stats |
| `domain <PORT> [DOMAIN]` | Assign domain to a port — omit DOMAIN for auto-assignment. Available: `*.tojest.dev`, `*.bieda.it`, `*.toadres.pl`, `*.byst.re` |
| `config` | Show config path, profiles, active credentials |
| `ssh` | SSH into the server (uses optional `ssh` field from the active profile) |
| `status` | mikr.us infrastructure status from `status.mikr.us` — colored dots per monitor. Auto-detects the user's hosting server by reading the `<h1>` of `<srv>.mikrus.xyz`, prints a `Your server: srvNN.mikr.us (<user_srv>)` header, and marks the matching monitor with `→`. Status page itself needs no auth; the host lookup uses no credentials either. |
| `status short` | One line per matched user server (e.g. `● srv30  up`); skips the full grid. Best when the user just wants to know if their VPS is up. |

## SSH

`mikrus ssh` does **not** call the API. It reads the `ssh` string from the
active profile in `~/.mikrus` and shells it out via `sh -c`, so any flags,
ports, or identity files embedded in that string are honored. If the profile
has no `ssh` field, the command errors with instructions to add one.

## Tips for the assistant

- Prefer `--json` when you need to extract a specific field or feed the output
  into further processing.
- `stats short` / `logs short` / `status short` are usually the right default
  for humans — they're easier to read than the full output. Reach for
  `status short` when the user just wants a yes/no answer about their VPS.
- Destructive-ish commands (`restart`, `domain`, `exec`) affect the live VPS.
  Confirm with the user before running them unless the user has just asked for
  exactly that action.
- `mikrus` with no subcommand prints the logo and `--help`; don't rely on this
  for scripting.
- Project docs: see `README.md` at the repo root for installation and the full
  config-file schema.
