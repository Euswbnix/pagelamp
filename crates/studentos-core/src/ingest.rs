//! Sync-time indexing shared by all sources: hash → (skip if unchanged) → extract → chunk →
//! store chunks → update text state. Heavy work happens here, never in MCP tool calls.

use std::path::Path;

use crate::Result;
use crate::store::Store;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexOutcome {
    /// Text extracted; this many chunks stored. text_status = ok.
    Indexed { chunks: u32 },
    /// Content hash equal to the stored one and text_status already ok — nothing done.
    Unchanged,
    /// Not a supported type. text_status = unsupported, chunks cleared.
    Unsupported,
    /// Supported but no text (e.g. scanned PDF). text_status = ok with 0 chunks,
    /// text_error = "no extractable text (scanned?)".
    Empty,
    /// Extraction error. text_status = error, text_error = message, chunks cleared.
    Failed(String),
}

/// Index a file at `path` for `material_id` (material row must exist).
/// Only `Err` for store/IO problems; extraction problems are reported via `IndexOutcome`.
pub fn index_file(
    store: &Store,
    material_id: &str,
    path: &Path,
    mime: Option<&str>,
) -> Result<IndexOutcome> {
    let _ = (store, material_id, path, mime);
    todo!(
        "sha256 → compare → studentos_extract::extract_file → chunk_segments(DEFAULT_CHUNK_CHARS) → replace_chunks → set_text_state, all in one transaction"
    )
}

/// Index LMS page / announcement / syllabus HTML.
pub fn index_html(store: &Store, material_id: &str, html: &str) -> Result<IndexOutcome> {
    let _ = (store, material_id, html);
    todo!()
}

/// Index plain text (e.g. already-converted content).
pub fn index_text(store: &Store, material_id: &str, text: &str) -> Result<IndexOutcome> {
    let _ = (store, material_id, text);
    todo!()
}

/// Lower-case hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let _ = bytes;
    todo!()
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let _ = path;
    todo!()
}
