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

use weekmark_core::brand;

use crate::{InstallKind, McpClient, McpClientConfig, McpLaunch, McpNoteCode};

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
pub(crate) fn client_configs(
    launch: &McpLaunch,
    shell: Shell,
    desktop_config_hint: Option<&str>,
) -> Vec<McpClientConfig> {
    vec![
        claude_desktop(launch, desktop_config_hint),
        claude_code(launch, shell),
        codex(launch),
        generic(launch),
    ]
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
        McpNoteCode::RestartClientAfterChange,
        format!(
            "Merge this into the config file (keep any other servers), then quit and reopen Claude Desktop. {} appears under the tools (🔌) menu.",
            brand::PRODUCT_NAME
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
                    "This sets WEEKMARK_HOME because {} uses a non-default data folder on this computer.",
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
                .map(|h| BTreeMap::from([("WEEKMARK_HOME".to_string(), h.to_string())]))
                .unwrap_or_default(),
        }
    }

    fn by_client(configs: &[McpClientConfig], client: McpClient) -> &McpClientConfig {
        configs.iter().find(|c| c.client == client).unwrap()
    }

    #[test]
    fn four_clients_with_notes_in_lockstep() {
        let launch = launch("/Applications/Weekmark/weekmark", None);
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
    fn claude_desktop_and_generic_are_valid_json() {
        let launch = launch("/demo/bin/weekmark", Some("/demo/data \"x\""));
        let configs = client_configs(&launch, Shell::Posix, None);
        let desktop: Value =
            serde_json::from_str(&by_client(&configs, McpClient::ClaudeDesktop).content).unwrap();
        let server = &desktop["mcpServers"]["weekmark"];
        assert_eq!(server["command"], "/demo/bin/weekmark");
        assert_eq!(server["args"], json!(["mcp"]));
        assert_eq!(server["env"]["WEEKMARK_HOME"], "/demo/data \"x\"");
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
        let configs = client_configs(&launch("/demo/weekmark", None), Shell::Posix, None);
        let desktop: Value =
            serde_json::from_str(&by_client(&configs, McpClient::ClaudeDesktop).content).unwrap();
        assert!(desktop["mcpServers"]["weekmark"].get("env").is_none());
        let codex = &by_client(&configs, McpClient::Codex).content;
        assert!(!codex.contains(".env]"), "{codex}");
        let code = &by_client(&configs, McpClient::ClaudeCode).content;
        assert_eq!(
            code,
            "claude mcp add --scope user --transport stdio weekmark -- /demo/weekmark mcp"
        );
    }

    #[test]
    fn claude_code_quotes_paths_and_keeps_env_away_from_the_name() {
        let launch = launch(
            "/Users/demo/My Apps/it's/weekmark",
            Some("/Users/demo/Study Data"),
        );
        let configs = client_configs(&launch, Shell::Posix, None);
        let code = &by_client(&configs, McpClient::ClaudeCode).content;
        assert_eq!(
            code,
            "claude mcp add --scope user --env 'WEEKMARK_HOME=/Users/demo/Study Data' \
             --transport stdio weekmark -- '/Users/demo/My Apps/it'\\''s/weekmark' mcp"
        );
        let windows = client_configs(&launch, Shell::Windows, None);
        let code = &by_client(&windows, McpClient::ClaudeCode).content;
        assert!(
            code.contains("\"/Users/demo/My Apps/it's/weekmark\" mcp"),
            "{code}"
        );
    }

    #[test]
    fn codex_toml_escapes_backslashes_and_quotes() {
        let launch = launch(
            r"C:\Users\Demo\weekmark.exe",
            Some(r#"C:\Data\"quoted"	tab"#),
        );
        let configs = client_configs(&launch, Shell::Windows, None);
        let codex = &by_client(&configs, McpClient::Codex).content;
        assert_eq!(
            codex,
            "[mcp_servers.weekmark]\n\
             command = \"C:\\\\Users\\\\Demo\\\\weekmark.exe\"\n\
             args = [\"mcp\"]\n\
             \n\
             [mcp_servers.weekmark.env]\n\
             WEEKMARK_HOME = \"C:\\\\Data\\\\\\\"quoted\\\"\\u0009tab\"\n"
        );
    }

    #[test]
    fn toml_string_and_quote_basics() {
        assert_eq!(toml_string("plain"), "\"plain\"");
        assert_eq!(toml_string("a\\b\"c\n"), "\"a\\\\b\\\"c\\u000A\"");
        assert_eq!(
            quote("/usr/local/bin/weekmark", Shell::Posix),
            "/usr/local/bin/weekmark"
        );
        assert_eq!(quote("", Shell::Posix), "''");
        assert_eq!(quote("$HOME", Shell::Posix), "'$HOME'");
    }
}
