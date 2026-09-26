//! `studentos` — planned commands (implemented by the backend session):
//!
//!   studentos canvas add --base-url https://lms.example.edu   (token read from stdin/prompt;
//!                                                            prints the personal-use-only notice)
//!   studentos folder add <path> [--term-start YYYY-MM-DD] [--label …]
//!   studentos ical add [--label …]                          (feed URL read from stdin/prompt)
//!   studentos sources [list|remove <id>]
//!   studentos sync [--source <id>] [--no-download]
//!   studentos status
//!   studentos courses
//!   studentos course policy <course> <unknown|prohibited|learning_aid|allowed_with_citation|unrestricted> [--note …]
//!   studentos course term <course> --start YYYY-MM-DD [--end YYYY-MM-DD]
//!   studentos course hide|show <course>
//!   studentos search <query> [--course …]
//!   studentos mcp                                            (stdio MCP server)
//!   studentos mcp-config [claude-desktop|claude-code|codex] (prints install snippet)
//!   studentos schema                                         (JSON Schemas for app types)
//!
//! Logging always goes to stderr (stdout is reserved for MCP protocol in `studentos mcp`).

fn main() -> anyhow::Result<()> {
    // Interim entry point: only `schema` works until the clap CLI lands (milestone M-E).
    match std::env::args().nth(1).as_deref() {
        Some("schema") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&studentos_app::json_schema())?
            );
            Ok(())
        }
        _ => todo!("clap CLI — see module docs"),
    }
}
