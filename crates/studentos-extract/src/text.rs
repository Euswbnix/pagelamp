//! Plain text, source code and Markdown.
//!
//! Plain text and code become one segment without a locator. Markdown is split at ATX
//! headings of level 1–3 (`# `, `## `, `### `) that are outside fenced code blocks; each
//! section keeps its heading line and gets the locator `"§ Heading"`. Text before the first
//! heading has no locator. Setext headings (`Title` underlined with `===`) are not split on.

use crate::util::{decode_text, failed, heading_locator};
use crate::{ExtractError, Segment};

/// How much of the start of a file is checked for binary content.
const BINARY_SNIFF_BYTES: usize = 8 * 1024;

/// The whole file as a single segment.
///
/// Files that look binary (see [`looks_binary`]) are refused with `Failed` instead of being
/// indexed as garbage — e.g. an MPEG video with the `.ts` extension, which we otherwise
/// read as TypeScript.
pub(crate) fn plain_segments(bytes: &[u8]) -> Result<Vec<Segment>, ExtractError> {
    if looks_binary(bytes) {
        return Err(failed("file looks binary, not like text"));
    }
    Ok(vec![Segment {
        locator: None,
        text: decode_text(bytes),
    }])
}

/// Whether the file has a NUL byte near its start. Text files never do, except UTF-16 ones,
/// which are recognised by their byte-order mark (see `decode_text`).
fn looks_binary(bytes: &[u8]) -> bool {
    let has_utf16_bom = bytes.starts_with(b"\xFF\xFE") || bytes.starts_with(b"\xFE\xFF");
    let start = &bytes[..bytes.len().min(BINARY_SNIFF_BYTES)];
    !has_utf16_bom && start.contains(&0)
}

/// Split Markdown into one segment per level 1–3 heading section.
pub(crate) fn markdown_segments(text: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current = Segment {
        locator: None,
        text: String::new(),
    };
    let mut open_fence: Option<Fence> = None;

    for line in text.lines() {
        if let Some(fence) = &open_fence {
            if fence.is_closed_by(line) {
                open_fence = None;
            }
        } else if let Some(fence) = Fence::opened_by(line) {
            open_fence = Some(fence);
        } else if let Some(locator) = atx_heading(line).and_then(heading_locator) {
            let finished = std::mem::replace(
                &mut current,
                Segment {
                    locator: Some(locator),
                    text: String::new(),
                },
            );
            segments.push(finished);
        }
        current.text.push_str(line);
        current.text.push('\n');
    }
    segments.push(current);
    segments
}

/// Heading text of an ATX heading line of level 1–3, e.g. `"## Week 2 ##"` → `"Week 2"`.
///
/// Follows CommonMark: at most 3 spaces of indentation, the `#`s must be followed by a space,
/// a tab or the end of the line (`#hashtag` is not a heading), and an optional closing run of
/// `#`s is removed. Level 4+ headings return `None`, so they stay inside their section.
fn atx_heading(line: &str) -> Option<&str> {
    let unindented = line.trim_start_matches(' ');
    if line.len() - unindented.len() > 3 {
        return None;
    }
    let level = unindented.len() - unindented.trim_start_matches('#').len();
    if !(1..=3).contains(&level) {
        return None;
    }
    let rest = &unindented[level..];
    if !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
        return None;
    }
    let mut heading = rest.trim();
    // Optional closing sequence: "# Title ##" → "Title", but "# C#" stays "C#".
    let without_closing = heading.trim_end_matches('#');
    if without_closing.is_empty() || without_closing.ends_with([' ', '\t']) {
        heading = without_closing.trim_end();
    }
    Some(heading)
}

/// An open fenced code block (```` ``` ```` or `~~~`).
struct Fence {
    marker: char,
    len: usize,
}

impl Fence {
    /// The fence opened by `line`, if it is an opening code fence.
    fn opened_by(line: &str) -> Option<Fence> {
        let (marker, len, rest) = fence_run(line)?;
        // A backtick fence's info string may not contain backticks ("```x```" is inline code).
        if marker == '`' && rest.contains('`') {
            return None;
        }
        Some(Fence { marker, len })
    }

    /// Whether `line` closes this fence: same marker, at least as long, nothing after it.
    fn is_closed_by(&self, line: &str) -> bool {
        matches!(fence_run(line), Some((marker, len, rest))
            if marker == self.marker && len >= self.len && rest.trim().is_empty())
    }
}

/// `(marker, run length, rest of line)` when `line` starts (after ≤ 3 spaces) with at least
/// three backticks or tildes.
fn fence_run(line: &str) -> Option<(char, usize, &str)> {
    let unindented = line.trim_start_matches(' ');
    if line.len() - unindented.len() > 3 {
        return None;
    }
    let marker = unindented
        .chars()
        .next()
        .filter(|c| *c == '`' || *c == '~')?;
    let rest = unindented.trim_start_matches(marker);
    let len = unindented.len() - rest.len(); // the marker is ASCII, so bytes == chars
    (len >= 3).then_some((marker, len, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locators(segments: &[Segment]) -> Vec<Option<&str>> {
        segments.iter().map(|s| s.locator.as_deref()).collect()
    }

    #[test]
    fn plain_text_is_one_segment_without_locator() {
        let segments = plain_segments(b"\xEF\xBB\xBFprint('hi')\n").unwrap();
        assert_eq!(
            segments,
            vec![Segment {
                locator: None,
                text: "print('hi')\n".into()
            }]
        );
    }

    #[test]
    fn binary_content_is_refused_but_utf16_text_is_not() {
        let result = plain_segments(b"\x47\x00\x00\x10 video packet");
        assert!(matches!(result, Err(ExtractError::Failed(m)) if m.contains("binary")));
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("Hi".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(plain_segments(&utf16).unwrap()[0].text, "Hi");
        // Only the start of the file is checked.
        let mut late_nul = vec![b'a'; BINARY_SNIFF_BYTES];
        late_nul.push(0);
        assert!(plain_segments(&late_nul).is_ok());
    }

    #[test]
    fn markdown_splits_at_level_1_to_3_headings() {
        let md = "Intro text\n# Course DEMO101\nWelcome\n## Week 1 ##\nTopics\n### Reading\nCh. 1\n#### Detail\nstays in Reading\n";
        let segments = markdown_segments(md);
        assert_eq!(
            locators(&segments),
            vec![
                None,
                Some("§ Course DEMO101"),
                Some("§ Week 1"),
                Some("§ Reading")
            ]
        );
        assert_eq!(segments[0].text, "Intro text\n");
        assert_eq!(segments[2].text, "## Week 1 ##\nTopics\n");
        assert_eq!(
            segments[3].text,
            "### Reading\nCh. 1\n#### Detail\nstays in Reading\n"
        );
    }

    #[test]
    fn markdown_ignores_headings_inside_fenced_code() {
        let md = "# Real\n```python\n# a comment, not a heading\n```\n~~~~\n## also code\n~~~\nstill code\n~~~~\n## After\n";
        let segments = markdown_segments(md);
        assert_eq!(
            locators(&segments),
            vec![None, Some("§ Real"), Some("§ After")]
        );
        assert!(segments[1].text.contains("# a comment"));
        assert!(segments[1].text.contains("## also code"));
    }

    #[test]
    fn unclosed_fence_runs_to_end_of_document() {
        let segments = markdown_segments("# A\n```\n# not heading\n");
        assert_eq!(locators(&segments), vec![None, Some("§ A")]);
    }

    #[test]
    fn inline_backticks_do_not_open_a_fence() {
        let segments = markdown_segments("```x``` inline\n# Heading\n");
        assert_eq!(locators(&segments), vec![None, Some("§ Heading")]);
    }

    #[test]
    fn atx_heading_rules() {
        assert_eq!(atx_heading("# Title"), Some("Title"));
        assert_eq!(atx_heading("   ### Indented ok"), Some("Indented ok"));
        assert_eq!(atx_heading("    # code block"), None);
        assert_eq!(atx_heading("#hashtag"), None);
        assert_eq!(atx_heading("#### Level four"), None);
        assert_eq!(atx_heading("# C#"), Some("C#"));
        assert_eq!(atx_heading("#\tTabbed #"), Some("Tabbed"));
        assert_eq!(atx_heading("# ###"), Some(""));
        assert_eq!(atx_heading("#"), Some(""));
        assert_eq!(atx_heading("plain"), None);
    }

    #[test]
    fn empty_headings_do_not_split() {
        let segments = markdown_segments("before\n#\nafter\n");
        assert_eq!(locators(&segments), vec![None]);
    }

    #[test]
    fn crlf_lines_are_handled() {
        let segments = markdown_segments("a\r\n# B\r\nc\r\n");
        assert_eq!(locators(&segments), vec![None, Some("§ B")]);
        assert_eq!(segments[1].text, "# B\nc\n");
    }
}
