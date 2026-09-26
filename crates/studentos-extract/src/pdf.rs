//! PDF → one segment per page (locator `"p. N"`, 1-based), using `pdf-extract`.
//!
//! We call `pdf-extract` page by page (the same building blocks its own
//! `extract_text_from_mem_by_pages` uses), so a page that fails or panics only loses that
//! page instead of the whole document. A PDF where every page yields no text (a scan) gives
//! empty segments, which `extract_file` turns into `Ok(vec![])`. When no page yields text
//! and some page *failed*, the failure is reported instead (it is not a scan).
//!
//! Robustness notes:
//! - `pdf-extract` panics (`unwrap`/`expect`) on some malformed PDFs, so every call into it
//!   is wrapped in [`catch_panic`].
//! - Some broken files would make it recurse until the stack overflows, which aborts the
//!   process (not catchable); [`PageChecker`] skips such pages first.
//! - `output_doc_page` re-reads the whole page tree on every call, so the work grows with the
//!   square of the page count. Files with more than `Limits::max_pdf_pages` page entries are
//!   refused, and a page listed twice in the page tree (only in broken files) is read once.
//! - Stream decompression inside `lopdf` has no size limit, so a small PDF can still inflate
//!   to a lot of memory (see "Safety limits" in `lib.rs`).
//! - It does not print to stdout itself: its debug macro is a no-op and its warnings go
//!   through the `log` crate, so they appear only where the host's logger sends them (e.g. a
//!   `tracing-subscriber` with the `log` bridge). `lopdf` only prints in its own tests and
//!   examples. A caught panic is still reported on stderr by Rust's panic hook. Extraction
//!   runs at sync time only, never inside the stdio MCP server (whose stdout must stay clean).

use std::collections::HashSet;

use pdf_extract::{Document, ObjectId, PlainTextOutput};

use crate::pdf_check::PageChecker;
use crate::util::{catch_panic, failed};
use crate::{ExtractError, Segment};

/// Extract the text of every page of the PDF in `bytes`. PDFs whose page tree lists more
/// than `max_pages` pages are refused.
pub(crate) fn extract(bytes: &[u8], max_pages: usize) -> Result<Vec<Segment>, ExtractError> {
    // First `?`: the parser panicked. Second `?`: it returned an error.
    let (document, pages) = catch_panic("PDF", || load(bytes))??;
    if pages.len() > max_pages {
        return Err(failed(format!(
            "PDF has more than {} pages (it has {}), so it was not indexed",
            crate::util::thousands(max_pages as u64),
            crate::util::thousands(pages.len() as u64)
        )));
    }

    let mut checker = PageChecker::default();
    let mut pages_read = HashSet::new();
    let mut segments = Vec::new();
    let mut first_error = None;
    for (page, page_id) in pages {
        if !pages_read.insert(page_id) {
            continue; // the same page object listed again
        }
        match page_text(&document, page, page_id, &mut checker) {
            Ok(text) => segments.push(Segment {
                locator: Some(format!("p. {page}")),
                text,
            }),
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    // Some pages failing is tolerated. But when no page gave any text, a failed page means
    // "unreadable", not "scanned": report the error instead of an empty result.
    let has_text = segments.iter().any(|s| !s.text.trim().is_empty());
    match first_error {
        Some(error) if !has_text => Err(error),
        _ => Ok(segments),
    }
}

/// Parse the document (decrypting it if it only has an empty user password) and list its
/// pages: `(page number, page object)`, page numbers 1-based and in page order.
fn load(bytes: &[u8]) -> Result<(Document, Vec<(u32, ObjectId)>), ExtractError> {
    let mut document =
        Document::load_mem(bytes).map_err(|e| failed(format!("cannot read PDF: {e}")))?;
    if document.is_encrypted() {
        document
            .decrypt("")
            .map_err(|_| failed("PDF is password-protected"))?;
    }
    let pages = document.get_pages().into_iter().collect();
    Ok((document, pages))
}

fn page_text(
    document: &Document,
    page: u32,
    page_id: ObjectId,
    checker: &mut PageChecker,
) -> Result<String, ExtractError> {
    catch_panic("PDF", || {
        checker
            .check(document, page_id)
            .map_err(|reason| failed(format!("PDF page {page} was skipped: {reason}")))?;
        let mut text = String::new();
        {
            let mut output = PlainTextOutput::new(&mut text);
            pdf_extract::output_doc_page(document, &mut output, page)
                .map_err(|e| failed(format!("cannot read PDF page {page}: {e}")))?;
        }
        Ok(text)
    })? // `?` handles a panic; the closure's own `Result` is returned as is.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Limits;
    use crate::test_support::{PdfFixture, pdf_bytes, pdf_bytes_with_broken_page};
    use pdf_extract::Dictionary;

    fn read(bytes: &[u8]) -> Result<Vec<Segment>, ExtractError> {
        extract(bytes, Limits::DEFAULT.max_pdf_pages)
    }

    fn page_texts(segments: &[Segment]) -> Vec<(String, String)> {
        segments
            .iter()
            .map(|s| (s.locator.clone().unwrap(), s.text.trim().to_string()))
            .collect()
    }

    #[test]
    fn one_segment_per_page_with_page_locators() {
        let bytes = pdf_bytes(&[Some("Alpha page one"), None, Some("Gamma page three")]);
        let segments = read(&bytes).unwrap();
        assert_eq!(
            page_texts(&segments),
            vec![
                ("p. 1".to_string(), "Alpha page one".to_string()),
                ("p. 2".to_string(), String::new()),
                ("p. 3".to_string(), "Gamma page three".to_string()),
            ]
        );
    }

    #[test]
    fn malformed_pdf_is_failed_not_panic() {
        for bytes in [
            &b"%PDF-1.5\nthis is not really a pdf"[..],
            b"",
            b"\x00\x01\x02garbage",
        ] {
            assert!(matches!(read(bytes), Err(ExtractError::Failed(_))));
        }
        // A truncated real PDF: lopdf may or may not recover it, but it must not panic.
        let full = pdf_bytes(&[Some("Alpha")]);
        let _ = read(&full[..full.len() / 2]);
    }

    #[test]
    fn a_broken_page_does_not_lose_the_other_pages() {
        // Page 2 has no MediaBox, which makes pdf-extract panic (`expect("MediaBox")`).
        let bytes = pdf_bytes_with_broken_page(
            &[
                Some("Alpha page one"),
                Some("Broken"),
                Some("Gamma page three"),
            ],
            1,
        );
        let segments = read(&bytes).unwrap();
        let locators: Vec<_> = segments.iter().map(|s| s.locator.as_deref()).collect();
        assert_eq!(locators, vec![Some("p. 1"), Some("p. 3")]);
    }

    #[test]
    fn blank_pages_do_not_hide_a_failed_text_page() {
        // Only page 1 has text, and it fails; page 2 is blank. Reporting "no text" (a scan)
        // would hide the parser error.
        let bytes = pdf_bytes_with_broken_page(&[Some("Alpha page one"), None], 0);
        let result = read(&bytes);
        assert!(matches!(result, Err(ExtractError::Failed(_))), "{result:?}");
    }

    /// Add a normal page after the broken one, and check that only the normal page (p. 2)
    /// is extracted: the broken page is skipped, and the process is still alive.
    fn assert_first_page_is_skipped(mut pdf: PdfFixture) {
        let resources = pdf.font_resources();
        let good = pdf.page_dict(PdfFixture::text("Good page"), resources);
        let good_id = pdf.next_id();
        pdf.add_page(good_id, good);
        let segments = read(&pdf.finish()).unwrap();
        assert_eq!(
            page_texts(&segments),
            vec![("p. 2".into(), "Good page".into())]
        );
    }

    #[test]
    fn page_whose_parent_chain_loops_is_skipped_not_a_stack_overflow() {
        // pdf-extract follows /Parent to inherit /Resources and /MediaBox; here /Parent points
        // back at the page itself and neither key exists.
        let mut pdf = PdfFixture::new();
        let page_id = pdf.next_id();
        let mut page = pdf.page_dict(PdfFixture::text("Loop"), Dictionary::new());
        page.remove(b"Resources");
        page.remove(b"MediaBox");
        page.set("Parent", page_id);
        pdf.add_page(page_id, page);
        assert_first_page_is_skipped(pdf);
    }

    #[test]
    fn form_that_draws_itself_is_skipped_not_a_stack_overflow() {
        let mut pdf = PdfFixture::new();
        let form = pdf.next_id();
        let resources = pdf.resources_with_forms(&[("X1", form)]);
        pdf.set_form(
            form,
            vec![PdfFixture::draw("X1")],
            Some(resources.clone().into()),
        );
        let page = pdf.page_dict(vec![PdfFixture::draw("X1")], resources);
        let page_id = pdf.next_id();
        pdf.add_page(page_id, page);
        assert_first_page_is_skipped(pdf);
    }

    #[test]
    fn forms_that_draw_each_other_are_skipped() {
        let mut pdf = PdfFixture::new();
        let (a, b) = (pdf.next_id(), pdf.next_id());
        let draws_b = pdf.resources_with_forms(&[("B", b)]);
        let draws_a = pdf.resources_with_forms(&[("A", a)]);
        pdf.set_form(a, vec![PdfFixture::draw("B")], Some(draws_b.into()));
        pdf.set_form(b, vec![PdfFixture::draw("A")], Some(draws_a.clone().into()));
        let page = pdf.page_dict(vec![PdfFixture::draw("A")], draws_a);
        let page_id = pdf.next_id();
        pdf.add_page(page_id, page);
        assert_first_page_is_skipped(pdf);
    }

    #[test]
    fn form_without_resources_that_redraws_itself_is_skipped() {
        // The form has no /Resources, so pdf-extract looks up its "/X Do" in the page's
        // resources — which name the form itself.
        let mut pdf = PdfFixture::new();
        let form = pdf.next_id();
        pdf.set_form(form, vec![PdfFixture::draw("X")], None);
        let resources = pdf.resources_with_forms(&[("X", form)]);
        let page = pdf.page_dict(vec![PdfFixture::draw("X")], resources);
        let page_id = pdf.next_id();
        pdf.add_page(page_id, page);
        assert_first_page_is_skipped(pdf);
    }

    /// Add a page that draws a chain of `depth` nested forms; the innermost one draws `text`.
    fn add_nested_forms_page(pdf: &mut PdfFixture, depth: usize, text: &str) {
        let ids: Vec<ObjectId> = (0..depth).map(|_| pdf.next_id()).collect();
        for (level, id) in ids.iter().enumerate() {
            match ids.get(level + 1) {
                Some(next) => {
                    let resources = pdf.resources_with_forms(&[("N", *next)]);
                    pdf.set_form(*id, vec![PdfFixture::draw("N")], Some(resources.into()));
                }
                None => {
                    let resources = pdf.font_resources();
                    pdf.set_form(*id, PdfFixture::text(text), Some(resources.into()));
                }
            }
        }
        let resources = pdf.resources_with_forms(&[("N", ids[0])]);
        let page = pdf.page_dict(vec![PdfFixture::draw("N")], resources);
        let page_id = pdf.next_id();
        pdf.add_page(page_id, page);
    }

    #[test]
    fn normally_nested_forms_are_extracted() {
        let mut pdf = PdfFixture::new();
        add_nested_forms_page(&mut pdf, 3, "Text inside forms");
        let segments = read(&pdf.finish()).unwrap();
        assert_eq!(
            page_texts(&segments),
            vec![("p. 1".into(), "Text inside forms".into())]
        );
    }

    #[test]
    fn very_deeply_nested_forms_are_skipped() {
        let mut pdf = PdfFixture::new();
        add_nested_forms_page(&mut pdf, 2_000, "Too deep");
        assert_first_page_is_skipped(pdf);
    }

    #[test]
    fn shared_resources_that_list_the_form_itself_are_not_a_loop() {
        // Some writers give forms the page's own resources dictionary, so the form's
        // resources name the form itself even though it never draws itself.
        let mut pdf = PdfFixture::new();
        let form = pdf.next_id();
        let shared = pdf.resources_with_forms(&[("Fm1", form)]);
        let shared_id = pdf.add_object(shared);
        pdf.set_form(form, PdfFixture::text("Form text"), Some(shared_id.into()));
        let page = pdf.page_dict(vec![PdfFixture::draw("Fm1")], shared_id);
        let page_id = pdf.next_id();
        pdf.add_page(page_id, page);
        let segments = read(&pdf.finish()).unwrap();
        assert_eq!(
            page_texts(&segments),
            vec![("p. 1".into(), "Form text".into())]
        );
    }

    #[test]
    fn too_many_pages_is_failed() {
        let bytes = pdf_bytes(&[Some("Alpha"), Some("Beta"), Some("Gamma")]);
        assert_eq!(extract(&bytes, 3).unwrap().len(), 3);
        let result = extract(&bytes, 2);
        assert!(
            matches!(&result, Err(ExtractError::Failed(m)) if m == "PDF has more than 2 pages (it has 3), so it was not indexed"),
            "{result:?}"
        );
    }

    #[test]
    fn repeated_page_references_are_extracted_once() {
        let mut pdf = PdfFixture::new();
        for text in ["Alpha", "Beta"] {
            let resources = pdf.font_resources();
            let page = pdf.page_dict(PdfFixture::text(text), resources);
            let page_id = pdf.next_id();
            pdf.add_page(page_id, page);
            if text == "Alpha" {
                pdf.add_kid(page_id); // listed twice: pages 1 and 2
            }
        }
        let segments = read(&pdf.finish()).unwrap();
        assert_eq!(
            page_texts(&segments),
            vec![
                ("p. 1".into(), "Alpha".into()),
                ("p. 3".into(), "Beta".into())
            ]
        );
    }
}
