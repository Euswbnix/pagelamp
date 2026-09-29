//! The dedicated `CODEX_HOME` (design §2.3, D9): Codex's config and sign-in live here, apart from
//! the student's own Codex (`~/.codex`, whose AGENTS.md, skills and MCP servers — including
//! PageLamp's own v0.1 entry — must not leak into PageLamp's runs).
//!
//! - PageLamp writes only `config.toml` (before every Codex command, under the lock) and
//!   `pagelamp.lock`. It never reads anything else here: not `auth.json`, not the "Codex Auth"
//!   keychain item. The folder never goes into logs, reports or exports.
//! - The lock is held around each config rewrite + run, and around login and logout: the desktop
//!   app, the CLI and the Swift shell share this folder, and how Codex rotates tokens under two
//!   concurrent runs is unknown. A second process gets `Busy`, never a parallel run.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = "config.toml";
pub const LOCK_FILE: &str = "pagelamp.lock";

/// The config PageLamp writes (design §2.3; D11: analytics off). Every key was checked against
/// `codex-rs/core/config.schema.json` of the pinned 0.158.0, where unknown keys fail
/// `--strict-config` (the root, `[features]`, `[tools]` and the other tables allow no extra
/// keys). Differences from the design's list: `view_image` is a feature (there is no
/// `tools.view_image`), `code_mode` takes a plain boolean, and 0.158.0 has more tools to turn off
/// (unified exec, JavaScript REPL, browser and computer use, image generation, plan updates, user
/// input requests). The same tool switches are passed again as `-c` overrides on every run, and
/// the JSONL tripwire kills a run that uses a tool anyway.
pub const CONFIG_TOML: &str = r#"# Written by PageLamp before every Codex command it starts; changes here are overwritten.
# This folder is PageLamp's own CODEX_HOME: your own Codex (~/.codex) is never used or changed.
cli_auth_credentials_store = "auto"
check_for_update_on_startup = false
project_root_markers = []
project_doc_max_bytes = 0
web_search = "disabled"

[analytics]
enabled = false

[feedback]
enabled = false

[history]
persistence = "none"

[tools.update_plan]
enabled = false

[tools.experimental_request_user_input]
enabled = false

[features]
shell_tool = false
unified_exec = false
js_repl = false
apps = false
plugins = false
multi_agent = false
collab = false
code_mode = false
hooks = false
codex_hooks = false
plugin_hooks = false
view_image = false
image_generation = false
web_search = false
standalone_web_search = false
browser_use = false
computer_use = false
memories = false

[windows]
sandbox = "unelevated"
"#;

#[derive(Debug, thiserror::Error)]
pub enum HomeError {
    /// Another PageLamp process (or window) is running Codex, signing in or out.
    #[error("Codex is in use by another PageLamp window")]
    Busy,
    #[error("could not prepare Codex's folder: {0}")]
    Io(String),
}

impl From<std::io::Error> for HomeError {
    fn from(err: std::io::Error) -> Self {
        HomeError::Io(err.to_string())
    }
}

/// `<local>/codex-home`.
#[derive(Clone, Debug)]
pub struct CodexHome {
    dir: PathBuf,
}

/// Held while PageLamp rewrites the config and runs a Codex command; released on drop (and by
/// the OS if the process dies).
#[derive(Debug)]
pub struct HomeLock {
    _file: File,
}

impl CodexHome {
    pub fn new(dir: impl Into<PathBuf>) -> CodexHome {
        CodexHome { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Take the lock without waiting (`Busy` when another process or window holds it).
    pub fn lock(&self) -> Result<HomeLock, HomeError> {
        pagelamp_core::paths::create_private_dir_all(&self.dir)?;
        let file = self.open_lock_file()?;
        match file.try_lock() {
            Ok(()) => Ok(HomeLock { _file: file }),
            Err(TryLockError::WouldBlock) => Err(HomeError::Busy),
            Err(TryLockError::Error(err)) => Err(err.into()),
        }
    }

    /// Whether someone holds the lock right now (probe: take and release it).
    pub fn is_locked(&self) -> bool {
        let Ok(file) = self.open_lock_file() else {
            return false;
        };
        matches!(file.try_lock(), Err(TryLockError::WouldBlock))
    }

    /// Rewrite `config.toml` (atomically: a temporary file, then a rename). Needs the lock.
    pub fn write_config(&self, _lock: &HomeLock) -> Result<(), HomeError> {
        let target = self.dir.join(CONFIG_FILE);
        let temp = self
            .dir
            .join(format!("{CONFIG_FILE}.{}.tmp", std::process::id()));
        {
            let mut file = File::create(&temp)?;
            file.write_all(CONFIG_TOML.as_bytes())?;
            file.sync_all()?;
        }
        std::fs::rename(&temp, &target).inspect_err(|_| {
            let _ = std::fs::remove_file(&temp);
        })?;
        Ok(())
    }

    fn open_lock_file(&self) -> std::io::Result<File> {
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.dir.join(LOCK_FILE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_is_valid_toml_with_every_tool_off() {
        let config: toml::Table = toml::from_str(CONFIG_TOML).unwrap();
        assert_eq!(config["cli_auth_credentials_store"].as_str(), Some("auto"));
        assert_eq!(config["web_search"].as_str(), Some("disabled"));
        assert_eq!(config["analytics"]["enabled"].as_bool(), Some(false), "D11");
        assert_eq!(config["history"]["persistence"].as_str(), Some("none"));
        assert_eq!(
            config["project_root_markers"].as_array().map(Vec::len),
            Some(0)
        );
        assert_eq!(config["windows"]["sandbox"].as_str(), Some("unelevated"));
        let features = config["features"].as_table().unwrap();
        assert!(
            features.values().all(|v| v.as_bool() == Some(false)),
            "{features:?}"
        );
        for tool in [
            "shell_tool",
            "unified_exec",
            "apps",
            "multi_agent",
            "code_mode",
            "view_image",
        ] {
            assert!(features.contains_key(tool), "{tool}");
        }
        // No MCP server is ever configured here.
        assert!(!config.contains_key("mcp_servers"));
    }

    /// For the CI contract test (`codex --strict-config` against the real binary):
    /// `CODEX_CONFIG_OUT=<dir> cargo test -p pagelamp-llm --lib -- --ignored contract_config`
    /// writes `<dir>/config.toml` exactly as PageLamp does.
    #[test]
    #[ignore = "writes the config for the CI contract test"]
    fn contract_config() {
        let dir = std::env::var_os("CODEX_CONFIG_OUT").expect("CODEX_CONFIG_OUT");
        let home = CodexHome::new(dir);
        let lock = home.lock().unwrap();
        home.write_config(&lock).unwrap();
    }

    #[test]
    fn one_holder_at_a_time_and_the_config_is_rewritten() {
        let temp = tempfile::tempdir().unwrap();
        let home = CodexHome::new(temp.path().join("codex-home"));
        assert!(!home.is_locked());
        let lock = home.lock().unwrap();
        assert!(home.is_locked());
        assert!(matches!(home.lock(), Err(HomeError::Busy)));
        std::fs::write(home.dir().join(CONFIG_FILE), "model = \"someone-elses\"\n").unwrap();
        home.write_config(&lock).unwrap();
        assert_eq!(
            std::fs::read_to_string(home.dir().join(CONFIG_FILE)).unwrap(),
            CONFIG_TOML
        );
        drop(lock);
        assert!(!home.is_locked());
        home.lock().unwrap();
        // Only the config and the lock file: nothing temporary is left.
        let mut names: Vec<String> = std::fs::read_dir(home.dir())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, [CONFIG_FILE, LOCK_FILE]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(home.dir()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "it may hold a credential file");
        }
    }
}
