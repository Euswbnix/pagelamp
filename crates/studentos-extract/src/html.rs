//! HTML (LMS pages and `.html` files) → text segments, using `html2text`.
//!
//! `html2text` already drops `<script>`, `<style>` and `<head>`. We give it our own
//! [`TextDecorator`] so that:
//! - links render inline as `text (url)` (no footnotes; `#anchor` and `javascript:` links
//!   keep just their text);
//! - `<em>`, `<strong>`, `<code>` render as plain text (no markdown-style `*`/`` ` ``);
//! - every h1–h3 heading line is prefixed with a marker character plus a short id that is
//!   unique to that heading element, which we then use to split the rendered text into one
//!   segment per heading (locator `"§ Heading"`); h4–h6 are plain lines inside the current
//!   section. The id is what tells "one heading broken over two lines by `<br>`" (same id on
//!   both lines) from "two headings on neighbouring lines", which happens for headings in
//!   consecutive list items or table cells (different ids).
//! - the ` (url)` suffix of a link is wrapped in another marker character, so a heading that
//!   is a link gets a clean locator (`"§ Week 3 slides"`) while its text keeps the URL.
//!
//! The markers and id digits are Unicode private-use characters (U+E000–U+E1FF) that never
//! reach the output. Literal copies in the input are removed first; one written as an HTML
//! entity (`&#xE000;`) could at worst cause an extra split.
//!
//! Depth guard: html2text's rendering time grows quadratically with tag nesting depth (a
//! hostile page nested 100,000 deep takes minutes). Real LMS pages nest a few dozen levels, so
//! input nested deeper than [`MAX_RENDER_DEPTH`] is instead converted by [`plain_text`], a
//! simple linear tag stripper (no heading split, no link URLs).

use std::cell::Cell;
use std::rc::Rc;

use html2text::render::TextDecorator;

use crate::Segment;
use crate::util::{catch_panic, heading_locator};

/// Marks the start of an h1–h3 heading line in `html2text`'s output (private-use character).
/// It is followed by the heading's id: [`HEADING_ID_DIGITS`] private-use characters.
const HEADING_MARK: char = '\u{E000}';

/// Written before and after the ` (url)` suffix of a link (private-use character).
const LINK_MARK: char = '\u{E001}';

/// Heading ids are written as base-256 "digits" U+E100 + 0..=255.
const HEADING_ID_DIGIT_BASE: u32 = 0xE100;

/// Digits per heading id: one per byte of the `u16` id. The id must have a fixed length:
/// `html2text` asks for a heading's prefix twice (once to measure it) and requires both
/// answers to be equally long. 65,536 ids exist before they repeat; only neighbouring lines
/// are compared, so a repeat is harmless.
const HEADING_ID_DIGITS: usize = std::mem::size_of::<u16>();

/// Whether `c` is one of our marker or heading-id characters.
fn is_marker_char(c: char) -> bool {
    ('\u{E000}'..='\u{E1FF}').contains(&c)
}

fn strip_markers(text: &str) -> String {
    text.chars().filter(|c| !is_marker_char(*c)).collect()
}

/// The prefix written before each line of the heading with number `id`.
fn heading_prefix(id: u16) -> String {
    let digits = id.to_be_bytes().map(|byte| {
        char::from_u32(HEADING_ID_DIGIT_BASE + u32::from(byte)).unwrap_or(HEADING_MARK)
    });
    std::iter::once(HEADING_MARK).chain(digits).collect()
}

/// Render width. Very wide, so paragraphs stay on one line instead of being wrapped.
const RENDER_WIDTH: usize = 1_000_000;

/// Convert HTML to segments split at h1–h3 headings. Text before the first heading has no
/// locator; each heading section starts with the heading text itself. Never fails: HTML that
/// cannot be rendered yields no segments.
pub(crate) fn segments(html: &str) -> Vec<Segment> {
    if nesting_depth(html) > MAX_RENDER_DEPTH {
        let text = plain_text(html);
        return if text.trim().is_empty() {
            Vec::new()
        } else {
            vec![Segment {
                locator: None,
                text,
            }]
        };
    }
    match render(&strip_markers(html)) {
        Some(text) => split_at_headings(&text),
        None => Vec::new(),
    }
}

/// Deepest tag nesting html2text may render (see the module docs).
pub(crate) const MAX_RENDER_DEPTH: usize = 400;

/// Elements that never contain anything (they don't add nesting).
const VOID_ELEMENTS: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Upper bound of the tag nesting depth, in one linear pass: `<x>` adds one unless `x` is a
/// void element or the tag is self-closing, `</x>` removes one. Unclosed optional tags
/// (`<p>`, `<li>`) make this overestimate, which only means the simple renderer is used.
fn nesting_depth(html: &str) -> usize {
    let bytes = html.as_bytes();
    let (mut depth, mut deepest, mut i) = (0usize, 0usize, 0usize);
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let closing = bytes.get(i + 1) == Some(&b'/');
        let name_start = if closing { i + 2 } else { i + 1 };
        let name_end = bytes[name_start.min(bytes.len())..]
            .iter()
            .position(|b| !b.is_ascii_alphanumeric())
            .map_or(bytes.len(), |n| name_start + n);
        let tag_end = bytes[i..]
            .iter()
            .position(|&b| b == b'>')
            .map_or(bytes.len(), |n| i + n);
        if name_end > name_start {
            let name = html[name_start..name_end].to_ascii_lowercase();
            let self_closing = tag_end > 0 && bytes.get(tag_end - 1) == Some(&b'/');
            if closing {
                depth = depth.saturating_sub(1);
            } else if !self_closing && !VOID_ELEMENTS.contains(&name.as_str()) {
                depth += 1;
                deepest = deepest.max(depth);
            }
        }
        i = tag_end.max(i + 1);
    }
    deepest
}

/// Linear-time HTML → text for pathological input: drops `<script>`/`<style>` content and all
/// tags, turns block-level tags into line breaks, decodes common entities.
fn plain_text(html: &str) -> String {
    const BLOCKS: [&str; 16] = [
        "p", "div", "br", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "section", "article",
        "ul", "ol", "table",
    ];
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&decode_entities(&rest[..start]));
        let after = &rest[start + 1..];
        let end = after.find('>').map_or(after.len(), |n| n + 1);
        let tag = &after[..end.saturating_sub(1)];
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        rest = &after[end.min(after.len())..];
        if (name == "script" || name == "style") && !tag.starts_with('/') {
            // Skip to the matching close tag (case-insensitive).
            let close = format!("</{name}");
            let lower = rest.to_ascii_lowercase();
            let skip = lower.find(&close).map_or(rest.len(), |n| {
                n + lower[n..].find('>').map_or(lower.len() - n, |m| m + 1)
            });
            rest = &rest[skip.min(rest.len())..];
        } else if BLOCKS.contains(&name.as_str()) {
            out.push('\n');
        }
    }
    out.push_str(&decode_entities(rest));
    crate::util::normalize_whitespace(&strip_markers(&out))
}

/// The handful of entities that matter for readable text.
fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Render with our decorator; `None` if html2text reports an error or panics.
fn render(html: &str) -> Option<String> {
    let rendered = catch_panic("HTML", || {
        html2text::config::with_decorator(ExtractDecorator::default())
            // Tables become one cell per line instead of box drawings.
            .raw_mode(true)
            .link_footnotes(false)
            .no_link_wrapping()
            .allow_width_overflow()
            .string_from_read(html.as_bytes(), RENDER_WIDTH)
    });
    rendered.ok()?.ok()
}

/// Split rendered text at heading lines (see [`heading_on_line`]). Consecutive lines with the
/// same heading id are one heading that contains a line break (`<br>`).
fn split_at_headings(rendered: &str) -> Vec<Segment> {
    let mut sections: Vec<Section> = vec![Section::default()];
    let mut previous_heading_id: Option<&str> = None;
    for line in rendered.lines() {
        // A heading with nothing but a link URL (e.g. an image link) is kept as text.
        let heading =
            heading_on_line(line).filter(|(_, text)| !without_link_urls(text).trim().is_empty());
        let Some((id, heading)) = heading else {
            previous_heading_id = None;
            if let Some(section) = sections.last_mut() {
                section.body.push_str(&strip_markers(line));
                section.body.push('\n');
            }
            continue;
        };
        match sections.last_mut() {
            Some(section) if previous_heading_id == Some(id) => {
                section.heading.push(' ');
                section.heading.push_str(heading);
            }
            _ => sections.push(Section {
                heading: heading.to_string(),
                body: String::new(),
            }),
        }
        previous_heading_id = Some(id);
    }
    sections.into_iter().map(Section::into_segment).collect()
}

/// `(heading id, heading text)` when `line` is a line of an h1–h3 heading. Anything before
/// the marker is a list bullet or quote prefix (`"* "`, `"> "`).
fn heading_on_line(line: &str) -> Option<(&str, &str)> {
    let mark = line.rfind(HEADING_MARK)?;
    let after_mark = &line[mark + HEADING_MARK.len_utf8()..];
    let id_len: usize = after_mark
        .chars()
        .take(HEADING_ID_DIGITS)
        .map(char::len_utf8)
        .sum();
    let (id, text) = after_mark.split_at(id_len);
    Some((id, text.trim()))
}

/// `text` without the link URL suffixes. The [`LINK_MARK`]s come in pairs around
/// `" (url)"`, so every second piece between marks is a URL suffix.
fn without_link_urls(text: &str) -> String {
    text.split(LINK_MARK).step_by(2).collect()
}

/// A heading (empty before the first heading) and the text under it.
/// `heading` still contains [`LINK_MARK`]s; `body` does not.
#[derive(Default)]
struct Section {
    heading: String,
    body: String,
}

impl Section {
    fn into_segment(self) -> Segment {
        match heading_locator(&without_link_urls(&self.heading)) {
            Some(locator) => Segment {
                locator: Some(locator),
                text: format!("{}\n\n{}", self.heading.replace(LINK_MARK, ""), self.body),
            },
            None => Segment {
                locator: None,
                text: self.body,
            },
        }
    }
}

/// `html2text` decorator producing plain text with inline link URLs and heading markers.
#[derive(Clone, Debug, Default)]
struct ExtractDecorator {
    /// URLs of the links currently open, innermost last (`decorate_link_end` needs the URL).
    open_links: Vec<String>,
    /// Heading ids handed out so far. Shared (`Rc`) with the decorators `html2text` makes for
    /// list items and table cells, so no two headings of one document share an id.
    headings_seen: Rc<Cell<u16>>,
}

impl TextDecorator for ExtractDecorator {
    type Annotation = ();

    fn decorate_link_start(&mut self, url: &str) -> (String, Self::Annotation) {
        self.open_links.push(url.to_string());
        (String::new(), ())
    }

    fn decorate_link_end(&mut self) -> String {
        match self.open_links.pop() {
            Some(url) if is_useful_url(&url) => format!("{LINK_MARK} ({url}){LINK_MARK}"),
            _ => String::new(),
        }
    }

    fn decorate_em_start(&self) -> (String, Self::Annotation) {
        (String::new(), ())
    }

    fn decorate_em_end(&self) -> String {
        String::new()
    }

    fn decorate_strong_start(&self) -> (String, Self::Annotation) {
        (String::new(), ())
    }

    fn decorate_strong_end(&self) -> String {
        String::new()
    }

    fn decorate_strikeout_start(&self) -> (String, Self::Annotation) {
        (String::new(), ())
    }

    fn decorate_strikeout_end(&self) -> String {
        String::new()
    }

    fn decorate_code_start(&self) -> (String, Self::Annotation) {
        (String::new(), ())
    }

    fn decorate_code_end(&self) -> String {
        String::new()
    }

    fn decorate_preformat_first(&self) -> Self::Annotation {}

    fn decorate_preformat_cont(&self) -> Self::Annotation {}

    /// Only called for images with alt text (`title` is the alt text).
    fn decorate_image(&mut self, _src: &str, title: &str) -> (String, Self::Annotation) {
        (format!("[image: {title}]"), ())
    }

    /// `html2text` repeats the returned prefix on every line of one heading element.
    fn header_prefix(&self, level: usize) -> String {
        if level > 3 {
            return String::new();
        }
        let id = self.headings_seen.get();
        self.headings_seen.set(id.wrapping_add(1));
        heading_prefix(id)
    }

    fn quote_prefix(&self) -> String {
        "> ".to_string()
    }

    fn unordered_item_prefix(&self) -> String {
        "* ".to_string()
    }

    fn ordered_item_prefix(&self, i: i64) -> String {
        format!("{i}. ")
    }

    fn make_subblock_decorator(&self) -> Self {
        ExtractDecorator {
            open_links: Vec::new(),
            headings_seen: Rc::clone(&self.headings_seen),
        }
    }
}

/// Whether a link target is worth showing next to the link text.
fn is_useful_url(url: &str) -> bool {
    let url = url.trim();
    !url.is_empty() && !url.starts_with('#') && !url.to_ascii_lowercase().starts_with("javascript:")
}

#[cfg(test)]
mod tests {
    #[test]
    fn deeply_nested_html_uses_the_linear_renderer_quickly() {
        let depth = 100_000;
        let html = format!(
            "{}<script>var hidden = 1;</script>deep text &amp; more{}",
            "<div>".repeat(depth),
            "</div>".repeat(depth)
        );
        let started = std::time::Instant::now();
        let segments = segments(&html);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "deep text & more");
        assert!(!segments[0].text.contains("hidden"));
    }

    #[test]
    fn nesting_depth_ignores_void_and_self_closing_tags() {
        assert_eq!(nesting_depth("<p>a<br>b<img src=x/><br/></p>"), 1);
        assert_eq!(nesting_depth("<div><div><span>x</span></div></div>"), 3);
        assert_eq!(nesting_depth("no tags at all < 3 > 2"), 0);
        assert_eq!(nesting_depth("<DIV><Div>"), 2);
        // Normal pages stay on the html2text path.
        let normal = format!("<h1>Week 1</h1>{}", "<ul><li>item</li></ul>".repeat(50));
        assert!(nesting_depth(&normal) < MAX_RENDER_DEPTH);
    }

    use super::*;

    fn all_text(segments: &[Segment]) -> String {
        segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn locators(segments: &[Segment]) -> Vec<Option<&str>> {
        segments.iter().map(|s| s.locator.as_deref()).collect()
    }

    #[test]
    fn scripts_and_styles_are_removed() {
        let html = r#"<html><head><title>T</title><style>.x{color:red}</style>
            <script>var secret = "SCRIPT_TEXT";</script></head>
            <body><p>Visible DEMO101 text</p><script>alert("INLINE_SCRIPT")</script>
            <style>p { STYLE_RULE: 1 }</style><noscript>No script fallback</noscript></body></html>"#;
        let text = all_text(&segments(html));
        assert!(text.contains("Visible DEMO101 text"), "{text}");
        for hidden in ["SCRIPT_TEXT", "INLINE_SCRIPT", "STYLE_RULE", "color:red"] {
            assert!(!text.contains(hidden), "{hidden} leaked into {text:?}");
        }
    }

    #[test]
    fn links_render_inline_with_url() {
        let html = r##"<p>See the <a href="https://example.edu/demo101/syllabus">syllabus</a>
            and <a href="#top">top</a> or <a href="javascript:void(0)">menu</a>.</p>"##;
        let text = all_text(&segments(html));
        assert!(
            text.contains(
                "See the syllabus (https://example.edu/demo101/syllabus) and top or menu."
            ),
            "{text:?}"
        );
        assert!(
            !text.contains("[1]"),
            "footnotes must not be used: {text:?}"
        );
    }

    #[test]
    fn splits_at_h1_to_h3_headings() {
        let html = "<p>Intro before headings.</p>\
            <h1>DEMO101 Intro to Demo Studies</h1><p>Welcome.</p>\
            <h2>Week <em>1</em></h2><p>Topics for week one.</p>\
            <h4>Small heading</h4><p>Still week one.</p>\
            <h3>Readings</h3><ul><li>Chapter 1</li></ul>";
        let segments = segments(html);
        assert_eq!(
            locators(&segments),
            vec![
                None,
                Some("§ DEMO101 Intro to Demo Studies"),
                Some("§ Week 1"),
                Some("§ Readings")
            ]
        );
        assert!(segments[0].text.contains("Intro before headings."));
        assert!(segments[2].text.starts_with("Week 1\n"));
        assert!(segments[2].text.contains("Small heading"));
        assert!(segments[2].text.contains("Still week one."));
        assert!(segments[3].text.contains("Chapter 1"));
    }

    #[test]
    fn consecutive_headings_stay_separate() {
        let segments = segments("<h1>Course</h1><h2>Week 1</h2><p>Body</p>");
        assert_eq!(
            locators(&segments),
            vec![None, Some("§ Course"), Some("§ Week 1")]
        );
    }

    #[test]
    fn linked_heading_has_clean_locator_but_keeps_url_in_text() {
        let segments = segments(
            r#"<h3><a href="https://example.edu/demo101/week3.pdf">Week 3 slides</a></h3><p>x</p>"#,
        );
        assert_eq!(locators(&segments), vec![None, Some("§ Week 3 slides")]);
        assert!(
            segments[1]
                .text
                .starts_with("Week 3 slides (https://example.edu/demo101/week3.pdf)\n"),
            "{:?}",
            segments[1].text
        );
        assert!(!all_text(&segments).contains(LINK_MARK));
    }

    #[test]
    fn heading_that_is_only_a_link_url_does_not_split() {
        // html2text itself drops empty links and headings, so feed rendered text directly.
        let rendered = format!(
            "{}Course\n\na\n\n{}{LINK_MARK} (https://example.edu/x){LINK_MARK}\nb\n",
            heading_prefix(0),
            heading_prefix(1)
        );
        let segments = split_at_headings(&rendered);
        assert_eq!(locators(&segments), vec![None, Some("§ Course")]);
        assert!(
            segments[1].text.contains("(https://example.edu/x)\nb"),
            "{:?}",
            segments[1].text
        );
        assert!(!all_text(&segments).chars().any(is_marker_char));
    }

    #[test]
    fn heading_prefix_has_a_fixed_length() {
        let lengths: Vec<usize> = [0, 1, 255, 256, u16::MAX]
            .into_iter()
            .map(|id| heading_prefix(id).len())
            .collect();
        assert!(lengths.iter().all(|len| *len == lengths[0]), "{lengths:?}");
        assert_eq!(heading_prefix(0).chars().count(), 1 + HEADING_ID_DIGITS);
        assert_ne!(heading_prefix(1), heading_prefix(256));
    }

    #[test]
    fn heading_with_line_break_is_one_heading() {
        let segments = segments("<h2>Week 3<br>Graphs</h2><p>Body text</p>");
        assert_eq!(locators(&segments), vec![None, Some("§ Week 3 Graphs")]);
    }

    #[test]
    fn adjacent_headings_in_lists_and_tables_stay_separate() {
        // html2text puts no blank line between these headings; each must still be its own
        // section (regression: they used to merge into "§ Unit Week 1" etc.).
        let cases: [(&str, &[&str]); 4] = [
            (
                "<h2>Unit</h2><ul><li><h3>Week 1</h3></li></ul><p>x</p>",
                &["§ Unit", "§ Week 1"],
            ),
            (
                "<ul><li><h3>Week 1</h3></li><li><h3>Week 2</h3></li></ul><p>body</p>",
                &["§ Week 1", "§ Week 2"],
            ),
            (
                "<table><tr><td><h2>Unit A</h2></td></tr><tr><td><h2>Unit B</h2></td></tr></table>",
                &["§ Unit A", "§ Unit B"],
            ),
            (
                "<table><tr><td><h2>Mon</h2></td><td><h2>Tue</h2></td></tr></table>",
                &["§ Mon", "§ Tue"],
            ),
        ];
        for (html, expected) in cases {
            let segments = segments(html);
            let found: Vec<&str> = segments
                .iter()
                .filter_map(|s| s.locator.as_deref())
                .collect();
            assert_eq!(found, expected, "{html}");
        }
    }

    #[test]
    fn heading_inside_list_or_table_is_found() {
        let segments = segments(
            "<ul><li><h3>Listed heading</h3></li></ul><table><tr><td><h2>Cell heading</h2></td><td>cell body</td></tr></table>",
        );
        let found = locators(&segments);
        assert!(found.contains(&Some("§ Listed heading")), "{found:?}");
        assert!(found.contains(&Some("§ Cell heading")), "{found:?}");
        assert!(all_text(&segments).contains("cell body"));
    }

    #[test]
    fn literal_marker_in_input_does_not_create_headings() {
        let segments = segments("<p>\u{E000}\u{E100}\u{E101}Not a \u{E001}heading</p>");
        assert_eq!(locators(&segments), vec![None]);
        assert!(
            segments[0].text.contains("Not a heading"),
            "{:?}",
            segments[0].text
        );
    }

    #[test]
    fn tables_render_without_borders() {
        let text = all_text(&segments(
            "<table><tr><th>Week</th><th>Topic</th></tr><tr><td>1</td><td>Demo basics</td></tr></table>",
        ));
        assert!(text.contains("Demo basics"), "{text}");
        assert!(!text.contains('─') && !text.contains('│'), "{text}");
    }

    #[test]
    fn long_paragraphs_are_not_wrapped() {
        let long = "word ".repeat(400);
        let text = all_text(&segments(&format!("<p>{long}</p>")));
        assert_eq!(text.trim().lines().count(), 1);
    }

    #[test]
    fn empty_and_garbage_input_do_not_panic() {
        assert!(all_text(&segments("")).trim().is_empty());
        let _ = segments("<<<>>><div><p><table><td>unclosed");
        let _ = segments(&"<div>".repeat(2000));
    }

    #[test]
    fn entities_and_images_are_rendered() {
        let text = all_text(&segments(
            "<p>A &amp; B &lt;3</p><img src=\"x.png\" alt=\"Diagram of demo flow\"><img src=\"y.png\">",
        ));
        assert!(text.contains("A & B <3"), "{text}");
        assert!(text.contains("[image: Diagram of demo flow]"), "{text}");
    }
}
