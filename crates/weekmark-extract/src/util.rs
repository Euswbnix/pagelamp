//! Small helpers shared by the format modules: text decoding, whitespace clean-up,
//! size-capped reading, heading locators and panic isolation.

use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::ExtractError;

/// Shorthand for `ExtractError::Failed`.
pub(crate) fn failed(message: impl Into<String>) -> ExtractError {
    ExtractError::Failed(message.into())
}

/// Decode file bytes as text: UTF-8 (a leading BOM is stripped), with invalid sequences
/// replaced by U+FFFD instead of failing. Files that start with a UTF-16 BOM (Windows
/// Notepad's "Unicode") are decoded as UTF-16.
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        return decode_utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        return decode_utf16(rest, u16::from_be_bytes);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn decode_utf16(bytes: &[u8], unit_from_bytes: fn([u8; 2]) -> u16) -> String {
    // A dangling odd last byte cannot form a UTF-16 unit and is ignored.
    let (pairs, _odd_byte) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|pair| unit_from_bytes(*pair)).collect();
    String::from_utf16_lossy(&units)
}

/// Tidy whitespace the same way for every format:
/// - `\r\n` and `\r` become `\n`;
/// - trailing spaces/tabs are trimmed from every line;
/// - runs of blank lines collapse into one blank line (a paragraph break);
/// - blank lines at the start and end are removed.
///
/// Indentation at the start of lines is kept (it matters for code).
pub(crate) fn normalize_whitespace(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(text.len());
    let mut pending_blank_line = false;
    for line in text.split('\n') {
        let line = line.trim_end();
        if line.is_empty() {
            // Only remember the gap; it is written before the next non-blank line, so blank
            // lines at the very end are never written.
            pending_blank_line = !out.is_empty();
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
            if pending_blank_line {
                out.push('\n');
            }
        }
        pending_blank_line = false;
        out.push_str(line);
    }
    out
}

/// The longest prefix of `text` that is at most `max_bytes` long and ends on a character
/// boundary (so a multi-byte UTF-8 character is never cut in half).
pub(crate) fn truncate_to_bytes(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Longest heading text kept in a locator; longer headings are shortened with "…".
const MAX_HEADING_CHARS: usize = 120;

/// Citation locator for a heading: `"§ Heading"`, with inner whitespace collapsed.
/// Returns `None` when the heading has no visible text.
pub(crate) fn heading_locator(heading: &str) -> Option<String> {
    let words: Vec<&str> = heading.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    let heading = words.join(" ");
    if heading.chars().count() <= MAX_HEADING_CHARS {
        return Some(format!("§ {heading}"));
    }
    let short: String = heading.chars().take(MAX_HEADING_CHARS).collect();
    Some(format!("§ {}…", short.trim_end()))
}

/// Read everything from `reader`, but never more than `max_bytes`.
///
/// Returns `Ok(None)` when the data is longer than `max_bytes`. At most `max_bytes + 1`
/// bytes are ever read, so this is safe even for endless streams (e.g. a zip bomb), and it
/// does not trust any size that a file header *claims*.
pub(crate) fn read_capped(reader: impl Read, max_bytes: u64) -> std::io::Result<Option<Vec<u8>>> {
    let mut data = Vec::new();
    reader
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut data)?;
    if data.len() as u64 > max_bytes {
        return Ok(None);
    }
    Ok(Some(data))
}

/// Longest panic message copied into an error (panic messages can contain whole PDF objects).
const MAX_PANIC_MESSAGE_CHARS: usize = 200;

thread_local! {
    /// True while this thread runs parser code inside `catch_panic`.
    static CATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether a panic happening right now on this thread will be caught by `catch_panic` (a
/// parser crash on a bad file, reported as `ExtractError::Failed`). A process-wide panic hook
/// uses this to avoid reporting such expected panics as application crashes.
pub fn panic_is_expected() -> bool {
    CATCHING.with(|c| c.get())
}

/// Run third-party parsing code that may panic on malformed input, turning a panic into
/// `ExtractError::Failed` so one bad file cannot abort a whole sync.
///
/// Notes for maintainers:
/// - This only works while the workspace builds with `panic = "unwind"` (the default).
/// - Rust's default panic hook still prints the panic message to **stderr**. Extraction only
///   runs at sync time (CLI / desktop), never inside the stdio MCP server, whose stdout must
///   stay clean — so this is harmless noise, not a protocol problem.
pub(crate) fn catch_panic<T>(what: &str, f: impl FnOnce() -> T) -> Result<T, ExtractError> {
    let outer = CATCHING.with(|c| c.replace(true));
    let result = catch_unwind(AssertUnwindSafe(f));
    CATCHING.with(|c| c.set(outer));
    result.map_err(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown error".to_string());
        let message: String = message.chars().take(MAX_PANIC_MESSAGE_CHARS).collect();
        failed(format!("{what} parser crashed: {message}"))
    })
}

/// `12345` → `"12,345"` (for user-visible messages).
pub(crate) fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Human-readable size for messages: "200 MB", "1.5 MB", "900 KB", "12 bytes".
pub(crate) fn size_text(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes >= MB {
        let mb = bytes as f64 / MB as f64;
        if bytes.is_multiple_of(MB) {
            format!("{} MB", bytes / MB)
        } else {
            format!("{mb:.1} MB")
        }
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_text_strips_utf8_bom() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFhello"), "hello");
    }

    #[test]
    fn decode_text_is_lossy_for_invalid_utf8() {
        assert_eq!(
            decode_text(b"ok \xFF\xFE\xFD end"),
            "ok \u{FFFD}\u{FFFD}\u{FFFD} end"
        );
    }

    #[test]
    fn decode_text_handles_utf16_boms() {
        let le: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("Hi é".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(decode_text(&le), "Hi é");
        let be: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain("Hi é".encode_utf16().flat_map(u16::to_be_bytes))
            .collect();
        assert_eq!(decode_text(&be), "Hi é");
    }

    #[test]
    fn normalize_whitespace_collapses_blank_lines_and_trims() {
        let input = "\n\n  code line   \r\nnext\t\r\n\r\n\n \n\nafter\n\n\n";
        assert_eq!(normalize_whitespace(input), "  code line\nnext\n\nafter");
    }

    #[test]
    fn normalize_whitespace_of_blank_text_is_empty() {
        assert_eq!(normalize_whitespace(" \n\t\n  "), "");
        assert_eq!(normalize_whitespace(""), "");
    }

    #[test]
    fn truncate_to_bytes_respects_char_boundaries() {
        assert_eq!(truncate_to_bytes("héllo", 2), "h");
        assert_eq!(truncate_to_bytes("héllo", 3), "hé");
        assert_eq!(truncate_to_bytes("😀x", 3), "");
        assert_eq!(truncate_to_bytes("abc", 10), "abc");
    }

    #[test]
    fn heading_locator_collapses_whitespace_and_shortens() {
        assert_eq!(
            heading_locator("  Week\t1 \n Intro "),
            Some("§ Week 1 Intro".into())
        );
        assert_eq!(heading_locator(" \n "), None);
        let long = "x".repeat(500);
        let locator = heading_locator(&long).unwrap();
        assert!(locator.ends_with('…'));
        assert_eq!(locator.chars().count(), 2 + MAX_HEADING_CHARS + 1);
    }

    #[test]
    fn read_capped_stops_on_endless_input() {
        let endless = std::io::repeat(b'x');
        assert_eq!(read_capped(endless, 1000).unwrap(), None);
        assert_eq!(
            read_capped(&b"12345"[..], 5).unwrap(),
            Some(b"12345".to_vec())
        );
        assert_eq!(read_capped(&b"123456"[..], 5).unwrap(), None);
    }

    #[test]
    fn catch_panic_turns_panics_into_failed() {
        let result: Result<(), _> = catch_panic("demo", || panic!("boom"));
        match result {
            Err(ExtractError::Failed(message)) => assert!(message.contains("boom"), "{message}"),
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(catch_panic("demo", || 7).unwrap(), 7);
    }

    #[test]
    fn human_numbers_for_messages() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(5000), "5,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(size_text(200 * 1024 * 1024), "200 MB");
        assert_eq!(size_text(1536 * 1024), "1.5 MB");
        assert_eq!(size_text(2048), "2 KB");
        assert_eq!(size_text(12), "12 bytes");
    }
}
