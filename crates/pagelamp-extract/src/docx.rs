//! Word `.docx` → one segment per heading section, locator `"§ Heading"` (text before the
//! first heading has no locator).
//!
//! - Text comes from the main document part (`word/document.xml`, or whatever `_rels/.rels`
//!   names; its styles part is found the same way). Every `<w:p>` paragraph (including those
//!   inside table cells and text boxes) becomes one paragraph of output; `<w:t>` is text,
//!   `<w:tab/>` a tab, `<w:br/>`/`<w:cr/>` a line break. Deleted text (`<w:delText>`) and
//!   field codes (`<w:instrText>`) are not `<w:t>`, so they are skipped automatically.
//! - `<mc:Fallback>` content is skipped: it is a second copy of text boxes for old readers.
//!   So is `<w:pPrChange>` (tracked changes): it holds a paragraph's *old* style.
//! - Paragraphs without visible text are dropped.
//! - A paragraph is a heading when its style (`<w:pStyle w:val="…">`) is defined in
//!   `word/styles.xml` with the name "heading 1", "heading 2", "heading 3" or "Title"
//!   (names are language-independent; style *ids* are translated in non-English Word). Style
//!   ids missing from `styles.xml` fall back to their id: "Title", "Heading", "Heading1" …
//!   "Heading3" (case-insensitive). Level 4+ headings stay inside their section, as for
//!   Markdown and HTML.

use std::collections::HashMap;
use std::io::{Read, Seek};

use quick_xml::events::Event;

use crate::ooxml::{
    Package, attr, end_local_name, event_text, for_each_event, local_name, related_part,
};
use crate::util::{failed, heading_locator};
use crate::{ExtractError, Segment};

pub(crate) fn extract<R: Read + Seek>(
    mut package: Package<R>,
) -> Result<Vec<Segment>, ExtractError> {
    let document_part = related_part(&mut package, "", "officeDocument", "word/document.xml")?;
    let styles_part = related_part(&mut package, &document_part, "styles", "word/styles.xml")?;

    let document = package
        .read_text(&document_part)?
        .ok_or_else(|| failed(format!("not a Word document: {document_part} is missing")))?;
    let heading_styles = match package.read_text(&styles_part)? {
        Some(xml) => heading_styles(&xml, &styles_part)?,
        None => HashMap::new(),
    };
    let paragraphs = paragraphs(&document, &document_part)?;
    Ok(group_by_headings(paragraphs, &heading_styles))
}

/// One `<w:p>`: its style id (if any) and its text.
#[derive(Debug, Default, PartialEq)]
struct Paragraph {
    style: Option<String>,
    text: String,
}

/// All paragraphs of the main document part that have visible text (`part` is its name, for
/// error messages). Empty paragraphs are dropped right away: they would only add blank lines,
/// and a document of nothing but `<w:p/>` would otherwise cost many times its size in memory.
fn paragraphs(xml: &str, part: &str) -> Result<Vec<Paragraph>, ExtractError> {
    let mut finished = Vec::new();
    // Paragraphs currently open. Usually 0 or 1, but a text box inside a paragraph holds
    // paragraphs of its own.
    let mut open: Vec<Paragraph> = Vec::new();
    let mut in_text = false; // inside <w:t>
    let mut in_tab_stops = false; // inside <w:tabs>, where <w:tab/> is a tab-stop definition
    let mut skip_depth = 0usize; // inside elements whose content is ignored (see below)

    for_each_event(xml, part, |event| match event {
        Event::Start(e) if is_skipped_element(local_name(e)) => skip_depth += 1,
        Event::End(e) if is_skipped_element(end_local_name(e)) => {
            skip_depth = skip_depth.saturating_sub(1);
        }
        _ if skip_depth > 0 => {}
        Event::Start(e) => match local_name(e) {
            "p" => open.push(Paragraph::default()),
            "t" => in_text = true,
            "tabs" => in_tab_stops = true,
            _ => {}
        },
        Event::End(e) => match end_local_name(e) {
            "p" => finished.extend(open.pop().filter(has_text)),
            "t" => in_text = false,
            "tabs" => in_tab_stops = false,
            _ => {}
        },
        Event::Empty(e) => {
            let Some(paragraph) = open.last_mut() else {
                return;
            };
            match local_name(e) {
                "pStyle" => paragraph.style = attr(e, "val"),
                "tab" if !in_tab_stops => paragraph.text.push('\t'),
                "br" | "cr" => paragraph.text.push('\n'),
                _ => {}
            }
        }
        other if in_text => {
            if let (Some(paragraph), Some(text)) = (open.last_mut(), event_text(other)) {
                paragraph.text.push_str(&text);
            }
        }
        _ => {}
    })?;
    // Unclosed paragraphs cannot happen in well-formed XML, but keep their text if they do.
    finished.extend(open.into_iter().filter(has_text));
    Ok(finished)
}

/// Elements whose whole content is ignored:
/// - `<mc:Fallback>`: a second copy of text boxes, for old readers;
/// - `<w:pPrChange>`: the *old* paragraph properties of a tracked change (its `<w:pStyle>`
///   must not count as the paragraph's style).
fn is_skipped_element(local_name: &str) -> bool {
    matches!(local_name, "Fallback" | "pPrChange")
}

fn has_text(paragraph: &Paragraph) -> bool {
    !paragraph.text.trim().is_empty()
}

/// Style id → whether it is a level 1–3 heading or title style, from `word/styles.xml`.
fn heading_styles(xml: &str, part: &str) -> Result<HashMap<String, bool>, ExtractError> {
    let mut styles = HashMap::new();
    let mut current_id: Option<String> = None;
    for_each_event(xml, part, |event| match event {
        Event::Start(e) | Event::Empty(e) if local_name(e) == "style" => {
            current_id = attr(e, "styleId");
        }
        Event::Start(e) | Event::Empty(e) if local_name(e) == "name" => {
            if let (Some(id), Some(name)) = (current_id.take(), attr(e, "val")) {
                styles.insert(id, is_heading_style_name(&name));
            }
        }
        Event::End(e) if end_local_name(e) == "style" => current_id = None,
        _ => {}
    })?;
    Ok(styles)
}

fn is_heading_style_name(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "heading 1" | "heading 2" | "heading 3" | "title"
    )
}

/// Fallback for style ids that `styles.xml` does not define.
fn is_heading_style_id(style_id: &str) -> bool {
    matches!(
        style_id.to_ascii_lowercase().as_str(),
        "title" | "heading" | "heading1" | "heading2" | "heading3"
    )
}

fn is_heading(paragraph: &Paragraph, heading_styles: &HashMap<String, bool>) -> bool {
    match paragraph.style.as_deref() {
        Some(id) => heading_styles
            .get(id)
            .copied()
            .unwrap_or_else(|| is_heading_style_id(id)),
        None => false,
    }
}

/// Start a new segment at every heading paragraph; paragraphs are separated by blank lines
/// so the chunker can use them as paragraph boundaries.
fn group_by_headings(
    paragraphs: Vec<Paragraph>,
    heading_styles: &HashMap<String, bool>,
) -> Vec<Segment> {
    let mut segments = vec![Segment {
        locator: None,
        text: String::new(),
    }];
    for paragraph in paragraphs {
        if is_heading(&paragraph, heading_styles)
            && let Some(locator) = heading_locator(&paragraph.text)
        {
            segments.push(Segment {
                locator: Some(locator),
                text: String::new(),
            });
        }
        if let Some(current) = segments.last_mut() {
            current.text.push_str(&paragraph.text);
            current.text.push_str("\n\n");
        }
    }
    segments
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::Limits;
    use crate::test_support::{document_xml, para, styles_xml, zip_text};

    fn extract_entries(entries: &[(&str, &str)]) -> Result<Vec<Segment>, ExtractError> {
        let package = Package::open(Cursor::new(zip_text(entries)), &Limits::DEFAULT)?;
        extract(package)
    }

    fn summary(segments: &[Segment]) -> Vec<(Option<String>, String)> {
        segments
            .iter()
            .map(|s| {
                (
                    s.locator.clone(),
                    crate::util::normalize_whitespace(&s.text),
                )
            })
            .collect()
    }

    #[test]
    fn groups_paragraphs_under_headings_from_styles_xml() {
        // Localised style ids: "berschrift1" is Word's German id for "heading 1".
        let body = [
            para(None, "Preface text"),
            para(Some("Titel"), "DEMO101 Intro to Demo Studies"),
            para(None, "Welcome to the course."),
            para(Some("berschrift1"), "Week 1"),
            para(Some("Normal"), "Demo basics."),
            para(Some("berschrift4"), "Minor heading"),
            para(None, "More detail."),
        ]
        .concat();
        let styles = styles_xml(&[
            ("Titel", "Title"),
            ("berschrift1", "heading 1"),
            ("berschrift4", "heading 4"),
            ("Normal", "Normal"),
        ]);
        let segments = extract_entries(&[
            ("word/document.xml", &document_xml(&body)),
            ("word/styles.xml", &styles),
        ])
        .unwrap();
        assert_eq!(
            summary(&segments),
            vec![
                (None, "Preface text".into()),
                (
                    Some("§ DEMO101 Intro to Demo Studies".into()),
                    "DEMO101 Intro to Demo Studies\n\nWelcome to the course.".into()
                ),
                (
                    Some("§ Week 1".into()),
                    "Week 1\n\nDemo basics.\n\nMinor heading\n\nMore detail.".into()
                ),
            ]
        );
    }

    #[test]
    fn falls_back_to_style_ids_without_styles_xml() {
        let body = [
            para(Some("Heading2"), "Section A"),
            para(None, "a"),
            para(Some("heading5"), "Not split"),
            para(Some("TITLE"), "Section B"),
        ]
        .concat();
        let segments = extract_entries(&[("word/document.xml", &document_xml(&body))]).unwrap();
        let locators: Vec<_> = segments.iter().map(|s| s.locator.as_deref()).collect();
        assert_eq!(
            locators,
            vec![None, Some("§ Section A"), Some("§ Section B")]
        );
        assert!(segments[1].text.contains("Not split"));
    }

    #[test]
    fn runs_tabs_breaks_tables_and_entities() {
        let body = r#"
            <w:p><w:pPr><w:tabs><w:tab w:val="left" w:pos="720"/></w:tabs></w:pPr>
              <w:r><w:t>Name</w:t></w:r><w:r><w:tab/><w:t xml:space="preserve">Value </w:t></w:r>
              <w:r><w:t>A&amp;B</w:t><w:br/><w:t>next line</w:t></w:r>
              <w:r><w:instrText> PAGE </w:instrText></w:r><w:del><w:r><w:delText>gone</w:delText></w:r></w:del>
            </w:p>
            <w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell 1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Cell 2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
            <w:p/>
        "#;
        let paragraphs = paragraphs(&document_xml(body), "word/document.xml").unwrap();
        let texts: Vec<&str> = paragraphs.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["Name\tValue A&B\nnext line", "Cell 1", "Cell 2"]
        );
    }

    #[test]
    fn tracked_change_old_style_does_not_make_a_heading() {
        // A reviewer turned a "heading 1" paragraph back into normal text (Normal writes no
        // <w:pStyle>). Only the *old* style remains, inside <w:pPrChange>.
        let formerly_heading = r#"<w:p><w:pPr><w:pPrChange w:id="1" w:author="Demo"><w:pPr><w:pStyle w:val="Heading1"/></w:pPr></w:pPrChange></w:pPr><w:r><w:t>Formerly a heading</w:t></w:r></w:p>"#;
        let changed_to_normal = r#"<w:p><w:pPr><w:pStyle w:val="Normal"/><w:pPrChange w:id="2" w:author="Demo"><w:pPr><w:pStyle w:val="Heading1"/></w:pPr></w:pPrChange></w:pPr><w:r><w:t>Also normal now</w:t></w:r></w:p>"#;
        let body = [
            para(Some("Heading1"), "Real heading"),
            formerly_heading.to_string(),
            changed_to_normal.to_string(),
        ]
        .concat();
        let segments = extract_entries(&[
            ("word/document.xml", &document_xml(&body)),
            (
                "word/styles.xml",
                &styles_xml(&[("Heading1", "heading 1"), ("Normal", "Normal")]),
            ),
        ])
        .unwrap();
        let locators: Vec<_> = segments.iter().map(|s| s.locator.as_deref()).collect();
        assert_eq!(locators, vec![None, Some("§ Real heading")]);
        assert_eq!(
            crate::util::normalize_whitespace(&segments[1].text),
            "Real heading\n\nFormerly a heading\n\nAlso normal now"
        );
    }

    #[test]
    fn paragraphs_without_text_are_not_kept() {
        let body = format!(
            "{}<w:p></w:p><w:p><w:r><w:t> </w:t></w:r></w:p>{}",
            "<w:p/>".repeat(3),
            para(None, "Text")
        );
        let paragraphs = paragraphs(&document_xml(&body), "word/document.xml").unwrap();
        let texts: Vec<&str> = paragraphs.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts, vec!["Text"]);
    }

    #[test]
    fn text_boxes_are_read_once() {
        let body = r#"<w:p><w:r><w:t>Before box.</w:t></w:r><w:r><mc:AlternateContent>
            <mc:Choice Requires="wps"><w:drawing><w:txbxContent><w:p><w:r><w:t>Box text</w:t></w:r></w:p></w:txbxContent></w:drawing></mc:Choice>
            <mc:Fallback><w:pict><w:txbxContent><w:p><w:r><w:t>Box text</w:t></w:r></w:p></w:txbxContent></w:pict></mc:Fallback>
            </mc:AlternateContent></w:r><w:r><w:t> After box.</w:t></w:r></w:p>"#;
        let all: String = paragraphs(&document_xml(body), "word/document.xml")
            .unwrap()
            .into_iter()
            .map(|p| p.text + "|")
            .collect();
        assert_eq!(all.matches("Box text").count(), 1, "{all}");
        assert!(all.contains("Before box. After box."), "{all}");
    }

    #[test]
    fn main_part_is_found_through_package_relationships() {
        let root_rels =
            crate::test_support::rels_xml(&[("rId1", "officeDocument", "/word/document2.xml")]);
        let segments = extract_entries(&[
            ("_rels/.rels", &root_rels),
            (
                "word/document2.xml",
                &document_xml(&para(None, "From document2")),
            ),
        ])
        .unwrap();
        assert_eq!(summary(&segments), vec![(None, "From document2".into())]);
    }

    #[test]
    fn missing_document_or_malformed_xml_is_failed() {
        assert!(matches!(
            extract_entries(&[("word/styles.xml", "<w:styles/>")]),
            Err(ExtractError::Failed(_))
        ));
        assert!(matches!(
            extract_entries(&[("word/document.xml", "<w:document><w:body><w:p></w:body>")]),
            Err(ExtractError::Failed(_))
        ));
    }
}
