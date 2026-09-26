//! Shared plumbing for the zip-based Office formats (`.pptx`, `.docx`):
//! - [`Package`]: reads parts out of the zip with zip-bomb limits;
//! - relationship (`_rels/*.rels`) parsing and target path resolution;
//! - small `quick-xml` helpers.
//!
//! Element and attribute names are matched by their *local* name (the part after `w:`,
//! `a:`, `p:` …), so documents that use unusual namespace prefixes still work.

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashMap;
use std::io::{self, Read, Seek};
use std::rc::Rc;

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use zip::ZipArchive;

use crate::util::{decode_text, failed, read_capped};
use crate::{ExtractError, Limits};

/// Most bytes opening a package may read: the search for the end-of-archive record (which
/// can follow a zip comment of up to 64 KiB) plus 1 KiB of central directory per allowed
/// entry. Real central-directory records are 50–150 bytes (46 fixed + name + extra fields).
fn open_read_budget(limits: &Limits) -> u64 {
    const END_RECORD_SEARCH_BYTES: u64 = 128 * 1024;
    const DIRECTORY_BYTES_PER_ENTRY: u64 = 1024;
    let entries = u64::try_from(limits.max_zip_entries).unwrap_or(u64::MAX);
    END_RECORD_SEARCH_BYTES.saturating_add(entries.saturating_mul(DIRECTORY_BYTES_PER_ENTRY))
}

/// An opened Office package (a zip archive of XML "parts").
///
/// Zip-bomb protection:
/// - Opening a zip reads its whole central directory (one record per entry) into memory
///   before the entries can be counted. While opening, reads are therefore capped by
///   [`open_read_budget`], so an archive listing millions of entries is refused after a few
///   MB instead of costing GBs; the entry count is checked again once the archive is open.
/// - Every part is decompressed through [`read_capped`], which stops as soon as the per-part
///   or per-file limit is exceeded (the sizes declared in the zip headers are not trusted).
pub(crate) struct Package<R> {
    archive: ZipArchive<BudgetedReader<R>>,
    /// Lower-cased part name → index, for case-insensitive lookups (first entry wins).
    lowercase_names: HashMap<String, usize>,
    limits: Limits,
    /// Decompressed bytes read so far, across all parts.
    bytes_read: u64,
}

impl<R: Read + Seek> Package<R> {
    pub(crate) fn open(reader: R, limits: &Limits) -> Result<Self, ExtractError> {
        let budget = Rc::new(Cell::new(open_read_budget(limits)));
        let reader = BudgetedReader {
            inner: reader,
            remaining: Rc::clone(&budget),
        };
        let archive = ZipArchive::new(reader).map_err(|e| {
            if budget.get() == 0 {
                failed(format!(
                    "Office file has too many zip entries (its zip directory is larger than {} bytes; at most {} entries are allowed)",
                    open_read_budget(limits),
                    limits.max_zip_entries
                ))
            } else {
                failed(format!(
                    "not a readable Office file (damaged, password-protected or an old binary format): {e}"
                ))
            }
        })?;
        // From now on `read_capped` limits what reading parts may decompress.
        budget.set(u64::MAX);
        if archive.len() > limits.max_zip_entries {
            return Err(failed(format!(
                "Office file has too many zip entries ({} > {})",
                archive.len(),
                limits.max_zip_entries
            )));
        }

        let mut lowercase_names = HashMap::new();
        for index in 0..archive.len() {
            if let Some(name) = archive.name_for_index(index) {
                lowercase_names
                    .entry(name.to_ascii_lowercase())
                    .or_insert(index);
            }
        }
        Ok(Package {
            archive,
            lowercase_names,
            limits: *limits,
            bytes_read: 0,
        })
    }

    /// Names of all parts in the package.
    pub(crate) fn part_names(&self) -> Vec<String> {
        self.archive.file_names().map(str::to_string).collect()
    }

    /// Read a part as text. `Ok(None)` if the part does not exist.
    pub(crate) fn read_text(&mut self, name: &str) -> Result<Option<String>, ExtractError> {
        Ok(self.read_bytes(name)?.map(|bytes| decode_text(&bytes)))
    }

    fn read_bytes(&mut self, name: &str) -> Result<Option<Vec<u8>>, ExtractError> {
        let Some(index) = self.find(name) else {
            return Ok(None);
        };
        let total_left = self
            .limits
            .max_zip_total_bytes
            .saturating_sub(self.bytes_read);
        let cap = self.limits.max_zip_entry_bytes.min(total_left);
        let too_large = || {
            failed(format!(
                "{name} is too large when decompressed (limits: {} bytes per part, {} bytes per file)",
                self.limits.max_zip_entry_bytes, self.limits.max_zip_total_bytes
            ))
        };

        let entry = self
            .archive
            .by_index(index)
            .map_err(|e| failed(format!("cannot read {name} from the zip: {e}")))?;
        // Cheap early exit using the declared size; `read_capped` enforces the real size.
        if entry.size() > cap {
            return Err(too_large());
        }
        let data = read_capped(entry, cap)
            .map_err(|e| failed(format!("cannot decompress {name}: {e}")))?
            .ok_or_else(too_large)?;
        self.bytes_read += data.len() as u64;
        Ok(Some(data))
    }

    /// Index of the part called `name`: exact match first, then case-insensitive (part
    /// names are case-insensitive in the Open Packaging Conventions).
    fn find(&self, name: &str) -> Option<usize> {
        self.archive.index_for_name(name).or_else(|| {
            self.lowercase_names
                .get(&name.to_ascii_lowercase())
                .copied()
        })
    }
}

/// A reader that fails once `remaining` bytes have been read through it. `remaining` is
/// shared with [`Package::open`], which lifts the limit once the archive is open.
struct BudgetedReader<R> {
    inner: R,
    remaining: Rc<Cell<u64>>,
}

impl<R: Read> Read for BudgetedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.remaining.get();
        if remaining == 0 && !buf.is_empty() {
            return Err(io::Error::other("read limit for the zip directory reached"));
        }
        let allowed = buf
            .len()
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        let read = self.inner.read(&mut buf[..allowed])?;
        self.remaining.set(remaining - read as u64);
        Ok(read)
    }
}

impl<R: Seek> Seek for BudgetedReader<R> {
    fn seek(&mut self, position: io::SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
}

/// One `<Relationship>` from a `.rels` part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Relationship {
    pub(crate) id: String,
    /// Full type URI, e.g. `…/relationships/slide`.
    pub(crate) rel_type: String,
    pub(crate) target: String,
}

impl Relationship {
    /// Whether the type URI ends with `/<kind>`, e.g. `is_kind("notesSlide")`.
    pub(crate) fn is_kind(&self, kind: &str) -> bool {
        self.rel_type
            .rsplit('/')
            .next()
            .is_some_and(|last| last == kind)
    }
}

/// The relationships of `part` (read from its `.rels` part); empty if there are none.
/// External targets (web links) are left out.
pub(crate) fn relationships_of<R: Read + Seek>(
    package: &mut Package<R>,
    part: &str,
) -> Result<Vec<Relationship>, ExtractError> {
    let rels_part = rels_path_for(part);
    match package.read_text(&rels_part)? {
        Some(xml) => parse_relationships(&xml, &rels_part),
        None => Ok(Vec::new()),
    }
}

/// The part that `part`'s first relationship of `kind` points to, or `default` if there is
/// none. `part = ""` means the package root (`_rels/.rels`), where the `officeDocument`
/// relationship names the main part. Office always uses the usual names
/// (`word/document.xml`, `ppt/presentation.xml` …), but other tools may not.
pub(crate) fn related_part<R: Read + Seek>(
    package: &mut Package<R>,
    part: &str,
    kind: &str,
    default: &str,
) -> Result<String, ExtractError> {
    let relationships = relationships_of(package, part)?;
    Ok(relationships
        .iter()
        .find(|rel| rel.is_kind(kind))
        .map_or_else(
            || default.to_string(),
            |rel| resolve_target(part, &rel.target),
        ))
}

fn parse_relationships(xml: &str, part: &str) -> Result<Vec<Relationship>, ExtractError> {
    let mut relationships = Vec::new();
    for_each_event(xml, part, |event| {
        if let Event::Start(e) | Event::Empty(e) = event
            && local_name(e) == "Relationship"
            && attr(e, "TargetMode").as_deref() != Some("External")
            && let (Some(id), Some(target)) = (attr(e, "Id"), attr(e, "Target"))
        {
            relationships.push(Relationship {
                id,
                rel_type: attr(e, "Type").unwrap_or_default(),
                target,
            });
        }
    })?;
    Ok(relationships)
}

/// Path of the `.rels` part for `part`: `ppt/slides/slide1.xml` →
/// `ppt/slides/_rels/slide1.xml.rels`.
pub(crate) fn rels_path_for(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// Resolve a relationship `target` against the folder of the part that owns the
/// relationship: (`ppt/slides/slide1.xml`, `../notesSlides/notesSlide1.xml`) →
/// `ppt/notesSlides/notesSlide1.xml`. Targets starting with `/` are package-absolute.
pub(crate) fn resolve_target(source_part: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        let mut dir: Vec<&str> = source_part.split('/').collect();
        dir.pop(); // drop the file name
        dir
    };
    for piece in target.split('/') {
        match piece {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            name => parts.push(name),
        }
    }
    parts.join("/")
}

/// Feed every event of `xml` to `handle`. Malformed XML → `ExtractError::Failed`.
pub(crate) fn for_each_event(
    xml: &str,
    part: &str,
    mut handle: impl FnMut(&Event),
) -> Result<(), ExtractError> {
    let mut reader = quick_xml::Reader::from_str(xml);
    loop {
        let event = reader.read_event().map_err(|e| {
            failed(format!(
                "malformed XML in {part} at byte {}: {e}",
                reader.error_position()
            ))
        })?;
        if matches!(event, Event::Eof) {
            return Ok(());
        }
        handle(&event);
    }
}

/// Local name of an element (`w:p` → `p`).
pub(crate) fn local_name<'a>(element: &'a BytesStart) -> &'a str {
    let name = element.name().0;
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

/// Local name of an end tag (`</w:p>` → `p`).
pub(crate) fn end_local_name<'a>(end: &'a quick_xml::events::BytesEnd) -> &'a str {
    let name = end.name().0;
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

/// Unescaped value of the attribute whose local name is `local` (prefix ignored), if any.
pub(crate) fn attr(element: &BytesStart, local: &str) -> Option<String> {
    attribute_where(element, |key| {
        key.rsplit_once(':').map_or(key, |(_, name)| name) == local
    })
}

/// Unescaped value of the first attribute whose full key (`r:id`) satisfies `matches`.
pub(crate) fn attribute_where(
    element: &BytesStart,
    matches: impl Fn(&str) -> bool,
) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|a| matches(a.key.0))
        .map(
            |a| match a.normalized_value(quick_xml::XmlVersion::Implicit1_0) {
                Ok(value) => value.into_owned(),
                Err(_) => a.value.into_owned(),
            },
        )
}

/// The character data carried by a text-like event, with entities resolved:
/// text, CDATA, and `&amp;` / `&#233;` style references (reported separately by quick-xml).
pub(crate) fn event_text<'a>(event: &'a Event) -> Option<Cow<'a, str>> {
    match event {
        Event::Text(text) => Some(text.xml10_content()),
        Event::CData(data) => Some(data.xml10_content()),
        Event::GeneralRef(reference) => match reference.resolve_char_ref() {
            Ok(Some(c)) => Some(Cow::Owned(c.to_string())),
            Ok(None) => resolve_predefined_entity(reference).map(Cow::Borrowed),
            Err(_) => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io::Cursor;
    use std::rc::Rc;

    use super::*;
    use crate::test_support::{zip_bytes, zip_text};

    fn small_limits() -> Limits {
        Limits {
            max_zip_entries: 5,
            max_zip_entry_bytes: 1_000,
            max_zip_total_bytes: 1_500,
            ..Limits::DEFAULT
        }
    }

    fn open(bytes: Vec<u8>, limits: &Limits) -> Result<Package<Cursor<Vec<u8>>>, ExtractError> {
        Package::open(Cursor::new(bytes), limits)
    }

    #[test]
    fn reads_parts_and_reports_missing_ones() {
        let mut package = open(zip_text(&[("a/b.xml", "<x/>")]), &small_limits()).unwrap();
        assert_eq!(
            package.read_text("a/b.xml").unwrap().as_deref(),
            Some("<x/>")
        );
        assert_eq!(
            package.read_text("A/B.XML").unwrap().as_deref(),
            Some("<x/>")
        );
        assert_eq!(package.read_text("missing.xml").unwrap(), None);
        assert_eq!(package.part_names(), vec!["a/b.xml".to_string()]);
    }

    #[test]
    fn entry_larger_than_cap_is_refused() {
        let big = vec![b'x'; 2_000];
        let mut package = open(zip_bytes(&[("big.xml", &big)]), &small_limits()).unwrap();
        let error = package.read_text("big.xml").unwrap_err();
        assert!(
            matches!(&error, ExtractError::Failed(m) if m.contains("too large")),
            "{error}"
        );
    }

    #[test]
    fn total_decompressed_size_is_capped() {
        let part = vec![b'x'; 800];
        let bytes = zip_bytes(&[("one.xml", &part), ("two.xml", &part)]);
        let mut package = open(bytes, &small_limits()).unwrap();
        assert!(package.read_text("one.xml").unwrap().is_some());
        assert!(matches!(
            package.read_text("two.xml"),
            Err(ExtractError::Failed(_))
        ));
    }

    #[test]
    fn highly_compressible_entry_is_refused() {
        // A classic zip bomb shape: tiny on disk, huge when inflated.
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("bomb.xml", options).unwrap();
        std::io::Write::write_all(&mut writer, &vec![b'0'; 200_000]).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert!(bytes.len() < 5_000);
        let mut package = open(bytes, &small_limits()).unwrap();
        assert!(matches!(
            package.read_text("bomb.xml"),
            Err(ExtractError::Failed(_))
        ));
    }

    #[test]
    fn lying_size_header_is_caught_while_reading() {
        // Deflate 5,000 bytes, then patch both size headers to claim 10 bytes: the cheap
        // declared-size check passes, so only the capped read can catch the real size.
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("liar.xml", options).unwrap();
        std::io::Write::write_all(&mut writer, &vec![b'0'; 5_000]).unwrap();
        let mut bytes = writer.finish().unwrap().into_inner();
        let claimed = 10u32.to_le_bytes();
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        bytes[22..26].copy_from_slice(&claimed); // local header: uncompressed size
        let central = bytes
            .windows(4)
            .position(|w| w == b"PK\x01\x02")
            .expect("central directory header");
        bytes[central + 24..central + 28].copy_from_slice(&claimed); // same, central copy

        let mut package = open(bytes, &small_limits()).unwrap();
        let error = package.read_text("liar.xml").unwrap_err();
        assert!(
            matches!(&error, ExtractError::Failed(m) if m.contains("too large")),
            "{error}"
        );
    }

    /// Wraps a reader and counts the bytes read through it.
    struct CountingReader {
        inner: Cursor<Vec<u8>>,
        bytes_read: Rc<Cell<u64>>,
    }

    impl Read for CountingReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.inner.read(buf)?;
            self.bytes_read.set(self.bytes_read.get() + n as u64);
            Ok(n)
        }
    }

    impl Seek for CountingReader {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(pos)
        }
    }

    #[test]
    fn huge_entry_list_is_refused_before_it_is_all_read() {
        // 3,000 entries → a central directory of ~180 KB, far more than 5 entries need.
        let names: Vec<String> = (0..3_000).map(|i| format!("part{i:05}.xml")).collect();
        let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b""[..])).collect();
        let bytes = zip_bytes(&entries);
        let bytes_read = Rc::new(Cell::new(0));
        let reader = CountingReader {
            inner: Cursor::new(bytes),
            bytes_read: Rc::clone(&bytes_read),
        };
        let result = Package::open(reader, &small_limits());
        assert!(
            matches!(&result, Err(ExtractError::Failed(m)) if m.contains("too many zip entries")),
            "{:?}",
            result.err()
        );
        // Opening may read the end-of-archive search area plus ~1 KB per allowed entry.
        let allowed = open_read_budget(&small_limits());
        assert!(
            bytes_read.get() <= allowed,
            "read {} bytes, budget {allowed}",
            bytes_read.get()
        );
    }

    #[test]
    fn case_insensitive_lookup_of_missing_parts_is_fast() {
        // Regression: every miss used to scan all entry names.
        let names: Vec<String> = (0..10_000).map(|i| format!("ppt/media/m{i}.bin")).collect();
        let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b""[..])).collect();
        let limits = Limits {
            max_zip_entries: 10_000,
            ..Limits::DEFAULT
        };
        let mut package = open(zip_bytes(&entries), &limits).unwrap();
        let started = std::time::Instant::now();
        for i in 0..50_000 {
            let name = format!("ppt/slides/slide{i}.xml");
            assert!(package.read_text(&name).unwrap().is_none());
        }
        assert!(package.read_text("PPT/MEDIA/M9999.BIN").unwrap().is_some());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn too_many_entries_is_refused() {
        let entries: Vec<(String, &str)> = (0..6).map(|i| (format!("{i}.xml"), "x")).collect();
        let entries: Vec<(&str, &str)> = entries.iter().map(|(n, t)| (n.as_str(), *t)).collect();
        assert!(matches!(
            open(zip_text(&entries), &small_limits()),
            Err(ExtractError::Failed(_))
        ));
    }

    #[test]
    fn not_a_zip_is_failed() {
        assert!(matches!(
            open(b"not a zip at all".to_vec(), &Limits::DEFAULT),
            Err(ExtractError::Failed(_))
        ));
    }

    #[test]
    fn relationships_are_parsed_and_external_ones_skipped() {
        let xml = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
            <Relationship Id="rId1" Type="http://x/relationships/slide" Target="slides/slide1.xml"/>
            <Relationship Id="rId2" Type="http://x/relationships/hyperlink" Target="https://example.edu/?a=1&amp;b=2" TargetMode="External"/>
            <Relationship Id="rId3" Type="http://x/relationships/notesSlide" Target="../notesSlides/notesSlide7.xml"/>
        </Relationships>"#;
        let rels = parse_relationships(xml, "test.rels").unwrap();
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].id, "rId1");
        assert!(rels[0].is_kind("slide"));
        assert!(!rels[0].is_kind("notesSlide"));
        assert!(rels[1].is_kind("notesSlide"));
        assert_eq!(rels[1].target, "../notesSlides/notesSlide7.xml");
    }

    #[test]
    fn malformed_xml_is_failed() {
        let result = for_each_event("<a><b></a>", "x.xml", |_| {});
        assert!(matches!(result, Err(ExtractError::Failed(m)) if m.contains("x.xml")));
    }

    #[test]
    fn related_part_follows_relationships_or_uses_default() {
        let root_rels = crate::test_support::rels_xml(&[
            ("rId1", "metadata/core-properties", "docProps/core.xml"),
            ("rId2", "officeDocument", "word/document2.xml"),
        ]);
        let bytes = zip_text(&[("_rels/.rels", &root_rels)]);
        let mut package = open(bytes, &small_limits()).unwrap();
        assert_eq!(
            related_part(&mut package, "", "officeDocument", "word/document.xml").unwrap(),
            "word/document2.xml"
        );
        assert_eq!(
            related_part(
                &mut package,
                "word/document2.xml",
                "styles",
                "word/styles.xml"
            )
            .unwrap(),
            "word/styles.xml"
        );
    }

    #[test]
    fn rels_paths_and_targets_resolve() {
        assert_eq!(
            rels_path_for("ppt/presentation.xml"),
            "ppt/_rels/presentation.xml.rels"
        );
        assert_eq!(rels_path_for("doc.xml"), "_rels/doc.xml.rels");
        assert_eq!(rels_path_for(""), "_rels/.rels");
        assert_eq!(
            resolve_target("", "/word/document.xml"),
            "word/document.xml"
        );
        assert_eq!(resolve_target("", "word/document.xml"), "word/document.xml");
        assert_eq!(
            resolve_target("ppt/presentation.xml", "slides/slide2.xml"),
            "ppt/slides/slide2.xml"
        );
        assert_eq!(
            resolve_target("ppt/slides/slide1.xml", "../notesSlides/./notesSlide3.xml"),
            "ppt/notesSlides/notesSlide3.xml"
        );
        assert_eq!(
            resolve_target("ppt/slides/slide1.xml", "/ppt/media/image1.png"),
            "ppt/media/image1.png"
        );
    }

    #[test]
    fn event_text_resolves_entities() {
        let mut out = String::new();
        for_each_event(
            "<t>A &amp; B &#233;&#x4E2D; <![CDATA[<raw>]]></t>",
            "x",
            |event| {
                if let Some(text) = event_text(event) {
                    out.push_str(&text);
                }
            },
        )
        .unwrap();
        assert_eq!(out, "A & B é中 <raw>");
    }

    #[test]
    fn names_and_attributes_ignore_prefixes() {
        for_each_event(
            r#"<w:pStyle w:val="Heading1" r:id="rId5" id="7"/>"#,
            "x",
            |event| {
                if let Event::Empty(e) = event {
                    assert_eq!(local_name(e), "pStyle");
                    assert_eq!(attr(e, "val").as_deref(), Some("Heading1"));
                    assert_eq!(
                        attribute_where(e, |key| key.ends_with(":id")).as_deref(),
                        Some("rId5")
                    );
                }
            },
        )
        .unwrap();
    }
}
