//! Product branding — the ONE place user-facing product wording comes from.
//!
//! A distribution (e.g. a student club's branded build) edits the "display" section of this
//! file to rename the product in CLI help, MCP server title/instructions and the
//! "connect your AI app" snippets. No other Rust file should hard-code the product name.
//!
//! The "stable identifiers" section must NOT change between distributions: existing MCP client
//! configs, keychain entries and data directories depend on it.

// ----- display (safe for distributions to edit) -------------------------------------------

/// Product name shown to people ("StudentOS").
pub const PRODUCT_NAME: &str = "StudentOS";

/// One-line description used in CLI `--help` and the MCP server description.
pub const TAGLINE: &str =
    "Your own course materials, deadlines and study plans, served to the AI app you already use";

/// Project homepage (README, issue tracker).
pub const HOMEPAGE: &str = "https://github.com/<owner>/studentos";

// ----- stable identifiers (do NOT change in distributions) --------------------------------

/// Name of the command-line binary students type (`studentos sync`, `studentos mcp`).
pub const CLI_NAME: &str = "studentos";

/// Key under which the MCP server is registered in client configs
/// (`mcpServers.studentos`, `[mcp_servers.studentos]`, `claude mcp add … studentos`).
pub const MCP_SERVER_KEY: &str = "studentos";
