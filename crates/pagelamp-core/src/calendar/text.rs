//! A material's text for quote checks (docs/design/v0.3-course-calendar.md §7.5 V3): rebuilt
//! from its stored chunks with the chunk overlap removed, and compared in a normalised form.
//!
//! How chunks overlap (read from `pagelamp_extract::chunk`, which `ingest` calls with
//! `DEFAULT_CHUNK_CHARS` = 1800):
//! - every extracted segment (a page, a slide, a notebook cell, an HTML section) is chunked on
//!   its own; chunks never span segments and carry the segment's locator;
//! - within one segment, the next chunk starts about `max_chars / 10` characters (180) before
//!   the previous chunk's cut, moved forward to the start of a word, so the start of a chunk
//!   repeats the end of the previous one (at most 180 characters; exactly 180 in text without
//!   spaces);
//! - chunk texts are trimmed, and whitespace was normalised before chunking.
//!
//! So the rule implemented here: consecutive chunks with the same locator are joined after
//! removing the longest prefix of the later chunk (at most `MAX_OVERLAP_CHARS`, at least
//! `MIN_OVERLAP_CHARS` characters) that equals a suffix of the text built so far; chunks with
//! another locator (or no such overlap: a new segment with the same locator) start a new part.
//! Parts keep their locator, so a found quote can cite "p. 2".
//!
//! Matching form (V3): NFKC, lowercase, every run of whitespace one space, all dashes '-' and
//! all quotation marks straight quotes.

use unicode_normalization::UnicodeNormalization;

use crate::model::Chunk;

/// Longest overlap removed between chunks: `DEFAULT_CHUNK_CHARS / 10`.
pub const MAX_OVERLAP_CHARS: usize = pagelamp_extract::DEFAULT_CHUNK_CHARS / 10;
/// Shorter common text between two chunks is a coincidence, not the chunker's overlap.
pub const MIN_OVERLAP_CHARS: usize = 20;

/// A stretch of a material's text under one locator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextPart {
    pub locator: Option<String>,
    pub text: String,
}

/// The material's text from its chunks (in `ord` order), overlap removed.
pub fn rebuild_parts(chunks: &[Chunk]) -> Vec<TextPart> {
    let mut ordered: Vec<&Chunk> = chunks.iter().collect();
    ordered.sort_by_key(|chunk| chunk.ord);
    let mut parts: Vec<TextPart> = Vec::new();
    for chunk in ordered {
        if let Some(last) = parts.last_mut()
            && last.locator == chunk.locator
            && let Some(overlap) = overlap_chars(&last.text, &chunk.text)
        {
            let rest: String = chunk.text.chars().skip(overlap).collect();
            last.text.push_str(&rest);
            continue;
        }
        parts.push(TextPart {
            locator: chunk.locator.clone(),
            text: chunk.text.clone(),
        });
    }
    parts
}

/// The length in characters of the longest prefix of `next` that `built` ends with, within
/// `MIN_OVERLAP_CHARS..=MAX_OVERLAP_CHARS`.
fn overlap_chars(built: &str, next: &str) -> Option<usize> {
    let next_chars: Vec<char> = next.chars().collect();
    let longest = MAX_OVERLAP_CHARS.min(next_chars.len());
    (MIN_OVERLAP_CHARS..=longest).rev().find(|&k| {
        let prefix: String = next_chars[..k].iter().collect();
        built.ends_with(&prefix)
    })
}

/// The matching form of `text` (see the module docs).
pub fn match_form(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.nfkc().flat_map(char::to_lowercase) {
        let c = match c {
            '\u{2010}'..='\u{2015}' | '\u{2212}' | '\u{fe58}' | '\u{fe63}' | '\u{ff0d}' => '-',
            '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}' | '\u{2032}' | '`' | '\u{00b4}' => {
                '\''
            }
            '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{201f}' | '\u{2033}' | '«' | '»' | '「'
            | '」' => '"',
            other => other,
        };
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(c);
    }
    out
}

/// Where a quote was found: the part (for its locator), or across parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuoteMatch {
    /// The part holding the whole quote; None when it spans parts (e.g. two pages).
    pub part: Option<usize>,
}

/// Whether `quote` is in `parts` (V3). Parts are joined by a space for quotes that span them.
pub fn find_quote(parts: &[TextPart], quote: &str) -> Option<QuoteMatch> {
    let needle = match_form(quote);
    if needle.is_empty() {
        return None;
    }
    if let Some(index) = parts
        .iter()
        .position(|part| match_form(&part.text).contains(&needle))
    {
        return Some(QuoteMatch { part: Some(index) });
    }
    let joined = parts
        .iter()
        .map(|part| match_form(&part.text))
        .collect::<Vec<_>>()
        .join(" ");
    joined
        .contains(&needle)
        .then_some(QuoteMatch { part: None })
}

/// Where `quote` starts in the parts joined by a space, in matching-form characters (bytes of
/// the matching form), for "is the header before the row" checks.
pub fn quote_position(parts: &[TextPart], quote: &str) -> Option<usize> {
    let needle = match_form(quote);
    if needle.is_empty() {
        return None;
    }
    let joined = parts
        .iter()
        .map(|part| match_form(&part.text))
        .collect::<Vec<_>>()
        .join(" ");
    joined.find(&needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pagelamp_extract::{DEFAULT_CHUNK_CHARS, Segment, chunk_segments};

    /// Store-shaped chunks of `segments`, exactly as `ingest` makes them.
    fn stored(segments: &[Segment]) -> Vec<Chunk> {
        (0..)
            .zip(chunk_segments(segments, DEFAULT_CHUNK_CHARS))
            .map(|(ord, chunk)| Chunk {
                material_id: "m".into(),
                ord,
                locator: chunk.locator,
                text: chunk.text,
            })
            .collect()
    }

    fn segment(locator: &str, text: &str) -> Segment {
        Segment {
            locator: Some(locator.into()),
            text: text.into(),
        }
    }

    /// A long synthetic outline page: numbered sentences, so any text is unique.
    fn long_page(sentences: usize) -> String {
        (0..sentences)
            .map(|i| format!("Sentence {i} of the synthetic outline talks about topic {i}."))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn rebuilding_removes_the_chunk_overlap() {
        let page = long_page(200);
        let chunks = stored(&[segment("p. 1", &page)]);
        assert!(chunks.len() > 4, "several chunks");
        let parts = rebuild_parts(&chunks);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].locator.as_deref(), Some("p. 1"));
        assert_eq!(parts[0].text, page);
    }

    #[test]
    fn rebuilding_handles_text_without_spaces() {
        let page: String = (0..900)
            .map(|i| char::from_u32(0x4e00 + (i % 500) as u32).unwrap())
            .collect::<String>()
            .repeat(3);
        let parts = rebuild_parts(&stored(&[segment("p. 2", &page)]));
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].text, page);
    }

    #[test]
    fn segments_stay_apart_and_keep_their_locators() {
        let chunks = stored(&[
            segment("p. 1", &long_page(60)),
            segment("p. 2", "Reading week: Oct 26–30."),
            segment("p. 3", &long_page(60)),
        ]);
        let parts = rebuild_parts(&chunks);
        let locators: Vec<Option<&str>> = parts.iter().map(|p| p.locator.as_deref()).collect();
        assert_eq!(locators, [Some("p. 1"), Some("p. 2"), Some("p. 3")]);
        let found = find_quote(&parts, "reading week: oct 26-30").unwrap();
        assert_eq!(found.part, Some(1));
    }

    /// CAL-39 (the chunk part): a 250-character quote across a chunk boundary is found.
    #[test]
    fn a_quote_across_a_chunk_boundary_is_found() {
        let page = long_page(200);
        let chunks = stored(&[segment("p. 4", &page)]);
        let first_end = chunks[0].text.chars().count();
        let chars: Vec<char> = page.chars().collect();
        // From before the second chunk's start (≤ 180 characters before the cut) to after the
        // first chunk's end.
        let quote: String = chars[first_end - 220..first_end + 30].iter().collect();
        assert_eq!(quote.chars().count(), 250);
        // Neither chunk holds all of it…
        assert!(chunks.iter().all(|c| !c.text.contains(&quote)));
        // …the rebuilt text does.
        let parts = rebuild_parts(&chunks);
        assert_eq!(
            find_quote(&parts, &quote),
            Some(QuoteMatch { part: Some(0) })
        );
    }

    #[test]
    fn matching_ignores_case_width_whitespace_dashes_and_quotes() {
        let parts = vec![TextPart {
            locator: None,
            text: "Reading Week:\n  Oct 26\u{2013}30 (\u{201c}no classes\u{201d})".into(),
        }];
        assert!(find_quote(&parts, "reading week: oct 26-30 (\"no classes\")").is_some());
        assert!(find_quote(&parts, "ＲＥＡＤＩＮＧ WEEK").is_some());
        assert!(find_quote(&parts, "reading week: oct 27").is_none());
        assert!(find_quote(&parts, "   ").is_none());
    }

    #[test]
    fn a_quote_spanning_two_pages_is_found_without_a_locator() {
        let parts = vec![
            TextPart {
                locator: Some("p. 1".into()),
                text: "Final exam period:".into(),
            },
            TextPart {
                locator: Some("p. 2".into()),
                text: "Dec 10-22".into(),
            },
        ];
        assert_eq!(
            find_quote(&parts, "final exam period: dec 10-22"),
            Some(QuoteMatch { part: None })
        );
    }

    #[test]
    fn short_common_text_is_not_an_overlap() {
        // Two separate segments with the same locator that happen to share a few characters.
        let chunks = vec![
            Chunk {
                material_id: "m".into(),
                ord: 0,
                locator: None,
                text: "Week 1: introduction.".into(),
            },
            Chunk {
                material_id: "m".into(),
                ord: 1,
                locator: None,
                text: "introduction. Week 2".into(),
            },
        ];
        let parts = rebuild_parts(&chunks);
        assert_eq!(parts.len(), 2);
    }
}
