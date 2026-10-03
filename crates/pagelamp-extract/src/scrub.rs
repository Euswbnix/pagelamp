//! Access parameters in link addresses.
//!
//! A link address in LMS HTML can carry a parameter that opens the file for whoever has the
//! address (`verifier`, `sf_verifier`, `access_token`). Such a parameter must never reach
//! stored text, the search index or anything PageLamp gives out. [`scrub_text`] removes them
//! from every address in a text:
//!
//! - an address to a file (`…/files/<id>`, also with `/download` or `/preview`) loses its
//!   whole query;
//! - any other address loses just those parameters.
//!
//! It is applied to all extracted text before it is stored, and again where text leaves
//! PageLamp, which covers text stored by an earlier version. Applying it twice changes
//! nothing more. One linear pass, no regular expressions.

use std::borrow::Cow;

/// The rules' version. It is part of the hash of indexed HTML, so text stored under older
/// rules is extracted again; raise it when the rules change.
pub const VERSION: u32 = 1;

/// Parameters that give access by themselves.
const ACCESS_PARAMETERS: [&str; 3] = ["verifier", "sf_verifier", "access_token"];

/// `text` without access parameters in the addresses it contains. Borrowed when there is
/// nothing to remove.
pub fn scrub_text(text: &str) -> Cow<'_, str> {
    if !may_need_scrubbing(text) {
        return Cow::Borrowed(text);
    }
    let mut out = String::new();
    let mut copied = 0; // `text[..copied]` is already in `out`
    let mut word_start = None;
    let scrub_word = |start: usize, end: usize, out: &mut String, copied: &mut usize| {
        if let Some(clean) = scrub_address(&text[start..end]) {
            out.push_str(&text[*copied..start]);
            out.push_str(&clean);
            *copied = end;
        }
    };
    for (at, c) in text.char_indices() {
        if ends_an_address(c) {
            if let Some(start) = word_start.take() {
                scrub_word(start, at, &mut out, &mut copied);
            }
        } else if word_start.is_none() {
            word_start = Some(at);
        }
    }
    if let Some(start) = word_start {
        scrub_word(start, text.len(), &mut out, &mut copied);
    }
    if copied == 0 {
        return Cow::Borrowed(text);
    }
    out.push_str(&text[copied..]);
    Cow::Owned(out)
}

/// A cheap test that lets almost every text through untouched.
fn may_need_scrubbing(text: &str) -> bool {
    text.contains('?')
        && (contains_ignoring_case(text, "/files/")
            || ACCESS_PARAMETERS
                .iter()
                .any(|name| contains_ignoring_case(text, name)))
}

fn contains_ignoring_case(text: &str, needle: &str) -> bool {
    let (text, needle) = (text.as_bytes(), needle.as_bytes());
    text.windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// Characters an address doesn't contain as it appears in text: whitespace, quotes and
/// brackets (extracted HTML writes a link as `text (address)`), and the backslash, which in
/// JSON output escapes the quote or line break that follows an address.
fn ends_an_address(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '`' | '\\'
        )
}

/// The marks the search index puts around a match in a snippet. They can sit inside an
/// address (`?«verifier»=…`), so a parameter's name is compared without them.
fn is_search_mark(c: char) -> bool {
    matches!(c, '«' | '»')
}

/// `word` without its access parameters, when it is an address that has any (or an address to
/// a file with a query). `None`: nothing to change.
fn scrub_address(word: &str) -> Option<String> {
    let (before, rest) = word.split_once('?')?;
    let (query, fragment) = match rest.split_once('#') {
        Some((query, fragment)) => (query, Some(fragment)),
        None => (rest, None),
    };
    let kept: Vec<&str> = if is_file_address(before) {
        Vec::new()
    } else {
        query
            .split('&')
            .filter(|pair| !is_access_parameter(pair))
            .collect()
    };
    if kept.len() == query.split('&').count() {
        return None;
    }
    let mut clean = before.to_string();
    if !kept.is_empty() {
        clean.push('?');
        clean.push_str(&kept.join("&"));
    }
    if let Some(fragment) = fragment {
        clean.push('#');
        clean.push_str(fragment);
    }
    Some(clean)
}

/// `name=value` (or a bare `name`) naming an access parameter. HTML that wasn't decoded
/// writes the separator as `&amp;`, which leaves `amp;` in front of the name.
fn is_access_parameter(pair: &str) -> bool {
    let pair = pair.strip_prefix("amp;").unwrap_or(pair);
    let name = pair.split_once('=').map_or(pair, |(name, _)| name);
    let name: String = name.chars().filter(|c| !is_search_mark(*c)).collect();
    ACCESS_PARAMETERS
        .iter()
        .any(|access| name.eq_ignore_ascii_case(access))
}

/// Whether the part of an address before its query ends in `/files/<id>`, `/files/<id>/download`
/// or `/files/<id>/preview` (an id is digits, or digits with `~` for another shard).
fn is_file_address(before_query: &str) -> bool {
    let lower = before_query.to_ascii_lowercase();
    let Some(at) = lower.rfind("/files/") else {
        return false;
    };
    let after = &lower[at + "/files/".len()..];
    let id_len = after
        .bytes()
        .take_while(|b| b.is_ascii_digit() || *b == b'~')
        .count();
    id_len > 0 && matches!(&after[id_len..], "" | "/" | "/download" | "/preview")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_address_loses_its_whole_query() {
        for (address, clean) in [
            (
                "https://lms.example.edu/courses/101/files/501/download?verifier=DEMOSECRET&wrap=1",
                "https://lms.example.edu/courses/101/files/501/download",
            ),
            (
                "https://lms.example.edu/files/501/preview?verifier=DEMOSECRET",
                "https://lms.example.edu/files/501/preview",
            ),
            (
                "/courses/101/files/1234~501?wrap=1",
                "/courses/101/files/1234~501",
            ),
            (
                "https://lms.example.edu/users/7/files/501/download?download_frd=1&sf_verifier=DEMOSECRET#page=2",
                "https://lms.example.edu/users/7/files/501/download#page=2",
            ),
        ] {
            assert_eq!(scrub_text(address), clean, "{address}");
            let in_text = format!("Week 3 slides ({address}), due Friday.");
            assert_eq!(
                scrub_text(&in_text),
                format!("Week 3 slides ({clean}), due Friday.")
            );
        }
    }

    #[test]
    fn other_addresses_lose_only_the_access_parameters() {
        for (address, clean) in [
            (
                "https://lms.example.edu/courses/101/pages/week-3?module_item_id=9&verifier=DEMOSECRET",
                "https://lms.example.edu/courses/101/pages/week-3?module_item_id=9",
            ),
            (
                "https://media.example.edu/watch?access_token=DEMOSECRET&t=30#intro",
                "https://media.example.edu/watch?t=30#intro",
            ),
            (
                "https://media.example.edu/watch?access_token=DEMOSECRET",
                "https://media.example.edu/watch",
            ),
            // Any letter case, an undecoded `&amp;`, and a name with no value.
            (
                "https://media.example.edu/watch?a=1&amp;Verifier=DEMOSECRET&amp;b=2&SF_VERIFIER",
                "https://media.example.edu/watch?a=1&amp;b=2",
            ),
        ] {
            assert_eq!(scrub_text(address), clean, "{address}");
        }
    }

    #[test]
    fn several_addresses_in_one_text_and_the_search_marks() {
        let text = "See notes (https://lms.example.edu/files/1/download?verifier=ONE) and\n\
                    the video https://media.example.edu/v?access_token=TWO&t=5 \"today\".";
        assert_eq!(
            scrub_text(text),
            "See notes (https://lms.example.edu/files/1/download) and\n\
             the video https://media.example.edu/v?t=5 \"today\"."
        );
        // A search snippet marks the match, inside the address: the parameter still goes.
        for snippet in [
            "…download?«verifier»=DEMOSECRET&wrap=1 and more…",
            "…watch?t=5&verifier=«DEMOSECRET» and more…",
        ] {
            assert!(!scrub_text(snippet).contains("DEMOSECRET"), "{snippet}");
        }
        assert_eq!(
            scrub_text("…watch?t=5&«verifier»=DEMOSECRET and more…"),
            "…watch?t=5 and more…"
        );
        let twice = scrub_text(text).into_owned();
        assert_eq!(scrub_text(&twice), twice);
    }

    #[test]
    fn json_stays_json() {
        // An address right before an escaped quote or line break of a JSON string.
        let value = serde_json::json!({
            "text": "Open \"https://lms.example.edu/files/1/download?verifier=DEMOSECRET\" now\nor https://media.example.edu/v?access_token=DEMOSECRET\nlater",
        });
        let json = serde_json::to_string(&value).unwrap();
        let clean = scrub_text(&json);
        assert!(!clean.contains("DEMOSECRET"));
        let back: serde_json::Value = serde_json::from_str(&clean).unwrap();
        assert_eq!(
            back["text"],
            "Open \"https://lms.example.edu/files/1/download\" now\nor https://media.example.edu/v\nlater"
        );
    }

    #[test]
    fn text_without_access_parameters_is_borrowed_and_unchanged() {
        for text in [
            "",
            "Plain text with a question? Yes.",
            "The verifier of a proof checks it; an access_token is a credential.",
            "https://lms.example.edu/courses/101/pages/week-3?module_item_id=9",
            "https://lms.example.edu/courses/101/files",
            "https://lms.example.edu/courses/101/files/folder/week?sort=name",
            "Which /files/ are due? All of them.",
            "x = verifier == 3 ? a : b",
        ] {
            assert!(matches!(scrub_text(text), Cow::Borrowed(_)), "{text}");
        }
    }
}
