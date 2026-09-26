//! Text extraction for course materials, plus chunking for full-text search.
//!
//! Output is a list of `Segment`s, each carrying a human-readable `locator` used for
//! citations ("p. 3", "slide 5", "cell 4", "§ Heading"). Chunking splits long segments
//! while keeping their locator.
//!
//! Supported (by extension, case-insensitive; `mime_hint` may override):
//! - `.pdf`  → one segment per page, locator "p. N" (1-based)
//! - `.pptx` → one segment per slide (slide order from `ppt/presentation.xml` sldIdLst,
//!             falling back to numeric order of `ppt/slides/slideN.xml`), locator "slide N";
//!             speaker notes (`ppt/notesSlides`) appended to the slide's text as "Notes: …"
//! - `.docx` → paragraphs from `word/document.xml`, grouped under headings, locator "§ Heading"
//!             (or None before the first heading)
//! - `.ipynb`→ markdown + code cells, locator "cell N"
//! - `.md`, `.markdown`, `.txt`, `.tex`, `.py`, `.java`, `.c`, `.cpp`, `.h`, `.hs`, `.rkt`,
//!   `.r`, `.sql`, `.js`, `.ts` → text; markdown split by headings with locator "§ Heading"
//! - `.html`, `.htm` → via `extract_html`
//! Anything else → `ExtractError::Unsupported`.
//!
//! Safety limits: refuse files > 200 MB; cap total extracted text at 5 MB per file (truncate
//! and note it); zip-based formats must guard against zip bombs (cap per-entry and total
//! decompressed size at 100 MB).

use std::path::Path;

use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub locator: Option<String>,
    pub text: String,
}

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("unsupported file type: {0}")]
    Unsupported(String),
    #[error("extraction failed: {0}")]
    Failed(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Extract text from a file on disk. Segments with only whitespace are dropped.
/// A PDF with no extractable text (scanned) returns `Ok(vec![])` — callers mark it so.
pub fn extract_file(path: &Path, mime_hint: Option<&str>) -> Result<Vec<Segment>, ExtractError> {
    let _ = (path, mime_hint);
    todo!()
}

/// Convert LMS page HTML to plain text segments split at h1–h3 headings
/// (locator "§ Heading"). Scripts/styles removed; links rendered as "text (url)".
pub fn extract_html(html: &str) -> Vec<Segment> {
    let _ = html;
    todo!()
}

/// Whether `extract_file` would attempt this file (by extension / mime).
pub fn is_supported(path: &Path, mime_hint: Option<&str>) -> bool {
    let _ = (path, mime_hint);
    todo!()
}

/// A chunk ready to store: locator + text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkText {
    pub locator: Option<String>,
    pub text: String,
}

/// Split segments into chunks of at most `max_chars` characters (not bytes), preferring
/// paragraph, then sentence, then whitespace boundaries; never split inside a UTF-8 char.
/// Short consecutive segments are NOT merged (each keeps its own locator). Consecutive
/// chunks from one long segment overlap by ~`max_chars / 10` characters.
/// Whitespace is normalised (runs of blank lines collapsed, trailing spaces trimmed).
pub fn chunk_segments(segments: &[Segment], max_chars: usize) -> Vec<ChunkText> {
    let _ = (segments, max_chars);
    todo!()
}

/// Default chunk size used by sync.
pub const DEFAULT_CHUNK_CHARS: usize = 1800;
