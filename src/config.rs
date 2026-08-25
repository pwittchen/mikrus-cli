use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
    #[serde(default)]
    pub ssh: Option<String>,
    /// Marks this profile as the default one. At most one profile per file should set it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
}

impl Profile {
    pub fn is_default(&self) -> bool {
        self.default.unwrap_or(false)
    }
}

impl Config {
    /// Name of the profile explicitly marked with `default = true`
    /// (the first one, in name order, if several are marked).
    pub fn explicit_default(&self) -> Option<&str> {
        self.servers
            .iter()
            .find(|(_, p)| p.is_default())
            .map(|(name, _)| name.as_str())
    }

    /// The profile that commands run against when none is named: the one marked
    /// `default = true`, or — when nothing is marked — the first one in the list.
    /// The bool tells which of the two it is (`true` = explicitly marked).
    pub fn effective_default(&self) -> Option<(&str, bool)> {
        if let Some(name) = self.explicit_default() {
            return Some((name, true));
        }
        self.servers.keys().next().map(|name| (name.as_str(), false))
    }
}

/// Global config, project-local config, and the merged result, kept apart so that
/// `mikrus ctx` can tell where each profile came from.
#[derive(Debug, Default)]
pub struct LoadedConfig {
    /// `~/.mikrus` merged with `./.mikrus` — what commands actually use.
    pub merged: Config,
    pub global: Config,
    pub global_path: Option<PathBuf>,
    pub local: Option<Config>,
    pub local_path: Option<PathBuf>,
}

impl LoadedConfig {
    /// Effective default profile of the merged config. A `default = true` in the
    /// project-local file wins over one in the global file.
    pub fn effective_default(&self) -> Option<(&str, bool)> {
        if let Some(local) = &self.local {
            if let Some(name) = local.explicit_default() {
                if let Some((name, _)) = self.merged.servers.get_key_value(name) {
                    return Some((name.as_str(), true));
                }
            }
        }
        self.merged.effective_default()
    }

    /// File that defines `name`: the local config when it has an entry for it,
    /// the global one otherwise.
    pub fn defining_path(&self, name: &str) -> Option<&Path> {
        if let (Some(local), Some(path)) = (&self.local, &self.local_path) {
            if local.servers.contains_key(name) {
                return Some(path.as_path());
            }
        }
        self.global_path.as_deref()
    }
}

/// Global config file: `~/.mikrus`.
pub fn config_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(CONFIG_FILENAME))
}

/// Project-local config file: `./.mikrus` in the current working directory.
/// Returns `None` when it does not exist, or when it is the same file as the global one.
pub fn local_config_path() -> Option<PathBuf> {
    let path = std::env::current_dir().ok()?.join(CONFIG_FILENAME);
    if !path.exists() {
        return None;
    }
    if config_path().is_some_and(|global| global == path) {
        return None;
    }
    Some(path)
}

/// Loads `~/.mikrus` and then merges `./.mikrus` on top of it.
/// A profile defined in the local file replaces the global profile of the same name;
/// profiles that only exist globally are kept.
pub fn load() -> Result<Config> {
    Ok(load_all()?.merged)
}

/// Same as [`load`], but keeps the global and project-local configs separately
/// alongside the merged result.
pub fn load_all() -> Result<LoadedConfig> {
    let global_path = config_path();
    let global = match &global_path {
        Some(path) => read_config(path)?,
        None => Config::default(),
    };
    let local_path = local_config_path();
    let local = match &local_path {
        Some(path) => Some(read_config(path)?),
        None => None,
    };

    let mut merged = Config {
        servers: global.servers.clone(),
    };
    if let Some(local) = &local {
        merge(
            &mut merged,
            Config {
                servers: local.servers.clone(),
            },
        );
    }

    Ok(LoadedConfig {
        merged,
        global,
        global_path,
        local,
        local_path,
    })
}

/// Rewrites `path` so that only `default_name` carries `default = true`; the key is
/// removed from every other profile. Comments and formatting are preserved.
/// Returns whether the file actually contains a profile named `default_name`.
pub fn write_default_flag(path: &Path, default_name: Option<&str>) -> Result<bool> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;
    let mut doc: toml_edit::DocumentMut = contents
        .parse()
        .with_context(|| format!("Failed to parse config file {}", path.display()))?;

    let mut found = false;
    if let Some(servers) = doc
        .get_mut("servers")
        .and_then(toml_edit::Item::as_table_like_mut)
    {
        for (name, item) in servers.iter_mut() {
            let Some(profile) = item.as_table_like_mut() else {
                continue;
            };
            if default_name == Some(name.get()) {
                profile.insert("default", toml_edit::value(true));
                found = true;
            } else {
                profile.remove("default");
            }
        }
    }

    std::fs::write(path, doc.to_string())
        .with_context(|| format!("Failed to write config file {}", path.display()))?;
    Ok(found)
}

/// Merges `local` into `global`, with local profiles taking precedence.
fn merge(global: &mut Config, local: Config) {
    global.servers.extend(local.servers);
}

fn read_config(path: &PathBuf) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;
    toml::from_str(&contents)
        .with_context(|| format!("Failed to parse config file {}", path.display()))
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
                ssh: None,
                default: None,
            },
        );
        servers.insert(
            "prod".to_string(),
            Profile {
                srv: "srv67890".to_string(),
                key: "def".to_string(),
                ssh: None,
                default: None,
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
    fn local_config_overrides_global_profiles() {
        let mut global = sample_config();
        let local: Config = toml::from_str(
            r#"
[servers.marek245]
srv = "srv99999"
key = "local-key"
ssh = "ssh root@local -p 10022"
"#,
        )
        .unwrap();

        merge(&mut global, local);

        // Overridden by the local file.
        assert_eq!(global.servers["marek245"].srv, "srv99999");
        assert_eq!(global.servers["marek245"].key, "local-key");
        assert_eq!(
            global.servers["marek245"].ssh.as_deref(),
            Some("ssh root@local -p 10022")
        );
        // Global-only profile is preserved.
        assert_eq!(global.servers["prod"].srv, "srv67890");
        assert_eq!(global.servers.len(), 2);
    }

    #[test]
    fn local_config_adds_new_profiles() {
        let mut global = sample_config();
        let local: Config = toml::from_str(
            r#"
[servers.staging]
srv = "srv11111"
key = "ghi"
"#,
        )
        .unwrap();

        merge(&mut global, local);

        assert_eq!(global.servers.len(), 3);
        assert_eq!(global.servers["staging"].srv, "srv11111");
        assert_eq!(global.servers["marek245"].srv, "srv12345");
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
ssh = "ssh root@example.com -p 12345"
"#;
        let cfg: Config = toml::from_str(src).unwrap();
        assert_eq!(cfg.servers.len(), 2);
        assert_eq!(cfg.servers["marek245"].srv, "srv12345");
        assert_eq!(cfg.servers["prod"].key, "def");
        assert!(cfg.servers["marek245"].ssh.is_none());
        assert_eq!(
            cfg.servers["prod"].ssh.as_deref(),
            Some("ssh root@example.com -p 12345")
        );
    }

    #[test]
    fn parse_default_flag() {
        let src = r#"
[servers.marek245]
srv = "srv12345"
key = "abc"

[servers.prod]
srv = "srv67890"
key = "def"
default = true
"#;
        let cfg: Config = toml::from_str(src).unwrap();
        assert!(!cfg.servers["marek245"].is_default());
        assert!(cfg.servers["prod"].is_default());
        assert_eq!(cfg.explicit_default(), Some("prod"));
        assert_eq!(cfg.effective_default(), Some(("prod", true)));
    }

    #[test]
    fn effective_default_falls_back_to_first_profile() {
        let cfg = sample_config();
        assert!(cfg.explicit_default().is_none());
        // BTreeMap keeps profiles in name order, so "marek245" comes first.
        assert_eq!(cfg.effective_default(), Some(("marek245", false)));
    }

    #[test]
    fn effective_default_is_none_for_empty_config() {
        assert!(Config::default().effective_default().is_none());
    }

    fn loaded(global_src: &str, local_src: Option<&str>) -> LoadedConfig {
        let global: Config = toml::from_str(global_src).unwrap();
        let local: Option<Config> = local_src.map(|s| toml::from_str(s).unwrap());
        let mut merged = Config {
            servers: global.servers.clone(),
        };
        if let Some(local) = &local {
            merge(
                &mut merged,
                Config {
                    servers: local.servers.clone(),
                },
            );
        }
        LoadedConfig {
            merged,
            global,
            global_path: Some(PathBuf::from("/home/u/.mikrus")),
            local,
            local_path: local_src.map(|_| PathBuf::from("/proj/.mikrus")),
        }
    }

    const GLOBAL_SRC: &str = r#"
[servers.marek245]
srv = "srv12345"
key = "abc"

[servers.prod]
srv = "srv67890"
key = "def"
default = true
"#;

    #[test]
    fn local_default_wins_over_global_default() {
        let cfg = loaded(
            GLOBAL_SRC,
            Some(
                r#"
[servers.staging]
srv = "srv11111"
key = "ghi"
default = true
"#,
            ),
        );
        assert_eq!(cfg.effective_default(), Some(("staging", true)));
    }

    #[test]
    fn global_default_used_when_local_marks_none() {
        let cfg = loaded(
            GLOBAL_SRC,
            Some(
                r#"
[servers.staging]
srv = "srv11111"
key = "ghi"
"#,
            ),
        );
        assert_eq!(cfg.effective_default(), Some(("prod", true)));
    }

    #[test]
    fn defining_path_prefers_local_file() {
        let cfg = loaded(
            GLOBAL_SRC,
            Some(
                r#"
[servers.prod]
srv = "srv99999"
key = "local"
"#,
            ),
        );
        assert_eq!(cfg.defining_path("prod").unwrap(), Path::new("/proj/.mikrus"));
        assert_eq!(
            cfg.defining_path("marek245").unwrap(),
            Path::new("/home/u/.mikrus")
        );
    }

    #[test]
    fn write_default_flag_moves_the_marker_and_keeps_comments() {
        let dir = std::env::temp_dir().join(format!(
            "mikrus-cli-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".mikrus");
        std::fs::write(
            &path,
            r#"# my servers
[servers.marek245]
srv = "srv12345" # main box
key = "abc"

[servers.prod]
srv = "srv67890"
key = "def"
default = true
"#,
        )
        .unwrap();

        assert!(write_default_flag(&path, Some("marek245")).unwrap());

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("# my servers"), "comments lost: {written}");
        assert!(written.contains("# main box"));
        let cfg: Config = toml::from_str(&written).unwrap();
        assert_eq!(cfg.explicit_default(), Some("marek245"));
        assert!(!cfg.servers["prod"].is_default());

        // Clearing every marker.
        assert!(!write_default_flag(&path, None).unwrap());
        let cfg: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(cfg.explicit_default().is_none());

        // Unknown profile name → nothing marked, reported as not found.
        assert!(!write_default_flag(&path, Some("ghost")).unwrap());

        std::fs::remove_dir_all(&dir).ok();
    }
}
