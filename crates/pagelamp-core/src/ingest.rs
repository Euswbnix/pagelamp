//! Sync-time indexing shared by all sources: hash → (skip if unchanged) → extract → chunk →
//! store chunks → update text state. Heavy work happens here, never in MCP tool calls.
//!
//! Every `index_*` function follows the same steps:
//! 1. Read the material row (`Error::NotFound` if it does not exist).
//! 2. Hash the content (SHA-256). If the hash equals the stored `content_hash` and
//!    `text_status` is already ok, stop: `IndexOutcome::Unchanged`, nothing is written.
//! 3. Extract and chunk the text. No transaction is open during this step: extracting a big
//!    PDF can take seconds, and holding SQLite's write lock that long would make other writers
//!    (e.g. an MCP server saving a study plan) wait.
//! 4. Write the new text state, the new hash and the chunks in ONE short transaction, so
//!    readers see either the old or the new index of a material, never a mix.
//!
//! Because of step 4, these functions must not be called inside `Store::in_transaction`
//! (SQLite transactions do not nest; the call would fail with a database error).

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::time::SystemTime;

use pagelamp_extract::{DEFAULT_CHUNK_CHARS, ExtractError, Segment, chunk_segments};
use sha2::{Digest, Sha256};

use crate::model::{Chunk, Material, TextStatus};
use crate::store::Store;
use crate::{Error, Result};

/// `text_error` stored for `IndexOutcome::Empty` (a supported file without any text).
pub const NO_TEXT_NOTE: &str = "no extractable text (scanned?)";

/// `sha256_file` reads the file in pieces of this size, so memory use does not depend on
/// the file size.
const HASH_BUFFER_BYTES: usize = 64 * 1024;

/// Most text `index_text` indexes per material (UTF-8 bytes). Same value as the per-file cap
/// inside `pagelamp_extract` (which is not public), so every material stays below it.
const MAX_TEXT_BYTES: usize = 5 * 1024 * 1024;

/// Appended by `index_text` when it cut the text off at `MAX_TEXT_BYTES`.
const TRUNCATED_NOTE: &str = "[Text truncated: only the first 5.0 MB of this text was indexed.]";

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
///
/// `mime` is a hint for the extractor (see `pagelamp_extract::extract_file`). A missing or
/// unreadable file is an `Error::Io` and leaves the stored state untouched, so the next sync
/// simply tries again; the same goes for a file whose size or modification time changed
/// while it was being indexed. In every other case the file's hash is stored, whatever the
/// outcome.
pub fn index_file(
    store: &Store,
    material_id: &str,
    path: &Path,
    mime: Option<&str>,
) -> Result<IndexOutcome> {
    index_file_with(
        store,
        material_id,
        path,
        mime,
        pagelamp_extract::extract_file,
    )
}

/// `index_file` with the extractor passed in (always `pagelamp_extract::extract_file`,
/// except in tests, which use this to change the file while it is being extracted).
fn index_file_with(
    store: &Store,
    material_id: &str,
    path: &Path,
    mime: Option<&str>,
    extract: impl FnOnce(&Path, Option<&str>) -> std::result::Result<Vec<Segment>, ExtractError>,
) -> Result<IndexOutcome> {
    let material = require_material(store, material_id)?;
    let stamp_before = file_stamp(path)?;
    let hash = sha256_file(path)?;
    if is_indexed(&material, &hash) {
        return Ok(IndexOutcome::Unchanged);
    }
    // Slow step, deliberately outside any transaction (see the module docs).
    let extracted = extract(path, mime);
    // Hashing and extracting are two separate reads of the file. If it was rewritten in
    // between, the hash may not describe the stored text, and a matching hash would then
    // keep that wrong text forever ("unchanged"). So store nothing; the next sync retries.
    if file_stamp(path)? != stamp_before {
        return Err(Error::Io(std::io::Error::other(format!(
            "{} changed while it was being indexed; it will be indexed on the next sync",
            path.display()
        ))));
    }
    save_extraction(store, material_id, &hash, extracted)
}

/// Index LMS page / announcement / syllabus HTML.
///
/// The hash is taken over the HTML text itself; locators come from its h1–h3 headings
/// ("§ Heading", see `pagelamp_extract::extract_html`).
pub fn index_html(store: &Store, material_id: &str, html: &str) -> Result<IndexOutcome> {
    let material = require_material(store, material_id)?;
    let hash = sha256_hex(html.as_bytes());
    if is_indexed(&material, &hash) {
        return Ok(IndexOutcome::Unchanged);
    }
    let segments = pagelamp_extract::extract_html(html);
    save_extraction(store, material_id, &hash, Ok(segments))
}

/// Index plain text (e.g. already-converted content).
///
/// The text is treated as one segment without a locator (long text still becomes several
/// chunks). Whitespace-only text gives `IndexOutcome::Empty`. Like extracted files, only the
/// first 5 MB are indexed, followed by a note that the text was cut off; the hash still
/// covers the whole text.
pub fn index_text(store: &Store, material_id: &str, text: &str) -> Result<IndexOutcome> {
    let material = require_material(store, material_id)?;
    let hash = sha256_hex(text.as_bytes());
    if is_indexed(&material, &hash) {
        return Ok(IndexOutcome::Unchanged);
    }
    let segments = vec![Segment {
        locator: None,
        text: capped_text(text, MAX_TEXT_BYTES),
    }];
    save_extraction(store, material_id, &hash, Ok(segments))
}

/// Lower-case hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Lower-case hex SHA-256 of a file's bytes (same format as `sha256_hex`).
///
/// The file is read in 64 KiB pieces, never loaded into memory as a whole.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_BUFFER_BYTES];
    loop {
        let read = match file.read(&mut buffer) {
            Ok(0) => break, // end of file
            Ok(read) => read,
            // A signal interrupted the read before any data arrived: just read again.
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

// ----- helpers ------------------------------------------------------------------------------

/// Size and last-modified time of a file: a cheap way to notice that it was rewritten.
/// (A same-size rewrite within the file system's timestamp resolution goes unnoticed; on
/// macOS/Linux/Windows file systems that resolution is far below a millisecond.)
fn file_stamp(path: &Path) -> std::io::Result<(u64, Option<SystemTime>)> {
    let metadata = std::fs::metadata(path)?;
    Ok((metadata.len(), metadata.modified().ok()))
}

/// `text` cut to at most `max_bytes` (never inside a UTF-8 character) plus `TRUNCATED_NOTE`,
/// or `text` unchanged when it already fits.
fn capped_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n\n{TRUNCATED_NOTE}", &text[..end])
}

/// The material row, or `Error::NotFound` (same message style as the store's errors).
fn require_material(store: &Store, material_id: &str) -> Result<Material> {
    store
        .get_material(material_id)?
        .ok_or_else(|| Error::NotFound(format!("material '{material_id}'")))
}

/// True when `material` already holds the successfully indexed text of content `hash`.
///
/// Any other stored status (pending, error, unsupported, …) is retried even with the same
/// hash: the extractor may have improved, or `upsert_material` reset the status to pending
/// because the file moved.
fn is_indexed(material: &Material, hash: &str) -> bool {
    material.text_status == TextStatus::Ok && material.content_hash.as_deref() == Some(hash)
}

/// Turn an extraction result into the material's new text state (one row of the
/// `IndexOutcome` table) and store it together with the content `hash`.
///
/// `ExtractError::Io` means the file could not be read (e.g. deleted after it was hashed).
/// That says nothing about the file's content, so it is returned as `Error::Io` and nothing
/// is written.
fn save_extraction(
    store: &Store,
    material_id: &str,
    hash: &str,
    extracted: std::result::Result<Vec<Segment>, ExtractError>,
) -> Result<IndexOutcome> {
    match extracted {
        Ok(segments) => {
            let chunks = to_chunks(material_id, &segments);
            if chunks.is_empty() {
                write_text_state(
                    store,
                    material_id,
                    hash,
                    TextStatus::Ok,
                    Some(NO_TEXT_NOTE),
                    &[],
                )?;
                Ok(IndexOutcome::Empty)
            } else {
                write_text_state(store, material_id, hash, TextStatus::Ok, None, &chunks)?;
                // More than u32::MAX chunks would take terabytes of text; saturate, never panic.
                let count = u32::try_from(chunks.len()).unwrap_or(u32::MAX);
                Ok(IndexOutcome::Indexed { chunks: count })
            }
        }
        Err(ExtractError::Unsupported(_)) => {
            write_text_state(store, material_id, hash, TextStatus::Unsupported, None, &[])?;
            Ok(IndexOutcome::Unsupported)
        }
        Err(ExtractError::Failed(message)) => {
            write_text_state(
                store,
                material_id,
                hash,
                TextStatus::Error,
                Some(&message),
                &[],
            )?;
            Ok(IndexOutcome::Failed(message))
        }
        Err(ExtractError::Io(error)) => Err(Error::Io(error)),
    }
}

/// Chunk the segments for search. `ord` counts from 0; each chunk keeps its locator.
fn to_chunks(material_id: &str, segments: &[Segment]) -> Vec<Chunk> {
    // `(0..)` numbers the chunks as u32 directly, so no integer casts are needed.
    (0..)
        .zip(chunk_segments(segments, DEFAULT_CHUNK_CHARS))
        .map(|(ord, chunk)| Chunk {
            material_id: material_id.to_string(),
            ord,
            locator: chunk.locator,
            text: chunk.text,
        })
        .collect()
}

/// Store text status/error, the content hash and the chunks (replacing all old chunks) in one
/// short transaction. An empty `chunks` clears the material's chunks.
fn write_text_state(
    store: &Store,
    material_id: &str,
    hash: &str,
    status: TextStatus,
    text_error: Option<&str>,
    chunks: &[Chunk],
) -> Result<()> {
    store.in_transaction(|store| {
        // First, so a material deleted meanwhile fails with NotFound, not a foreign-key error.
        store.set_text_state(material_id, status, text_error, Some(hash))?;
        store.replace_chunks(material_id, chunks)
    })
}

#[cfg(test)]
mod tests {
    //! Paths that the public API reaches only through a race (a file changing or vanishing
    //! mid-sync, a material deleted mid-sync). All data is synthetic.

    use std::path::PathBuf;

    use super::*;
    use crate::model::{CourseUpsert, MaterialKind, MaterialUpsert, SourceKind, SourceRecord};

    const COURSE: &str = "folder:demo/course/DEMO101";
    const MATERIAL: &str = "folder:demo/course/DEMO101/material/notes.txt";

    /// In-memory store with one material ("DEMO101 Intro to Demo Studies" / notes.txt).
    fn demo_store() -> Store {
        let store = Store::open_in_memory().unwrap();
        store
            .upsert_source(&SourceRecord {
                id: "folder:demo".to_string(),
                kind: SourceKind::Folder,
                label: "Demo courses".to_string(),
                config: serde_json::json!({ "path": "/demo/courses" }),
                last_synced_at: None,
                last_error: None,
                last_error_kind: None,
            })
            .unwrap();
        store
            .upsert_course(&CourseUpsert {
                id: COURSE.to_string(),
                source_id: "folder:demo".to_string(),
                external_id: "DEMO101".to_string(),
                code: Some("DEMO101".to_string()),
                name: "Intro to Demo Studies".to_string(),
                term_start: None,
                term_end: None,
                url: None,
                syllabus_text: None,
                lms: Default::default(),
            })
            .unwrap();
        store
            .upsert_material(&MaterialUpsert {
                id: MATERIAL.to_string(),
                course_id: COURSE.to_string(),
                module_id: None,
                kind: MaterialKind::File,
                title: "notes.txt".to_string(),
                url: None,
                local_path: None,
                mime: None,
                published_at: None,
                week_hint: None,
            })
            .unwrap();
        store
    }

    fn write_notes(dir: &tempfile::TempDir, text: &str) -> PathBuf {
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, text).unwrap();
        path
    }

    /// Text status, error, hash and chunk count of the demo material.
    fn state(store: &Store) -> (TextStatus, Option<String>, Option<String>, u32) {
        let material = store.get_material(MATERIAL).unwrap().unwrap();
        let chunks = store.chunk_count(MATERIAL).unwrap();
        (
            material.text_status,
            material.text_error,
            material.content_hash,
            chunks,
        )
    }

    fn one_segment(text: &str) -> Vec<Segment> {
        vec![Segment {
            locator: None,
            text: text.to_string(),
        }]
    }

    #[test]
    fn capped_text_cuts_at_a_char_boundary_and_adds_a_note() {
        assert_eq!(capped_text("héllo", 10), "héllo");
        assert_eq!(capped_text("héllo", 6), "héllo"); // exactly 6 bytes ("é" takes 2)
        assert_eq!(capped_text("héllo", 2), format!("h\n\n{TRUNCATED_NOTE}"));
        assert_eq!(capped_text("héllo", 3), format!("hé\n\n{TRUNCATED_NOTE}"));
    }

    #[test]
    fn extraction_io_error_writes_nothing() {
        let store = demo_store();
        let dir = tempfile::tempdir().unwrap();
        let path = write_notes(&dir, "Demo notes about omicronword.");
        index_file(&store, MATERIAL, &path, None).unwrap();
        let before = state(&store);

        let gone = ExtractError::Io(std::io::Error::other("gone"));
        let result = save_extraction(&store, MATERIAL, &sha256_hex(b"new"), Err(gone));

        assert!(matches!(result, Err(Error::Io(_))), "{result:?}");
        assert_eq!(state(&store), before);
    }

    #[test]
    fn material_deleted_during_extraction_is_not_found() {
        let store = demo_store();
        let missing = "folder:demo/course/DEMO101/material/deleted.txt";

        let result = save_extraction(&store, missing, "hash", Ok(one_segment("Demo text")));

        assert!(matches!(result, Err(Error::NotFound(_))), "{result:?}");
        assert_eq!(store.counts().unwrap().chunks, 0);
    }

    #[test]
    fn file_rewritten_during_extraction_writes_nothing() {
        let store = demo_store();
        let dir = tempfile::tempdir().unwrap();
        let path = write_notes(&dir, "First draft about piword.");
        index_file(&store, MATERIAL, &path, None).unwrap();
        let before = state(&store);
        std::fs::write(&path, "Second draft about rhoword.").unwrap();

        // An editor rewrites the file while it is being extracted: the extractor sees
        // other bytes than the ones that were hashed.
        let result = index_file_with(&store, MATERIAL, &path, None, |path, mime| {
            std::fs::write(path, "Third draft, longer, about sigmaword.").unwrap();
            pagelamp_extract::extract_file(path, mime)
        });

        assert!(matches!(result, Err(Error::Io(_))), "{result:?}");
        assert_eq!(state(&store), before);
        // The next sync indexes the file as it is now.
        assert_eq!(
            index_file(&store, MATERIAL, &path, None).unwrap(),
            IndexOutcome::Indexed { chunks: 1 }
        );
        assert_eq!(state(&store).2, Some(sha256_file(&path).unwrap()));
    }

    #[test]
    fn same_size_rewrite_during_extraction_is_noticed_by_its_mtime() {
        let store = demo_store();
        let dir = tempfile::tempdir().unwrap();
        let path = write_notes(&dir, "Draft about tauword.");

        let result = index_file_with(&store, MATERIAL, &path, None, |path, mime| {
            // Same length, different time: like saving an undo of the same size.
            let file = std::fs::File::options().write(true).open(path).unwrap();
            let earlier = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1);
            file.set_modified(earlier).unwrap();
            pagelamp_extract::extract_file(path, mime)
        });

        assert!(matches!(result, Err(Error::Io(_))), "{result:?}");
        assert_eq!(state(&store).0, TextStatus::Pending);
        assert_eq!(store.chunk_count(MATERIAL).unwrap(), 0);
    }
}
