//! Builders for tiny synthetic fixture files used by the unit tests (no real course data).

use std::io::{Cursor, Write};
use std::path::PathBuf;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Write `bytes` to `dir/name` and return the path.
pub(crate) fn write_file(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).expect("write fixture");
    path
}

/// A zip archive with the given `(name, content)` entries (stored, not compressed).
pub(crate) fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, content) in entries {
        writer.start_file(*name, options).expect("start zip entry");
        writer.write_all(content).expect("write zip entry");
    }
    writer.finish().expect("finish zip").into_inner()
}

/// Same as [`zip_bytes`] for text entries.
pub(crate) fn zip_text(entries: &[(&str, &str)]) -> Vec<u8> {
    let entries: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(name, text)| (*name, text.as_bytes()))
        .collect();
    zip_bytes(&entries)
}

/// A PDF with one page per entry; `Some(text)` draws one line of Helvetica text, `None`
/// leaves the page blank (like a scanned page without a text layer).
pub(crate) fn pdf_bytes(pages: &[Option<&str>]) -> Vec<u8> {
    build_pdf(pages, None)
}

/// Like [`pdf_bytes`], but page `broken_index` (0-based) has no MediaBox, which makes
/// `pdf-extract` panic on that page.
pub(crate) fn pdf_bytes_with_broken_page(pages: &[Option<&str>], broken_index: usize) -> Vec<u8> {
    build_pdf(pages, Some(broken_index))
}

fn build_pdf(pages: &[Option<&str>], broken_index: Option<usize>) -> Vec<u8> {
    let mut pdf = PdfFixture::new();
    for (index, text) in pages.iter().enumerate() {
        let operations = text.map(PdfFixture::text).unwrap_or_default();
        let resources = pdf.font_resources();
        let mut page = pdf.page_dict(operations, resources);
        if broken_index == Some(index) {
            page.remove(b"MediaBox");
        }
        let page_id = pdf.next_id();
        pdf.add_page(page_id, page);
    }
    pdf.finish()
}

/// Builder for hand-made PDF fixtures: a Helvetica font (`/F1`), pages, content streams and
/// Form XObjects. Object ids can be reserved first (`next_id`) so objects can point at
/// themselves, as some broken files do.
pub(crate) struct PdfFixture {
    doc: Document,
    pages_id: ObjectId,
    font_id: ObjectId,
    /// Page references of the page-tree root, in order (may repeat a page on purpose).
    kids: Vec<Object>,
}

impl PdfFixture {
    pub(crate) fn new() -> Self {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        PdfFixture {
            doc,
            pages_id,
            font_id,
            kids: Vec::new(),
        }
    }

    /// Reserve an object id.
    pub(crate) fn next_id(&mut self) -> ObjectId {
        self.doc.new_object_id()
    }

    /// Content operations that draw one line of `text` with `/F1`.
    pub(crate) fn text(text: &str) -> Vec<Operation> {
        vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.into()]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ]
    }

    /// The `Do` operation that draws the XObject called `name` in the current resources.
    pub(crate) fn draw(name: &str) -> Operation {
        Operation::new("Do", vec![Object::Name(name.as_bytes().to_vec())])
    }

    /// A resources dictionary with just the font.
    pub(crate) fn font_resources(&self) -> Dictionary {
        dictionary! { "Font" => dictionary! { "F1" => self.font_id } }
    }

    /// A resources dictionary with the font and the given `(name, form)` XObjects.
    pub(crate) fn resources_with_forms(&self, forms: &[(&str, ObjectId)]) -> Dictionary {
        let mut xobjects = Dictionary::new();
        for (name, id) in forms {
            xobjects.set(*name, *id);
        }
        let mut resources = self.font_resources();
        resources.set("XObject", xobjects);
        resources
    }

    /// Store `object` under a new id (e.g. a resources dictionary shared by several objects).
    pub(crate) fn add_object(&mut self, object: impl Into<Object>) -> ObjectId {
        self.doc.add_object(object)
    }

    /// Store a Form XObject under `id`. `resources` is a dictionary or a reference to one;
    /// `None` leaves out its `/Resources`.
    pub(crate) fn set_form(
        &mut self,
        id: ObjectId,
        operations: Vec<Operation>,
        resources: Option<Object>,
    ) {
        let bbox: Vec<Object> = vec![0.into(), 0.into(), 612.into(), 792.into()];
        let mut dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => bbox,
        };
        if let Some(resources) = resources {
            dict.set("Resources", resources);
        }
        let content = Content { operations }.encode().expect("encode form");
        self.doc
            .objects
            .insert(id, Object::Stream(Stream::new(dict, content)));
    }

    /// A normal page dictionary (Parent, MediaBox, a content stream with `operations`).
    /// `resources` is a dictionary or a reference to one.
    pub(crate) fn page_dict(
        &mut self,
        operations: Vec<Operation>,
        resources: impl Into<Object>,
    ) -> Dictionary {
        let resources: Object = resources.into();
        let content = Content { operations }.encode().expect("encode content");
        let content_id = self.doc.add_object(Stream::new(dictionary! {}, content));
        let media_box: Vec<Object> = vec![0.into(), 0.into(), 612.into(), 792.into()];
        dictionary! {
            "Type" => "Page",
            "Parent" => self.pages_id,
            "Contents" => content_id,
            "Resources" => resources,
            "MediaBox" => media_box,
        }
    }

    /// Store `page` under `id` and append it to the page tree.
    pub(crate) fn add_page(&mut self, id: ObjectId, page: Dictionary) {
        self.doc.objects.insert(id, Object::Dictionary(page));
        self.add_kid(id);
    }

    /// Append another reference to an existing page to the page tree.
    pub(crate) fn add_kid(&mut self, id: ObjectId) {
        self.kids.push(id.into());
    }

    pub(crate) fn finish(mut self) -> Vec<u8> {
        let page_count = self.kids.len() as i64;
        self.doc.objects.insert(
            self.pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => self.kids,
                "Count" => page_count,
            }),
        );
        let catalog_id = self.doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => self.pages_id,
        });
        self.doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        self.doc.save_to(&mut bytes).expect("save pdf");
        bytes
    }
}

// ---------------------------------------------------------------------------------------
// PPTX parts
// ---------------------------------------------------------------------------------------

const PML: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;

/// `ppt/presentation.xml` listing slides by relationship id, in presentation order.
pub(crate) fn presentation_xml(slide_rel_ids: &[&str]) -> String {
    let ids: String = slide_rel_ids
        .iter()
        .enumerate()
        .map(|(i, rid)| format!(r#"<p:sldId id="{}" r:id="{rid}"/>"#, 256 + i))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentation {PML}><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rIdMaster"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst></p:presentation>"#
    )
}

/// A `.rels` part: `(Id, type suffix such as "slide", Target)`.
pub(crate) fn rels_xml(relationships: &[(&str, &str, &str)]) -> String {
    let rels: String = relationships
        .iter()
        .map(|(id, kind, target)| {
            format!(
                r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{target}"/>"#
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#
    )
}

/// A slide with one text box per entry of `boxes`; each box holds paragraphs.
pub(crate) fn slide_xml(boxes: &[&[&str]]) -> String {
    let shapes: String = boxes
        .iter()
        .map(|paragraphs| {
            let paras: String = paragraphs
                .iter()
                .map(|p| format!("<a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{p}</a:t></a:r></a:p>"))
                .collect();
            format!("<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Box\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:txBody><a:bodyPr/>{paras}</p:txBody></p:sp>")
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld {PML}><p:cSld><p:spTree>{shapes}</p:spTree></p:cSld></p:sld>"#
    )
}

/// A notes slide with a slide-image placeholder, the notes body and a slide-number
/// placeholder (whose text must not end up in the notes).
pub(crate) fn notes_xml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:notes {PML}><p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image"/><p:cNvSpPr/><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes"/><p:cNvSpPr/><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:txBody><a:bodyPr/><a:p><a:r><a:t>{body}</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="4" name="Slide Number"/><p:cNvSpPr/><p:nvPr><p:ph type="sldNum" idx="5"/></p:nvPr></p:nvSpPr><p:txBody><a:bodyPr/><a:p><a:fld id="{{X}}" type="slidenum"><a:t>SLIDE_NUMBER_42</a:t></a:fld></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:notes>"#
    )
}

// ---------------------------------------------------------------------------------------
// DOCX parts
// ---------------------------------------------------------------------------------------

const WML: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#;

/// `word/document.xml` with the given body XML.
pub(crate) fn document_xml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {WML}><w:body>{body}<w:sectPr/></w:body></w:document>"#
    )
}

/// One `<w:p>` with an optional paragraph style and a single run of text.
pub(crate) fn para(style: Option<&str>, text: &str) -> String {
    let props = style
        .map(|s| format!(r#"<w:pPr><w:pStyle w:val="{s}"/></w:pPr>"#))
        .unwrap_or_default();
    format!(r#"<w:p>{props}<w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

/// `word/styles.xml` defining paragraph styles as `(styleId, name)` pairs.
pub(crate) fn styles_xml(styles: &[(&str, &str)]) -> String {
    let styles: String = styles
        .iter()
        .map(|(id, name)| {
            format!(
                r#"<w:style w:type="paragraph" w:styleId="{id}"><w:name w:val="{name}"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr></w:style>"#
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {WML}>{styles}</w:styles>"#
    )
}
