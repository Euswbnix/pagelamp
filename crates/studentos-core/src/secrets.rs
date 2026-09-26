//! OS keychain storage for source secrets (Canvas access token, calendar-feed URL).
//!
//! Keychain service name: `dev.studentos`; account = source id (e.g. `canvas:q.utoronto.ca`).
//! For CI/tests an environment override is honoured: `STUDENTOS_SECRET_<SANITISED_ID>`
//! where the id is upper-cased and every non-alphanumeric char becomes `_`
//! (e.g. `STUDENTOS_SECRET_CANVAS_Q_UTORONTO_CA`).
//!
//! Secrets must never be written to the database, logs, or MCP output.

use crate::Result;

pub const KEYCHAIN_SERVICE: &str = "dev.studentos";

/// Name of the env var that overrides the keychain for `source_id`.
pub fn env_override_name(source_id: &str) -> String {
    let _ = source_id;
    todo!("STUDENTOS_SECRET_ + sanitised id")
}

pub fn set_secret(source_id: &str, secret: &str) -> Result<()> {
    let _ = (source_id, secret);
    todo!("store in OS keychain")
}

/// Env override first, then keychain. `Ok(None)` when absent.
pub fn get_secret(source_id: &str) -> Result<Option<String>> {
    let _ = source_id;
    todo!("read env override or OS keychain")
}

/// Remove from keychain; absent is not an error.
pub fn delete_secret(source_id: &str) -> Result<()> {
    let _ = source_id;
    todo!("delete from OS keychain")
}
