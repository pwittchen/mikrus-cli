# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

mikrus-cli is a CLI tool for managing [mikrus.us](https://mikr.us/) VPS services, written in Rust. The API documentation is at https://api.mikr.us/. The project uses Rust edition 2024.

## Build Commands

```bash
cargo build --verbose        # Build the project
cargo test --verbose         # Run all tests
cargo test <test_name>       # Run a single test by name
cargo run                    # Run the CLI
```

## CI

GitHub Actions runs `cargo build --verbose` and `cargo test --verbose` on pushes and PRs to `master` (ubuntu-latest). Markdown file changes are excluded from CI triggers.

## Architecture

- `src/main.rs` — CLI entry point using `clap` (derive API). Defines the `Cli` struct and `Command` enum with 11 subcommands: `info`, `servers`, `restart`, `logs`, `amfetamina`, `db`, `exec`, `stats`, `ports`, `cloud`, `domain`. Before clap parsing, `main` loads `~/.mikrus` and calls `config::extract_profile_arg` to consume an optional leading profile name (e.g. `mikrus marek245 info`).
- `src/api.rs` — `MikrusClient` struct wrapping `reqwest::Client`. All API calls are POST requests to `https://api.mikr.us` with `srv` and `key` form params.
- `src/config.rs` — `Config`/`Profile` structs loaded from `~/.mikrus` (TOML, `[servers.<name>]` sections). Provides `extract_profile_arg` for argv preprocessing and `config_path` for display.

## Dependencies

- `clap` (derive + env) — CLI argument parsing
- `reqwest` (json) — HTTP client
- `tokio` (macros, rt-multi-thread) — async runtime
- `serde` / `serde_json` — serialization
- `anyhow` — error handling

## Configuration

Authentication resolution order (highest first):
1. `--srv`/`--key` CLI flags or `MIKRUS_SRV`/`MIKRUS_KEY` env vars
2. Named profile from `~/.mikrus` passed as first positional arg (`mikrus <profile> <command>`)
3. Single-profile auto-select when `~/.mikrus` has exactly one `[servers.*]` entry

Config file `~/.mikrus` (TOML):
```toml
[servers.marek245]
srv = "srv12345"
key = "your-api-key"
```

## Git Commits

Do not add `Co-Authored-By` lines to commit messages.
