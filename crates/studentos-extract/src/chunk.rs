//! Splitting segments into search-sized chunks (see `chunk_segments` in `lib.rs`).
//!
//! All positions here are *character* indexes into a `Vec<char>`, so a cut can never land
//! inside a multi-byte UTF-8 character and every length is measured in characters.
//!
//! How one long text is cut (`chunk_spans`):
//! 1. A chunk starts at `start` and may end anywhere up to `start + max_chars`.
//! 2. We look for the *latest* good cut in the second half of that window, trying in order:
//!    a paragraph break (blank line), the end of a sentence, any whitespace. If none exists
//!    (e.g. one very long word, or CJK text without spaces) we cut hard at the window end.
//!    Only looking in the second half keeps chunks reasonably long.
//! 3. The next chunk starts about `max_chars / 10` characters before the cut (moved forward
//!    to a word start when possible), so neighbouring chunks overlap a little and a sentence
//!    on the boundary can still be found. The start always moves forward by at least one
//!    character, so the loop always terminates.

use crate::util::normalize_whitespace;
use crate::{ChunkText, Segment};

/// See `crate::chunk_segments` for the contract. `max_chars == 0` is treated as 1.
pub(crate) fn chunk_segments(segments: &[Segment], max_chars: usize) -> Vec<ChunkText> {
    let max_chars = max_chars.max(1);
    let mut chunks = Vec::new();
    for segment in segments {
        let text = normalize_whitespace(&segment.text);
        let chars: Vec<char> = text.chars().collect();
        for (start, end) in chunk_spans(&chars, max_chars) {
            let piece: String = chars[start..end].iter().collect();
            let piece = piece.trim();
            if !piece.is_empty() {
                chunks.push(ChunkText {
                    locator: segment.locator.clone(),
                    text: piece.to_string(),
                });
            }
        }
    }
    chunks
}

/// Character ranges `start..end` of the chunks of `chars`; each is at most `max_chars` long.
/// `max_chars` must be at least 1.
fn chunk_spans(chars: &[char], max_chars: usize) -> Vec<(usize, usize)> {
    let overlap = max_chars / 10;
    let mut spans = Vec::new();
    let mut start = skip_whitespace(chars, 0);
    while start < chars.len() {
        if chars.len() - start <= max_chars {
            spans.push((start, chars.len()));
            break;
        }
        let end = best_cut(chars, start, max_chars);
        spans.push((start, end));
        start = skip_whitespace(chars, next_start(chars, start, end, overlap));
    }
    spans
}

/// Where to end the chunk that starts at `start`, given that more than `max_chars`
/// characters remain. The result is in `start + 1 ..= start + max_chars`.
fn best_cut(chars: &[char], start: usize, max_chars: usize) -> usize {
    let window_end = start + max_chars; // < chars.len(), so chars[window_end] exists
    let earliest = start + (max_chars / 2).max(1);
    let latest_cut = |is_cut: fn(&[char], usize) -> bool| {
        (earliest..=window_end)
            .rev()
            .find(|&end| is_cut(chars, end))
    };
    latest_cut(is_paragraph_break)
        .or_else(|| latest_cut(is_sentence_end))
        .or_else(|| latest_cut(is_whitespace_at))
        .unwrap_or(window_end)
}

/// Start of the chunk after `start..end`: about `overlap` characters before `end`,
/// preferably at the start of a word, and always after `start`.
fn next_start(chars: &[char], start: usize, end: usize, overlap: usize) -> usize {
    if overlap == 0 {
        return end;
    }
    let target = end.saturating_sub(overlap).max(start + 1);
    (target..end)
        .find(|&i| chars[i - 1].is_whitespace() && !chars[i].is_whitespace())
        .unwrap_or(target)
}

fn skip_whitespace(chars: &[char], mut index: usize) -> usize {
    while index < chars.len() && chars[index].is_whitespace() {
        index += 1;
    }
    index
}

/// A chunk ending at `end` stops right before a blank line (`"\n\n"`).
fn is_paragraph_break(chars: &[char], end: usize) -> bool {
    chars.get(end) == Some(&'\n') && chars.get(end + 1) == Some(&'\n')
}

/// A chunk ending at `end` ends with sentence punctuation (optionally followed by a closing
/// quote or bracket). ASCII `.`, `!`, `?` also need whitespace after them, so "3.14" and
/// "example.com" are not sentence ends; CJK full-width punctuation does not.
fn is_sentence_end(chars: &[char], end: usize) -> bool {
    let mut last = end;
    if last > 0
        && matches!(
            chars[last - 1],
            '"' | '\'' | ')' | ']' | '”' | '’' | '」' | '）'
        )
    {
        last -= 1;
    }
    if last == 0 {
        return false;
    }
    match chars[last - 1] {
        '。' | '！' | '？' => true,
        '.' | '!' | '?' => chars.get(end).is_none_or(|c| c.is_whitespace()),
        _ => false,
    }
}

/// A chunk ending at `end` stops right before a whitespace character.
fn is_whitespace_at(chars: &[char], end: usize) -> bool {
    chars.get(end).is_some_and(|c| c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(locator: Option<&str>, text: &str) -> Segment {
        Segment {
            locator: locator.map(str::to_string),
            text: text.to_string(),
        }
    }

    fn texts(chunks: &[ChunkText]) -> Vec<&str> {
        chunks.iter().map(|c| c.text.as_str()).collect()
    }

    #[test]
    fn short_segments_are_not_merged_and_keep_locators() {
        let segments = [
            segment(Some("p. 1"), "First page."),
            segment(Some("p. 2"), "  \n\n "),
            segment(Some("p. 3"), "Third page."),
        ];
        let chunks = chunk_segments(&segments, 100);
        assert_eq!(
            chunks,
            vec![
                ChunkText {
                    locator: Some("p. 1".into()),
                    text: "First page.".into()
                },
                ChunkText {
                    locator: Some("p. 3".into()),
                    text: "Third page.".into()
                },
            ]
        );
    }

    #[test]
    fn whitespace_is_normalised() {
        let chunks = chunk_segments(&[segment(None, "a  \r\n\r\n\r\n\n b\t\n")], 100);
        assert_eq!(texts(&chunks), vec!["a\n\n b"]);
    }

    #[test]
    fn prefers_paragraph_breaks() {
        let text = "First paragraph has words. It goes on.\n\nSecond paragraph is here and it is long enough.";
        let chunks = chunk_segments(&[segment(Some("§ A"), text)], 50);
        assert_eq!(chunks[0].text, "First paragraph has words. It goes on.");
        assert!(chunks.iter().all(|c| c.locator.as_deref() == Some("§ A")));
        assert!(chunks.last().unwrap().text.ends_with("long enough."));
    }

    #[test]
    fn prefers_sentence_end_over_plain_whitespace() {
        let text = "Alpha beta gamma delta. Epsilon zeta eta theta iota kappa lambda mu nu xi.";
        let chunks = chunk_segments(&[segment(None, text)], 40);
        assert_eq!(chunks[0].text, "Alpha beta gamma delta.");
    }

    #[test]
    fn decimal_points_are_not_sentence_ends() {
        let chars: Vec<char> = "pi is 3.14 ok".chars().collect();
        assert!(!is_sentence_end(&chars, 8)); // "pi is 3." | "14 ok"
        let chars: Vec<char> = "He said \"stop.\" Then".chars().collect();
        assert!(is_sentence_end(&chars, 15)); // after the closing quote
    }

    #[test]
    fn falls_back_to_whitespace_then_hard_split() {
        let chunks = chunk_segments(&[segment(None, "aaaa bbbb cccc dddd")], 10);
        assert_eq!(chunks[0].text, "aaaa bbbb");
        let chunks = chunk_segments(&[segment(None, &"x".repeat(25))], 10);
        assert_eq!(
            texts(&chunks),
            vec!["x".repeat(10), "x".repeat(10), "x".repeat(7)]
        );
    }

    #[test]
    fn consecutive_chunks_overlap_by_about_a_tenth() {
        let words: Vec<String> = (0..400).map(|i| format!("w{i:03}")).collect();
        let text = words.join(" ");
        let chunks = chunk_segments(&[segment(None, &text)], 200);
        assert!(chunks.len() > 5);
        for pair in chunks.windows(2) {
            let last_word_of_first = pair[0].text.split(' ').next_back().unwrap();
            assert!(
                pair[1].text.contains(last_word_of_first),
                "no overlap between {:?} and {:?}",
                pair[0].text,
                pair[1].text
            );
            let first_word_of_second = pair[1].text.split(' ').next().unwrap();
            let overlap_start = pair[0].text.find(first_word_of_second).unwrap();
            let overlap_chars = pair[0].text[overlap_start..].chars().count();
            assert!((5..=25).contains(&overlap_chars), "overlap {overlap_chars}");
        }
    }

    #[test]
    fn counts_characters_not_bytes() {
        let text = "é".repeat(30);
        let chunks = chunk_segments(&[segment(None, &text)], 30);
        assert_eq!(texts(&chunks), vec![text.as_str()]);
        let cjk = "中文字符测试".repeat(10); // 60 chars, 180 bytes, no spaces
        let chunks = chunk_segments(&[segment(None, &cjk)], 25);
        assert!(chunks.iter().all(|c| c.text.chars().count() <= 25));
        assert!(chunks.len() >= 3);
    }

    #[test]
    fn cjk_sentence_punctuation_is_a_boundary() {
        let text = "第一句话在这里。第二句话也在这里但是更长一些。";
        let chunks = chunk_segments(&[segment(None, text)], 15);
        assert_eq!(chunks[0].text, "第一句话在这里。");
    }

    #[test]
    fn tiny_and_zero_max_chars_terminate() {
        let text = "ab 😀 c.\n\nd";
        for max in 0..6 {
            let chunks = chunk_segments(&[segment(None, text)], max);
            assert!(!chunks.is_empty());
            assert!(chunks.iter().all(|c| c.text.chars().count() <= max.max(1)));
        }
    }

    #[test]
    fn empty_input_gives_no_chunks() {
        assert!(chunk_segments(&[], 10).is_empty());
        assert!(chunk_segments(&[segment(None, "")], 10).is_empty());
    }

    /// Tiny deterministic pseudo-random generator (a linear congruential generator), so the
    /// stress test is reproducible without the `rand` crate.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    #[test]
    fn stress_invariants_hold_for_random_text() {
        const ALPHABET: &[char] = &[
            'a', 'b', 'c', 'x', ' ', ' ', '\n', '\n', '\t', '.', '!', '?', '"', '。', '中', '文',
            '😀', 'é', '\r', '-',
        ];
        let mut rng = Lcg(42);
        for _ in 0..3000 {
            let len = rng.below(400);
            let text: String = (0..len)
                .map(|_| ALPHABET[rng.below(ALPHABET.len())])
                .collect();
            // Half of the runs use tiny chunk sizes (1..=20), the rest 1..=120.
            let max_limit = if rng.below(2) == 0 { 20 } else { 120 };
            let max = 1 + rng.below(max_limit);
            let locator = Some(format!("p. {}", rng.below(9)));
            let chunks = chunk_segments(
                &[Segment {
                    locator: locator.clone(),
                    text: text.clone(),
                }],
                max,
            );

            for chunk in &chunks {
                assert!(!chunk.text.is_empty());
                assert_eq!(chunk.text, chunk.text.trim(), "untrimmed chunk");
                assert!(
                    chunk.text.chars().count() <= max,
                    "chunk longer than {max}: {:?}",
                    chunk.text
                );
                assert_eq!(chunk.locator, locator);
            }

            // Spans: forward progress, bounded length, and nothing but whitespace is lost.
            let normalized = normalize_whitespace(&text);
            let chars: Vec<char> = normalized.chars().collect();
            let spans = chunk_spans(&chars, max);
            let mut covered = vec![false; chars.len()];
            let mut previous_start = None;
            for &(start, end) in &spans {
                assert!(start < end && end - start <= max);
                assert!(previous_start.is_none_or(|p| start > p), "no progress");
                previous_start = Some(start);
                covered[start..end].iter_mut().for_each(|c| *c = true);
            }
            for (i, c) in chars.iter().enumerate() {
                assert!(
                    covered[i] || c.is_whitespace(),
                    "lost {c:?} in {normalized:?}"
                );
            }
        }
    }

    #[test]
    fn large_input_is_fast_enough() {
        // Guards against accidental quadratic behaviour: ~2 MB of text must chunk quickly.
        let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(36_000);
        let started = std::time::Instant::now();
        let chunks = chunk_segments(&[segment(None, &text)], crate::DEFAULT_CHUNK_CHARS);
        assert!(chunks.len() > 1000);
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
    }
}
