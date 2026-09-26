//! "Connect your AI app": install snippets for Claude Desktop, Claude Code, Codex and generic
//! MCP clients, all generated from ONE `McpLaunch` (so a future `.mcpb` Desktop Extension
//! manifest can be generated from the same value).
//!
//! Formats (verified 2026-09-25):
//! - Claude Code: `claude mcp add [options] <name> -- <command> [args…]`; `--env` takes several
//!   `KEY=value` pairs, so another option (`--transport stdio`) must separate it from the name
//!   (https://code.claude.com/docs/en/mcp).
//! - Codex: `[mcp_servers.<name>]` with `command`, `args` and an `env` table in
//!   `~/.codex/config.toml`, shared by the Codex CLI, IDE extension and the ChatGPT desktop app
//!   (https://learn.chatgpt.com/docs/extend/mcp).
//! - Claude Desktop: `mcpServers.<name>` in `claude_desktop_config.json`.
//!
//! Notes state facts only; each note has a stable `McpNoteCode` so UIs can localise.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pagelamp_core::brand;

use crate::{InstallKind, McpClient, McpClientConfig, McpLaunch, McpNoteCode, TemporaryLocation};

/// Which shell quoting rules the Claude Code command uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shell {
    Posix,
    Windows,
}

impl Shell {
    pub(crate) fn current() -> Shell {
        if cfg!(windows) {
            Shell::Windows
        } else {
            Shell::Posix
        }
    }
}

/// All four configs for `launch`. `desktop_config_hint` is the OS-specific location of
/// `claude_desktop_config.json` (None where Claude Desktop has no documented location).
/// When the binary runs from a temporary location (`McpLaunch::temporary_location`), every
/// config starts with a `RunFromTemporaryLocation` note.
pub(crate) fn client_configs(
    launch: &McpLaunch,
    shell: Shell,
    desktop_config_hint: Option<&str>,
) -> Vec<McpClientConfig> {
    let mut configs = vec![
        claude_desktop(launch, desktop_config_hint),
        claude_code(launch, shell),
        codex(launch),
        generic(launch),
    ];
    if let Some(temporary) = launch.temporary_location {
        for config in &mut configs {
            config.notes.insert(0, temporary_note(temporary));
            config
                .note_codes
                .insert(0, McpNoteCode::RunFromTemporaryLocation);
        }
    }
    configs
}

fn temporary_note(location: TemporaryLocation) -> String {
    match location {
        TemporaryLocation::DiskImage | TemporaryLocation::Translocated => format!(
            "{name} is running from a temporary location. Move {name} to Applications, open it from there, then copy this again — otherwise your AI app won't find {name} later.",
            name = brand::PRODUCT_NAME
        ),
        TemporaryLocation::AppImage => format!(
            "The {cli} inside the AppImage moves every launch; for your AI app use the .deb/.rpm or the command-line archive.",
            cli = brand::CLI_NAME
        ),
    }
}

/// What detection needs to know about this process's environment (a parameter, so tests can
/// fake it).
pub(crate) struct LaunchEnv {
    /// `$APPDIR`: set by the AppImage runtime to where the image is mounted.
    pub appdir: Option<PathBuf>,
    /// `$TMPDIR` (AppImages mount under it when it is set).
    pub tmpdir: Option<PathBuf>,
    /// Whether the volume holding a path is mounted read-only.
    pub read_only_volume: fn(&Path) -> bool,
}

impl LaunchEnv {
    pub(crate) fn current() -> LaunchEnv {
        let dir = |var: &str| {
            std::env::var_os(var)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        LaunchEnv {
            appdir: dir("APPDIR"),
            tmpdir: dir("TMPDIR"),
            read_only_volume,
        }
    }
}

/// Where `command` (the absolute binary path) lives, if that place is temporary:
/// - `/AppTranslocation/` anywhere: macOS runs an app not yet moved out of Downloads from a
///   randomised read-only copy;
/// - under `/Volumes/` on a read-only volume: the downloaded disk image (a copy on an
///   external drive is fine);
/// - inside `$APPDIR` (only an AppImage sets it; merely inheriting `APPIMAGE` from a parent
///   app doesn't count), or under a `.mount_*` directory in `/tmp` or `$TMPDIR`.
pub(crate) fn temporary_location(command: &Path, env: &LaunchEnv) -> Option<TemporaryLocation> {
    let text = command.to_string_lossy();
    if text.contains("/AppTranslocation/") {
        return Some(TemporaryLocation::Translocated);
    }
    if text.starts_with("/Volumes/") && (env.read_only_volume)(command) {
        return Some(TemporaryLocation::DiskImage);
    }
    let under_mount = |dir: &Path| {
        command
            .strip_prefix(dir)
            .ok()
            .and_then(|rest| rest.components().next())
            .is_some_and(|first| first.as_os_str().to_string_lossy().starts_with(".mount_"))
    };
    let in_appdir = env
        .appdir
        .as_deref()
        .is_some_and(|appdir| command.starts_with(appdir));
    if in_appdir || under_mount(Path::new("/tmp")) || env.tmpdir.as_deref().is_some_and(under_mount)
    {
        return Some(TemporaryLocation::AppImage);
    }
    None
}

/// Whether the file system holding `path` is mounted read-only (a disk image is).
#[cfg(target_os = "macos")]
fn read_only_volume(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let mut stats = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `path` is a valid NUL-terminated string and `stats` is large enough for the
    // result; it is only read after statfs reported success.
    let ok = unsafe { libc::statfs(path.as_ptr(), stats.as_mut_ptr()) } == 0;
    ok && unsafe { stats.assume_init() }.f_flags & (libc::MNT_RDONLY as u32) != 0
}

#[cfg(not(target_os = "macos"))]
fn read_only_volume(_path: &Path) -> bool {
    false
}

/// `claude_desktop_config.json` location for the OS this binary was built for.
pub(crate) fn claude_desktop_config_hint() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("~/Library/Application Support/Claude/claude_desktop_config.json")
    } else if cfg!(windows) {
        Some("%APPDATA%\\Claude\\claude_desktop_config.json")
    } else {
        None
    }
}

fn claude_desktop(launch: &McpLaunch, hint: Option<&str>) -> McpClientConfig {
    let servers = BTreeMap::from([(brand::MCP_SERVER_KEY, server_entry(launch))]);
    let content = BTreeMap::from([("mcpServers", servers)]);
    let mut notes = Notes::default();
    notes.add(
        McpNoteCode::WorksOnAllClaudePlans,
        "Works on every Claude plan, including Free.",
    );
    notes.add(
        McpNoteCode::AdminsMayDisableExtensions,
        "Admins of Team, Enterprise and Education workspaces can disable extensions.",
    );
    notes.add(
        McpNoteCode::QuitBeforeEditing,
        format!(
            "1) Quit Claude Desktop completely. 2) Open the config file (create it if it's missing). If it is empty, paste this in. If it already has \"mcpServers\", add only the \"{key}\" entry inside it. If it has other settings (such as \"preferences\") but no \"mcpServers\", add the \"mcpServers\": {{ … }} part as a new top-level key inside the outer {{ }}, separated by a comma from the neighbouring setting. 3) Save the file. 4) Open Claude Desktop: {name} appears under the tools (🔌) menu.",
            key = brand::MCP_SERVER_KEY,
            name = brand::PRODUCT_NAME
        ),
    );
    notes.custom_data_dir(launch);
    config(
        McpClient::ClaudeDesktop,
        "Claude Desktop",
        InstallKind::JsonSnippet,
        hint,
        pretty(&content),
        notes,
        launch,
    )
}

fn claude_code(launch: &McpLaunch, shell: Shell) -> McpClientConfig {
    let mut words = vec![
        "claude".to_string(),
        "mcp".into(),
        "add".into(),
        "--scope".into(),
        "user".into(),
    ];
    for (key, value) in &launch.env {
        words.push("--env".into());
        words.push(quote(&format!("{key}={value}"), shell));
    }
    // Must follow `--env` (which takes several KEY=value pairs) so the name isn't swallowed.
    words.extend(["--transport".into(), "stdio".into()]);
    words.push(brand::MCP_SERVER_KEY.into());
    words.push("--".into());
    words.push(quote(&launch.command, shell));
    words.extend(launch.args.iter().map(|arg| quote(arg, shell)));

    let mut notes = Notes::default();
    notes.add(
        McpNoteCode::NeedsPaidClaudePlan,
        "Claude Code needs a paid Claude plan (Pro or higher).",
    );
    notes.add(
        McpNoteCode::RestartClientAfterChange,
        "Run this once in a terminal; new Claude Code sessions then have the server.",
    );
    notes.custom_data_dir(launch);
    config(
        McpClient::ClaudeCode,
        "Claude Code",
        InstallKind::ShellCommand,
        None,
        words.join(" "),
        notes,
        launch,
    )
}

fn codex(launch: &McpLaunch) -> McpClientConfig {
    let key = brand::MCP_SERVER_KEY;
    let args: Vec<String> = launch.args.iter().map(|a| toml_string(a)).collect();
    let mut content = format!(
        "[mcp_servers.{key}]\ncommand = {}\nargs = [{}]\n",
        toml_string(&launch.command),
        args.join(", ")
    );
    if !launch.env.is_empty() {
        content.push_str(&format!("\n[mcp_servers.{key}.env]\n"));
        for (name, value) in &launch.env {
            content.push_str(&format!("{name} = {}\n", toml_string(value)));
        }
    }
    let mut notes = Notes::default();
    notes.add(
        McpNoteCode::CodexConfigSharedWithChatgptDesktop,
        "The Codex CLI, the Codex IDE extension and the ChatGPT desktop app (Work/Codex mode) all read ~/.codex/config.toml.",
    );
    notes.add(
        McpNoteCode::CodexPlusAndEduDocumented,
        "Codex is documented for ChatGPT Plus and higher plans, and for Edu.",
    );
    notes.add(
        McpNoteCode::FreeGoUndocumented,
        "Support on the Free and Go plans is not documented.",
    );
    notes.add(
        McpNoteCode::RestartClientAfterChange,
        "Append this to the file, then restart the app or start a new Codex session.",
    );
    notes.custom_data_dir(launch);
    config(
        McpClient::Codex,
        "Codex / ChatGPT desktop",
        InstallKind::TomlSnippet,
        Some("~/.codex/config.toml"),
        content,
        notes,
        launch,
    )
}

fn generic(launch: &McpLaunch) -> McpClientConfig {
    let mut notes = Notes::default();
    notes.add(
        McpNoteCode::GenericStdioClient,
        format!(
            "Any MCP client that can launch a local (stdio) server can use {}: adapt this command, arguments and environment to that client's config format.",
            brand::PRODUCT_NAME
        ),
    );
    notes.custom_data_dir(launch);
    config(
        McpClient::Generic,
        "Other MCP clients",
        InstallKind::JsonSnippet,
        None,
        pretty(&server_entry(launch)),
        notes,
        launch,
    )
}

// ----- helpers ------------------------------------------------------------------------------

/// Notes and their codes, kept in lockstep.
#[derive(Default)]
struct Notes {
    texts: Vec<String>,
    codes: Vec<McpNoteCode>,
}

impl Notes {
    fn add(&mut self, code: McpNoteCode, text: impl Into<String>) {
        self.codes.push(code);
        self.texts.push(text.into());
    }

    fn custom_data_dir(&mut self, launch: &McpLaunch) {
        if !launch.env.is_empty() {
            self.add(
                McpNoteCode::CustomDataDir,
                format!(
                    "This sets PAGELAMP_HOME because {} uses a non-default data folder on this computer.",
                    brand::PRODUCT_NAME
                ),
            );
        }
    }
}

fn config(
    client: McpClient,
    title: &str,
    install_kind: InstallKind,
    config_path_hint: Option<&str>,
    content: String,
    notes: Notes,
    launch: &McpLaunch,
) -> McpClientConfig {
    McpClientConfig {
        client,
        title: title.to_string(),
        install_kind,
        config_path_hint: config_path_hint.map(str::to_string),
        content,
        notes: notes.texts,
        note_codes: notes.codes,
        launch: launch.clone(),
    }
}

/// `{"command", "args", "env"?}` — the per-server object used by JSON configs (a struct, so
/// the keys keep this natural order in the printed snippet).
#[derive(serde::Serialize)]
struct ServerEntry<'a> {
    command: &'a str,
    args: &'a [String],
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    env: &'a BTreeMap<String, String>,
}

fn server_entry(launch: &McpLaunch) -> ServerEntry<'_> {
    ServerEntry {
        command: &launch.command,
        args: &launch.args,
        env: &launch.env,
    }
}

fn pretty(value: &impl serde::Serialize) -> String {
    serde_json::to_string_pretty(value).expect("plain maps and strings always serialise")
}

/// Quote one shell word. POSIX: unchanged when it only has safe characters, else single
/// quotes (with `'` written as `'\''`). Windows (PowerShell/cmd): double quotes when needed.
fn quote(word: &str, shell: Shell) -> String {
    let safe = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c));
    match (safe, shell) {
        (true, _) => word.to_string(),
        (false, Shell::Posix) => format!("'{}'", word.replace('\'', r"'\''")),
        (false, Shell::Windows) => format!("\"{}\"", word.replace('"', "\\\"")),
    }
}

/// A TOML basic string: `\` and `"` escaped, control characters as `\uXXXX`.
fn toml_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn launch(command: &str, home: Option<&str>) -> McpLaunch {
        McpLaunch {
            command: command.to_string(),
            args: vec!["mcp".to_string()],
            env: home
                .map(|h| BTreeMap::from([("PAGELAMP_HOME".to_string(), h.to_string())]))
                .unwrap_or_default(),
            temporary_location: None,
        }
    }

    fn by_client(configs: &[McpClientConfig], client: McpClient) -> &McpClientConfig {
        configs.iter().find(|c| c.client == client).unwrap()
    }

    #[test]
    fn four_clients_with_notes_in_lockstep() {
        let launch = launch("/Applications/PageLamp/pagelamp", None);
        let configs = client_configs(&launch, Shell::Posix, Some("~/demo.json"));
        let clients: Vec<_> = configs.iter().map(|c| c.client).collect();
        assert_eq!(
            clients,
            [
                McpClient::ClaudeDesktop,
                McpClient::ClaudeCode,
                McpClient::Codex,
                McpClient::Generic
            ]
        );
        for config in &configs {
            assert_eq!(
                config.notes.len(),
                config.note_codes.len(),
                "{:?}",
                config.client
            );
            assert!(!config.note_codes.contains(&McpNoteCode::CustomDataDir));
            assert_eq!(config.launch, launch);
        }
        let desktop = by_client(&configs, McpClient::ClaudeDesktop);
        assert_eq!(desktop.config_path_hint.as_deref(), Some("~/demo.json"));
        assert!(
            desktop
                .note_codes
                .contains(&McpNoteCode::WorksOnAllClaudePlans)
        );
        assert!(
            desktop
                .note_codes
                .contains(&McpNoteCode::AdminsMayDisableExtensions)
        );
        let code = by_client(&configs, McpClient::ClaudeCode);
        assert!(code.note_codes.contains(&McpNoteCode::NeedsPaidClaudePlan));
        let codex = by_client(&configs, McpClient::Codex);
        for expected in [
            McpNoteCode::CodexConfigSharedWithChatgptDesktop,
            McpNoteCode::CodexPlusAndEduDocumented,
            McpNoteCode::FreeGoUndocumented,
        ] {
            assert!(codex.note_codes.contains(&expected));
        }
    }

    #[test]
    fn temporary_locations_get_a_note_first_in_every_config() {
        fn read_only(path: &Path) -> bool {
            path.starts_with("/Volumes/PageLamp")
        }
        let env = |appdir: Option<&str>, tmpdir: Option<&str>| LaunchEnv {
            appdir: appdir.map(PathBuf::from),
            tmpdir: tmpdir.map(PathBuf::from),
            read_only_volume: read_only,
        };
        let plain = env(None, None);
        use TemporaryLocation::*;
        for (command, env, expected) in [
            (
                "/Applications/PageLamp.app/Contents/MacOS/pagelamp",
                &plain,
                None,
            ),
            ("/home/demo/.local/bin/pagelamp", &plain, None),
            (r"C:\Program Files\PageLamp\pagelamp.exe", &plain, None),
            // The downloaded disk image (read-only) vs a copy on an external drive.
            (
                "/Volumes/PageLamp/PageLamp.app/Contents/MacOS/pagelamp",
                &plain,
                Some(DiskImage),
            ),
            (
                "/Volumes/USB Stick/PageLamp.app/Contents/MacOS/pagelamp",
                &plain,
                None,
            ),
            (
                "/private/var/folders/x/T/AppTranslocation/1234/d/PageLamp.app/Contents/MacOS/pagelamp",
                &plain,
                Some(Translocated),
            ),
            (
                "/tmp/.mount_PageLaAbCd/usr/bin/pagelamp",
                &plain,
                Some(AppImage),
            ),
            (
                "/run/user/1000/.mount_PageLa1/usr/bin/pagelamp",
                &env(None, Some("/run/user/1000")),
                Some(AppImage),
            ),
            (
                "/mnt/image/usr/bin/pagelamp",
                &env(Some("/mnt/image"), None),
                Some(AppImage),
            ),
            // A .deb install run from a shell that an AppImage app started (it inherits
            // APPDIR, but the binary is not inside it).
            (
                "/usr/bin/pagelamp",
                &env(Some("/tmp/.mount_Other1"), None),
                None,
            ),
            ("/tmp/pagelamp-build/pagelamp", &plain, None),
        ] {
            assert_eq!(
                temporary_location(Path::new(command), env),
                expected,
                "{command}"
            );
        }

        let mut launch = launch(
            "/Volumes/PageLamp/PageLamp.app/Contents/MacOS/pagelamp",
            None,
        );
        assert!(client_configs(&launch, Shell::Posix, None).iter().all(|c| {
            !c.note_codes
                .contains(&McpNoteCode::RunFromTemporaryLocation)
        }));
        launch.temporary_location = Some(DiskImage);
        for config in client_configs(&launch, Shell::Posix, None) {
            assert_eq!(config.note_codes[0], McpNoteCode::RunFromTemporaryLocation);
            assert!(config.notes[0].contains("Move PageLamp to Applications"));
            assert_eq!(config.notes.len(), config.note_codes.len());
        }
        launch.temporary_location = Some(AppImage);
        let appimage = client_configs(&launch, Shell::Posix, None);
        assert!(appimage[0].notes[0].contains(".deb/.rpm"));
        let json = serde_json::to_value(&launch).unwrap();
        assert_eq!(json["temporary_location"], "appimage");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn read_only_volumes_are_detected() {
        // The sealed system volume is mounted read-only; a temp dir is writable.
        assert!(read_only_volume(Path::new("/System/Library")));
        assert!(!read_only_volume(&std::env::temp_dir()));
        assert!(!read_only_volume(Path::new(
            "/Volumes/no such volume/pagelamp"
        )));
    }

    #[test]
    fn claude_desktop_and_generic_are_valid_json() {
        let launch = launch("/demo/bin/pagelamp", Some("/demo/data \"x\""));
        let configs = client_configs(&launch, Shell::Posix, None);
        let desktop: Value =
            serde_json::from_str(&by_client(&configs, McpClient::ClaudeDesktop).content).unwrap();
        let server = &desktop["mcpServers"]["pagelamp"];
        assert_eq!(server["command"], "/demo/bin/pagelamp");
        assert_eq!(server["args"], json!(["mcp"]));
        assert_eq!(server["env"]["PAGELAMP_HOME"], "/demo/data \"x\"");
        // Quit first: Claude Desktop rewrites its config file on exit.
        let desktop = by_client(&configs, McpClient::ClaudeDesktop);
        assert!(desktop.note_codes.contains(&McpNoteCode::QuitBeforeEditing));
        assert!(
            !desktop
                .note_codes
                .contains(&McpNoteCode::RestartClientAfterChange)
        );
        let steps = &desktop.notes;
        let steps = steps.iter().find(|n| n.contains("Quit")).unwrap();
        assert!(
            steps.starts_with("1) Quit Claude Desktop completely."),
            "{steps}"
        );
        assert!(steps.contains("add only the \"pagelamp\" entry"), "{steps}");
        // Claude Desktop writes "preferences" itself: the file often exists without servers.
        assert!(
            steps.contains("but no \"mcpServers\", add the \"mcpServers\": { … } part"),
            "{steps}"
        );
        let generic: Value =
            serde_json::from_str(&by_client(&configs, McpClient::Generic).content).unwrap();
        assert_eq!(&generic, server);
        assert!(
            by_client(&configs, McpClient::Generic)
                .note_codes
                .contains(&McpNoteCode::CustomDataDir)
        );
    }

    #[test]
    fn env_is_omitted_when_default_data_dir() {
        let configs = client_configs(&launch("/demo/pagelamp", None), Shell::Posix, None);
        let desktop: Value =
            serde_json::from_str(&by_client(&configs, McpClient::ClaudeDesktop).content).unwrap();
        assert!(desktop["mcpServers"]["pagelamp"].get("env").is_none());
        let codex = &by_client(&configs, McpClient::Codex).content;
        assert!(!codex.contains(".env]"), "{codex}");
        let code = &by_client(&configs, McpClient::ClaudeCode).content;
        assert_eq!(
            code,
            "claude mcp add --scope user --transport stdio pagelamp -- /demo/pagelamp mcp"
        );
    }

    #[test]
    fn claude_code_quotes_paths_and_keeps_env_away_from_the_name() {
        let launch = launch(
            "/Users/demo/My Apps/it's/pagelamp",
            Some("/Users/demo/Study Data"),
        );
        let configs = client_configs(&launch, Shell::Posix, None);
        let code = &by_client(&configs, McpClient::ClaudeCode).content;
        assert_eq!(
            code,
            "claude mcp add --scope user --env 'PAGELAMP_HOME=/Users/demo/Study Data' \
             --transport stdio pagelamp -- '/Users/demo/My Apps/it'\\''s/pagelamp' mcp"
        );
        let windows = client_configs(&launch, Shell::Windows, None);
        let code = &by_client(&windows, McpClient::ClaudeCode).content;
        assert!(
            code.contains("\"/Users/demo/My Apps/it's/pagelamp\" mcp"),
            "{code}"
        );
    }

    #[test]
    fn codex_toml_escapes_backslashes_and_quotes() {
        let launch = launch(
            r"C:\Users\Demo\pagelamp.exe",
            Some(r#"C:\Data\"quoted"	tab"#),
        );
        let configs = client_configs(&launch, Shell::Windows, None);
        let codex = &by_client(&configs, McpClient::Codex).content;
        assert_eq!(
            codex,
            "[mcp_servers.pagelamp]\n\
             command = \"C:\\\\Users\\\\Demo\\\\pagelamp.exe\"\n\
             args = [\"mcp\"]\n\
             \n\
             [mcp_servers.pagelamp.env]\n\
             PAGELAMP_HOME = \"C:\\\\Data\\\\\\\"quoted\\\"\\u0009tab\"\n"
        );
    }

    #[test]
    fn toml_string_and_quote_basics() {
        assert_eq!(toml_string("plain"), "\"plain\"");
        assert_eq!(toml_string("a\\b\"c\n"), "\"a\\\\b\\\"c\\u000A\"");
        assert_eq!(
            quote("/usr/local/bin/pagelamp", Shell::Posix),
            "/usr/local/bin/pagelamp"
        );
        assert_eq!(quote("", Shell::Posix), "''");
        assert_eq!(quote("$HOME", Shell::Posix), "'$HOME'");
    }
}
