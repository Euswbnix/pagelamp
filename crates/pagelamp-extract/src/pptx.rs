//! PowerPoint `.pptx` → one segment per slide, locator `"slide N"` (N = 1-based position in
//! the presentation, i.e. what PowerPoint shows).
//!
//! - Slide order: the `<p:sldIdLst>` of `ppt/presentation.xml` (or the main part named in
//!   `_rels/.rels`), whose `r:id`s point (through `ppt/_rels/presentation.xml.rels`) to the
//!   slide parts. If that is missing or empty we fall back to the numeric order of
//!   `ppt/slides/slideN.xml`. A listed slide that is missing, or listed a second time, is
//!   skipped without renumbering the others.
//! - Slide text: every `<a:t>` run, a newline after each `<a:p>` paragraph (text boxes,
//!   placeholders and tables alike).
//! - Speaker notes: found through the slide's own relationships
//!   (`ppt/slides/_rels/slideN.xml.rels`, type `notesSlide`) — notes file numbers do not
//!   have to match slide numbers. Only the notes *body* placeholder is used (not the slide
//!   image or slide number placeholders); it is appended as `"Notes: …"`.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek};

use quick_xml::events::Event;

use crate::ooxml::{
    Package, attr, attribute_where, end_local_name, event_text, for_each_event, local_name,
    related_part, relationships_of, resolve_target,
};
use crate::{ExtractError, Segment};

pub(crate) fn extract<R: Read + Seek>(
    mut package: Package<R>,
) -> Result<Vec<Segment>, ExtractError> {
    let slide_parts = slide_parts_in_order(&mut package)?;
    let mut slides_read = HashSet::new();
    let mut segments = Vec::new();
    for (index, part) in slide_parts.iter().enumerate() {
        if !slides_read.insert(part.as_str()) {
            continue; // listed twice (broken file): read once, keep the numbering of the others
        }
        let Some(xml) = package.read_text(part)? else {
            continue; // listed but missing: skip it, keep the numbering of the others
        };
        let mut text = slide_text(&xml, part)?.trim_end().to_string();
        let notes = speaker_notes(&mut package, part)?;
        if !notes.trim().is_empty() {
            text.push_str("\n\nNotes: ");
            text.push_str(notes.trim());
        }
        segments.push(Segment {
            locator: Some(format!("slide {}", index + 1)),
            text,
        });
    }
    Ok(segments)
}

/// Slide part names in presentation order.
fn slide_parts_in_order<R: Read + Seek>(
    package: &mut Package<R>,
) -> Result<Vec<String>, ExtractError> {
    let from_presentation = slides_from_presentation(package)?;
    if !from_presentation.is_empty() {
        return Ok(from_presentation);
    }
    Ok(slides_by_file_number(&package.part_names()))
}

fn slides_from_presentation<R: Read + Seek>(
    package: &mut Package<R>,
) -> Result<Vec<String>, ExtractError> {
    let presentation = related_part(package, "", "officeDocument", "ppt/presentation.xml")?;
    let Some(xml) = package.read_text(&presentation)? else {
        return Ok(Vec::new());
    };
    let slide_ids = slide_relationship_ids(&xml, &presentation)?;
    let relationships = relationships_of(package, &presentation)?;
    // Relationship id → target (the first one wins if an id repeats).
    let mut targets: HashMap<&str, &str> = HashMap::new();
    for rel in &relationships {
        targets
            .entry(rel.id.as_str())
            .or_insert(rel.target.as_str());
    }
    Ok(slide_ids
        .iter()
        .filter_map(|id| targets.get(id.as_str()))
        .map(|target| resolve_target(&presentation, target))
        .collect())
}

/// The `r:id` of every `<p:sldId>`, in order. (`<p:sldId>` also has a plain numeric `id`
/// attribute, so we want the *prefixed* one.)
fn slide_relationship_ids(presentation_xml: &str, part: &str) -> Result<Vec<String>, ExtractError> {
    let mut ids = Vec::new();
    for_each_event(presentation_xml, part, |event| {
        if let Event::Start(e) | Event::Empty(e) = event
            && local_name(e) == "sldId"
            && let Some(id) = attribute_where(e, |key| key.ends_with(":id"))
        {
            ids.push(id);
        }
    })?;
    Ok(ids)
}

/// Fallback order: `ppt/slides/slide<N>.xml` sorted by N.
fn slides_by_file_number(part_names: &[String]) -> Vec<String> {
    let mut numbered: Vec<(u32, &String)> = part_names
        .iter()
        .filter_map(|name| {
            let number = name
                .strip_prefix("ppt/slides/slide")?
                .strip_suffix(".xml")?
                .parse()
                .ok()?;
            Some((number, name))
        })
        .collect();
    numbered.sort();
    numbered.into_iter().map(|(_, name)| name.clone()).collect()
}

/// All text of a slide.
fn slide_text(xml: &str, part: &str) -> Result<String, ExtractError> {
    let mut text = DrawingText::default();
    for_each_event(xml, part, |event| text.feed(event))?;
    Ok(text.text)
}

/// Body text of the notes slide linked from `slide_part`, or `""` when there are no notes.
fn speaker_notes<R: Read + Seek>(
    package: &mut Package<R>,
    slide_part: &str,
) -> Result<String, ExtractError> {
    let relationships = relationships_of(package, slide_part)?;
    let Some(notes_rel) = relationships.iter().find(|rel| rel.is_kind("notesSlide")) else {
        return Ok(String::new());
    };
    let notes_part = resolve_target(slide_part, &notes_rel.target);
    match package.read_text(&notes_part)? {
        Some(xml) => notes_body_text(&xml, &notes_part),
        None => Ok(String::new()),
    }
}

/// Text of the shapes that are the notes body placeholder (`<p:ph type="body">`).
fn notes_body_text(xml: &str, part: &str) -> Result<String, ExtractError> {
    let mut notes = String::new();
    // Text of the `<p:sp>` shape we are inside, and whether it is the body placeholder.
    let mut shape: Option<(DrawingText, bool)> = None;
    for_each_event(xml, part, |event| {
        match event {
            Event::Start(e) if local_name(e) == "sp" => {
                shape = Some((DrawingText::default(), false));
                return;
            }
            Event::End(e) if end_local_name(e) == "sp" => {
                if let Some((text, true)) = shape.take() {
                    notes.push_str(&text.text);
                }
                return;
            }
            Event::Start(e) | Event::Empty(e) if local_name(e) == "ph" => {
                if let Some((_, is_body)) = &mut shape {
                    *is_body = attr(e, "type").as_deref() == Some("body");
                }
            }
            _ => {}
        }
        if let Some((text, _)) = &mut shape {
            text.feed(event);
        }
    })?;
    Ok(notes)
}

/// Collects DrawingML text: `<a:t>` content, a newline after each `<a:p>` paragraph and for
/// each `<a:br/>` line break.
#[derive(Default)]
struct DrawingText {
    text: String,
    in_text_run: bool,
}

impl DrawingText {
    fn feed(&mut self, event: &Event) {
        match event {
            Event::Start(e) if local_name(e) == "t" => self.in_text_run = true,
            Event::End(e) if end_local_name(e) == "t" => self.in_text_run = false,
            Event::End(e) if end_local_name(e) == "p" => self.text.push('\n'),
            Event::Empty(e) if matches!(local_name(e), "p" | "br") => self.text.push('\n'),
            other if self.in_text_run => {
                if let Some(text) = event_text(other) {
                    self.text.push_str(&text);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::Limits;
    use crate::test_support::{notes_xml, presentation_xml, rels_xml, slide_xml, zip_text};

    fn extract_entries(entries: &[(&str, &str)]) -> Result<Vec<Segment>, ExtractError> {
        let package = Package::open(Cursor::new(zip_text(entries)), &Limits::DEFAULT)?;
        extract(package)
    }

    fn summary(segments: &[Segment]) -> Vec<(String, String)> {
        segments
            .iter()
            .map(|s| (s.locator.clone().unwrap(), s.text.trim().to_string()))
            .collect()
    }

    #[test]
    fn slides_follow_presentation_order_with_notes() {
        // Presentation order is slide3.xml, slide1.xml, slide2.xml, and the notes files are
        // numbered differently from the slides.
        let presentation = presentation_xml(&["rId7", "rId5", "rId6"]);
        let presentation_rels = rels_xml(&[
            ("rId5", "slide", "slides/slide1.xml"),
            ("rId6", "slide", "slides/slide2.xml"),
            ("rId7", "slide", "/ppt/slides/slide3.xml"),
            ("rIdMaster", "slideMaster", "slideMasters/slideMaster1.xml"),
        ]);
        let slide1 = slide_xml(&[
            &["Week 1: Demo basics"],
            &["First point", "Second &amp; last point"],
        ]);
        let slide2 = slide_xml(&[&["Summary"]]);
        let slide3 = slide_xml(&[&["DEMO101 Intro to Demo Studies"]]);
        let slide1_rels = rels_xml(&[
            ("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"),
            ("rId2", "notesSlide", "../notesSlides/notesSlide9.xml"),
        ]);
        let notes9 = notes_xml("Remember to explain the demo.");

        let segments = extract_entries(&[
            ("ppt/presentation.xml", &presentation),
            ("ppt/_rels/presentation.xml.rels", &presentation_rels),
            ("ppt/slides/slide1.xml", &slide1),
            ("ppt/slides/slide2.xml", &slide2),
            ("ppt/slides/slide3.xml", &slide3),
            ("ppt/slides/_rels/slide1.xml.rels", &slide1_rels),
            ("ppt/notesSlides/notesSlide9.xml", &notes9),
        ])
        .unwrap();

        assert_eq!(
            summary(&segments),
            vec![
                ("slide 1".into(), "DEMO101 Intro to Demo Studies".into()),
                (
                    "slide 2".into(),
                    "Week 1: Demo basics\nFirst point\nSecond & last point\n\nNotes: Remember to explain the demo.".into()
                ),
                ("slide 3".into(), "Summary".into()),
            ]
        );
        assert!(!segments[1].text.contains("SLIDE_NUMBER_42"));
    }

    #[test]
    fn many_listed_slides_are_ordered_quickly() {
        // Regression: each slide id used to be matched by scanning all relationships, which
        // is quadratic. Only the first and last listed slides exist; the rest are skipped.
        const SLIDES: usize = 80_000;
        let ids: Vec<String> = (0..SLIDES).map(|i| format!("rId{i}")).collect();
        let targets: Vec<String> = (0..SLIDES).map(|i| format!("slides/s{i}.xml")).collect();
        let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let rels: Vec<(&str, &str, &str)> = ids
            .iter()
            .zip(&targets)
            .rev() // worst case for a linear search
            .map(|(id, target)| (id.as_str(), "slide", target.as_str()))
            .collect();
        let last = format!("ppt/slides/s{}.xml", SLIDES - 1);
        let entries = [
            ("ppt/presentation.xml", presentation_xml(&id_refs)),
            ("ppt/_rels/presentation.xml.rels", rels_xml(&rels)),
            ("ppt/slides/s0.xml", slide_xml(&[&["First"]])),
            (last.as_str(), slide_xml(&[&["Last"]])),
        ];
        let entries: Vec<(&str, &str)> = entries.iter().map(|(n, x)| (*n, x.as_str())).collect();
        let started = std::time::Instant::now();
        let segments = extract_entries(&entries).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        assert_eq!(
            summary(&segments),
            vec![
                ("slide 1".into(), "First".into()),
                (format!("slide {SLIDES}"), "Last".into()),
            ]
        );
    }

    #[test]
    fn slide_listed_twice_is_read_once() {
        let segments = extract_entries(&[
            (
                "ppt/presentation.xml",
                &presentation_xml(&["rId1", "rId1", "rId2"]),
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                &rels_xml(&[
                    ("rId1", "slide", "slides/slide1.xml"),
                    ("rId2", "slide", "slides/slide2.xml"),
                ]),
            ),
            ("ppt/slides/slide1.xml", &slide_xml(&[&["One"]])),
            ("ppt/slides/slide2.xml", &slide_xml(&[&["Two"]])),
        ])
        .unwrap();
        assert_eq!(
            summary(&segments),
            vec![
                ("slide 1".into(), "One".into()),
                ("slide 3".into(), "Two".into())
            ]
        );
    }

    #[test]
    fn falls_back_to_numeric_slide_file_order() {
        let segments = extract_entries(&[
            ("ppt/slides/slide10.xml", &slide_xml(&[&["Ten"]])),
            ("ppt/slides/slide2.xml", &slide_xml(&[&["Two"]])),
            ("ppt/slides/slide1.xml", &slide_xml(&[&["One"]])),
            (
                "ppt/slides/slideLayout.xml",
                &slide_xml(&[&["not a slide"]]),
            ),
        ])
        .unwrap();
        assert_eq!(
            summary(&segments),
            vec![
                ("slide 1".into(), "One".into()),
                ("slide 2".into(), "Two".into()),
                ("slide 3".into(), "Ten".into()),
            ]
        );
    }

    #[test]
    fn line_breaks_and_tables_are_extracted() {
        let slide = r#"<p:sld xmlns:a="a" xmlns:p="p"><p:cSld><p:spTree>
            <p:sp><p:txBody><a:p><a:r><a:t>Line one</a:t></a:r><a:br/><a:r><a:t>Line two</a:t></a:r></a:p></p:txBody></p:sp>
            <p:graphicFrame><a:graphic><a:graphicData><a:tbl><a:tr>
              <a:tc><a:txBody><a:p><a:r><a:t>Cell A</a:t></a:r></a:p></a:txBody></a:tc>
              <a:tc><a:txBody><a:p><a:r><a:t>Cell B</a:t></a:r></a:p></a:txBody></a:tc>
            </a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>
        </p:spTree></p:cSld></p:sld>"#;
        let text = slide_text(slide, "s.xml").unwrap();
        assert_eq!(text.trim(), "Line one\nLine two\nCell A\nCell B");
    }

    #[test]
    fn notes_use_only_the_body_placeholder() {
        let notes = notes_body_text(&notes_xml("Only this."), "n.xml").unwrap();
        assert_eq!(notes.trim(), "Only this.");
    }

    #[test]
    fn malformed_slide_xml_is_failed() {
        let result = extract_entries(&[("ppt/slides/slide1.xml", "<p:sld><a:t>oops</p:sld>")]);
        assert!(matches!(result, Err(ExtractError::Failed(_))));
    }

    #[test]
    fn empty_presentation_gives_no_segments() {
        let segments = extract_entries(&[("[Content_Types].xml", "<Types/>")]).unwrap();
        assert!(segments.is_empty());
    }
}
