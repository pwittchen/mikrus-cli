use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const CONFIG_FILENAME: &str = ".mikrus";

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub servers: BTreeMap<String, Profile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Profile {
    pub srv: String,
    pub key: String,
}

pub fn config_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(CONFIG_FILENAME))
}

pub fn load() -> Result<Config> {
    let Some(path) = config_path() else {
        return Ok(Config::default());
    };
    if !path.exists() {
        return Ok(Config::default());
    }
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;
    let config: Config = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse config file {}", path.display()))?;
    Ok(config)
}

/// If the first positional argument matches a profile name, split it out.
/// Returns (profile_name, remaining_args_without_profile).
///
/// Positional = first token after argv[0] that is not a flag (does not start with `-`).
/// Flags before the profile name (e.g. `--json`) are preserved in place.
pub fn extract_profile_arg(
    args: &[String],
    config: &Config,
) -> (Option<String>, Vec<String>) {
    if args.is_empty() {
        return (None, args.to_vec());
    }

    // Scan args[1..] for the first non-flag token; if it matches a profile, consume it.
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if a.starts_with('-') {
            // Skip this flag; if it's `--foo` with no `=` and takes a value, the value may
            // also be a non-flag token we must not treat as a profile. Conservative approach:
            // skip one additional token if the flag is a known value-bearing flag.
            if is_value_flag(a) && i + 1 < args.len() && !args[i + 1].starts_with('-') {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        // First non-flag positional arg.
        if config.servers.contains_key(a) {
            let mut rest = args.to_vec();
            let profile = rest.remove(i);
            return (Some(profile), rest);
        }
        return (None, args.to_vec());
    }
    (None, args.to_vec())
}

fn is_value_flag(flag: &str) -> bool {
    // Flags defined in Cli that take a value. `=` form is handled by clap naturally.
    matches!(
        flag,
        "--srv" | "--key" | "--truncate"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> Config {
        let mut servers = BTreeMap::new();
        servers.insert(
            "marek245".to_string(),
            Profile {
                srv: "srv12345".to_string(),
                key: "abc".to_string(),
            },
        );
        servers.insert(
            "prod".to_string(),
            Profile {
                srv: "srv67890".to_string(),
                key: "def".to_string(),
            },
        );
        Config { servers }
    }

    fn to_vec(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn extract_profile_when_matches() {
        let cfg = sample_config();
        let args = to_vec(&["mikrus", "marek245", "info"]);
        let (profile, rest) = extract_profile_arg(&args, &cfg);
        assert_eq!(profile.as_deref(), Some("marek245"));
        assert_eq!(rest, to_vec(&["mikrus", "info"]));
    }

    #[test]
    fn no_profile_when_first_positional_is_subcommand() {
        let cfg = sample_config();
        let args = to_vec(&["mikrus", "info"]);
        let (profile, rest) = extract_profile_arg(&args, &cfg);
        assert!(profile.is_none());
        assert_eq!(rest, args);
    }

    #[test]
    fn extract_profile_after_global_flag() {
        let cfg = sample_config();
        let args = to_vec(&["mikrus", "--json", "marek245", "info"]);
        let (profile, rest) = extract_profile_arg(&args, &cfg);
        assert_eq!(profile.as_deref(), Some("marek245"));
        assert_eq!(rest, to_vec(&["mikrus", "--json", "info"]));
    }

    #[test]
    fn no_profile_when_srv_flag_used() {
        let cfg = sample_config();
        // `--srv marek245` — "marek245" is a value of --srv, not a profile name.
        let args = to_vec(&["mikrus", "--srv", "marek245", "--key", "x", "info"]);
        let (profile, rest) = extract_profile_arg(&args, &cfg);
        assert!(profile.is_none());
        assert_eq!(rest, args);
    }

    #[test]
    fn no_profile_when_empty_config() {
        let cfg = Config::default();
        let args = to_vec(&["mikrus", "marek245", "info"]);
        let (profile, rest) = extract_profile_arg(&args, &cfg);
        assert!(profile.is_none());
        assert_eq!(rest, args);
    }

    #[test]
    fn parse_toml_config() {
        let src = r#"
[servers.marek245]
srv = "srv12345"
key = "abc"

[servers.prod]
srv = "srv67890"
key = "def"
"#;
        let cfg: Config = toml::from_str(src).unwrap();
        assert_eq!(cfg.servers.len(), 2);
        assert_eq!(cfg.servers["marek245"].srv, "srv12345");
        assert_eq!(cfg.servers["prod"].key, "def");
    }
}
