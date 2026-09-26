//! Decompression-bomb checks for PDFs.
//!
//! `lopdf` inflates streams without any size limit (object streams while loading, page
//! contents, fonts, colour profiles and every drawn XObject while `pdf-extract` reads a
//! page), so a few hundred KB of PDF can grow to gigabytes of memory. Two checks keep that
//! bounded; both decode through a counting reader with a cap (nothing is kept beyond one
//! 64 KB buffer, plus an ASCII-decoded copy that is smaller than the raw stream):
//!
//! 1. `check`, before `lopdf` sees the file: scans the raw bytes for
//!    `<< … >> stream EOL … endstream` and measures every stream except images. This protects
//!    what `lopdf` decodes while loading (object and cross-reference streams). It is a
//!    heuristic over raw bytes and deliberately lenient (what it can't make sense of is
//!    skipped, never refused).
//! 2. `defuse`, right after loading, on the objects `lopdf` actually parsed: image streams
//!    (`/Subtype /Image`) are emptied — text extraction never needs their bytes, and
//!    `pdf-extract` would otherwise decode and parse drawn images as content streams — and
//!    every other stream is measured with the filters `lopdf` will apply. This one is exact.
//!
//! Filters are followed in order: `ASCIIHexDecode`/`ASCII85Decode` before the first
//! `FlateDecode` are decoded on the way, every `FlateDecode` layer is inflated, and any other
//! filter before a `FlateDecode` makes the file refused (its expansion can't be measured).
//! Limits: `Caps::per_stream` for one stream, `Caps::total` for all together. Known gaps,
//! until extraction runs in a separate process: `LZWDecode` alone is not measured (its worst
//! ratio is far smaller), and the raw scan can be misled about what is an image (the stream
//! is then emptied by `defuse` unless it is an object stream, which `lopdf` decodes while
//! loading).

use std::borrow::Cow;
use std::io::Read;

use flate2::read::ZlibDecoder;

use crate::ExtractError;
use crate::util::{failed, size_text};

/// Longest stream dictionary we look back over (they are normally < 1 KB).
const MAX_DICT_BYTES: usize = 64 * 1024;
/// Most `FlateDecode` layers applied to one stream (a chain can multiply the ratio).
const MAX_LAYERS: usize = 4;

/// How far one stream, and all streams together, may expand.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Caps {
    pub per_stream: u64,
    pub total: u64,
}

/// Running total of the bytes measured so far against `Caps`.
pub(crate) struct Budget {
    caps: Caps,
    sum: u64,
}

impl Budget {
    pub(crate) fn new(caps: Caps) -> Budget {
        Budget { caps, sum: 0 }
    }

    /// Measure one stream whose filters are `plan`.
    pub(crate) fn measure(&mut self, plan: Plan, raw: &[u8]) -> Result<(), ExtractError> {
        let (ascii, layers) = match plan {
            Plan::Skip => return Ok(()),
            Plan::Refuse => {
                return Err(failed(
                    "PDF encodes its content in a way whose size can't be checked, so it was not \
                     indexed",
                ));
            }
            Plan::Measure { ascii, layers } => (ascii, layers),
        };
        let mut data = Cow::Borrowed(raw);
        for filter in ascii {
            data = Cow::Owned(filter.decode(&data));
        }
        let per_stream = self.caps.per_stream;
        let size = inflated_size(&data, layers, per_stream.saturating_add(1));
        if size > per_stream {
            return Err(too_large(per_stream));
        }
        self.sum += size;
        if self.sum > self.caps.total {
            return Err(too_large(self.caps.total));
        }
        Ok(())
    }
}

fn too_large(limit: u64) -> ExtractError {
    failed(format!(
        "PDF would expand to more than {} when opened, so it was not indexed",
        size_text(limit)
    ))
}

/// Check 1 (see the module docs): refuse the raw PDF in `bytes` if a non-image stream would
/// inflate past `caps`.
pub(crate) fn check(bytes: &[u8], caps: Caps) -> Result<(), ExtractError> {
    let mut budget = Budget::new(caps);
    let mut pos = 0;
    while let Some(found) = find(&bytes[pos..], b"stream") {
        let keyword = pos + found;
        pos = keyword + b"stream".len();
        if keyword >= 3 && &bytes[keyword - 3..keyword] == b"end" {
            continue; // the end of a stream, not its start
        }
        // The keyword is followed by an end of line (spaces or tabs before it are tolerated,
        // as `lopdf` does; a lone CR too).
        let mut start = pos;
        while matches!(bytes.get(start), Some(b' ' | b'\t')) {
            start += 1;
        }
        let eol_start = start;
        if bytes.get(start) == Some(&b'\r') {
            start += 1;
        }
        if bytes.get(start) == Some(&b'\n') {
            start += 1;
        }
        if start == eol_start {
            continue; // e.g. a name like /Downstream
        }
        let Some(dict) = dictionary_before(bytes, keyword) else {
            continue;
        };
        let end = find(&bytes[start..], b"endstream").map_or(bytes.len(), |e| start + e);
        pos = end;
        if is_image(dict) && !decoded_while_loading(dict) {
            continue; // emptied by `defuse` before anything decodes it
        }
        budget.measure(raw_plan(dict), &bytes[start..end])?;
    }
    Ok(())
}

/// An ASCII filter we can undo on the way to the first `FlateDecode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AsciiFilter {
    Hex,
    Base85,
}

impl AsciiFilter {
    fn decode(self, data: &[u8]) -> Vec<u8> {
        match self {
            AsciiFilter::Hex => ascii_hex(data),
            AsciiFilter::Base85 => ascii85(data),
        }
    }
}

/// What to do with one stream, from its filter chain.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    /// No `FlateDecode` in the chain.
    Skip,
    /// Undo `ascii`, then inflate `layers` times.
    Measure {
        ascii: Vec<AsciiFilter>,
        layers: usize,
    },
    /// Another filter comes before a `FlateDecode`.
    Refuse,
}

/// The plan for a filter chain (names without the leading `/`, in decoding order).
pub(crate) fn plan_for<S: AsRef<str>>(filters: &[S]) -> Plan {
    let is_flate = |f: &str| matches!(f, "FlateDecode" | "Fl");
    let Some(last_flate) = filters.iter().rposition(|f| is_flate(f.as_ref())) else {
        return Plan::Skip;
    };
    let mut ascii = Vec::new();
    let mut layers = 0;
    for filter in &filters[..=last_flate] {
        match filter.as_ref() {
            f if is_flate(f) => layers += 1,
            "ASCIIHexDecode" | "AHx" if layers == 0 => ascii.push(AsciiFilter::Hex),
            "ASCII85Decode" | "A85" if layers == 0 => ascii.push(AsciiFilter::Base85),
            _ => return Plan::Refuse,
        }
    }
    Plan::Measure {
        ascii,
        layers: layers.min(MAX_LAYERS),
    }
}

/// The plan for a raw stream dictionary. When its top-level `/Filter` can't be read (e.g. an
/// indirect reference) but `/FlateDecode` appears in it anyway, every occurrence counts as a
/// layer over the raw bytes (the cautious reading).
fn raw_plan(dict: &[u8]) -> Plan {
    match filter_names(dict) {
        Some(filters) => plan_for(&filters),
        None => match count(dict, b"/FlateDecode") {
            0 => Plan::Skip,
            n => Plan::Measure {
                ascii: Vec::new(),
                layers: n.min(MAX_LAYERS),
            },
        },
    }
}

/// PDF delimiters and white space end a name.
fn ends_name(byte: u8) -> bool {
    byte.is_ascii_whitespace() || b"()<>[]{}/%".contains(&byte)
}

/// The bytes after the top-level key `/key` of `dict` (`<< … >>`), up to the dictionary's
/// end. Keys of nested dictionaries, names inside arrays, strings and comments don't count,
/// and the name must match as a whole (`/FilterX` is not `/Filter`).
fn top_level_value<'a>(dict: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let (mut depth, mut array, mut i) = (0usize, 0usize, 0usize);
    while i < dict.len() {
        match dict[i] {
            b'%' => {
                while i < dict.len() && !matches!(dict[i], b'\n' | b'\r') {
                    i += 1;
                }
            }
            b'(' => {
                let mut level = 0usize;
                while i < dict.len() {
                    match dict[i] {
                        b'\\' => i += 1,
                        b'(' => level += 1,
                        b')' => {
                            level -= 1;
                            if level == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'<' if dict.get(i + 1) == Some(&b'<') => {
                depth += 1;
                i += 1;
            }
            b'<' => {
                while i < dict.len() && dict[i] != b'>' {
                    i += 1;
                }
            }
            b'>' if dict.get(i + 1) == Some(&b'>') => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'[' => array += 1,
            b']' => array = array.saturating_sub(1),
            b'/' => {
                let start = i + 1;
                let mut end = start;
                while end < dict.len() && !ends_name(dict[end]) {
                    end += 1;
                }
                if depth == 1 && array == 0 && &dict[start..end] == key {
                    return Some(&dict[end..]);
                }
                i = end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// A name at the start of `bytes` (after white space): its text and the bytes after it.
fn leading_name(bytes: &[u8]) -> Option<(String, &[u8])> {
    let body = bytes.trim_ascii_start().strip_prefix(b"/")?;
    let len = body
        .iter()
        .position(|&b| ends_name(b))
        .unwrap_or(body.len());
    (len > 0).then(|| {
        (
            String::from_utf8_lossy(&body[..len]).into_owned(),
            &body[len..],
        )
    })
}

/// The names of the top-level `/Filter` (a single `/Name` or an array of them), in order;
/// `None` when there is none we can read (e.g. an indirect reference).
fn filter_names(dict: &[u8]) -> Option<Vec<String>> {
    let value = top_level_value(dict, b"Filter")?.trim_ascii_start();
    if let Some(mut list) = value.strip_prefix(b"[") {
        let mut names = Vec::new();
        loop {
            list = list.trim_ascii_start();
            if list.starts_with(b"]") {
                return Some(names);
            }
            let (filter, rest) = leading_name(list)?;
            names.push(filter);
            list = rest;
        }
    }
    leading_name(value).map(|(filter, _)| vec![filter])
}

/// Top-level `/Subtype /Image`.
fn is_image(dict: &[u8]) -> bool {
    top_level_value(dict, b"Subtype")
        .and_then(leading_name)
        .is_some_and(|(name, _)| name == "Image")
}

/// Object and cross-reference streams: `lopdf` decodes them while loading, before `defuse`.
fn decoded_while_loading(dict: &[u8]) -> bool {
    top_level_value(dict, b"Type")
        .and_then(leading_name)
        .is_some_and(|(name, _)| matches!(name.as_str(), "ObjStm" | "XRef"))
}

/// `ASCIIHexDecode`: hex digit pairs, whitespace ignored, `>` ends the data.
fn ascii_hex(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 2);
    let mut high: Option<u8> = None;
    for &byte in data {
        if byte == b'>' {
            break;
        }
        let Some(value) = (byte as char).to_digit(16) else {
            continue;
        };
        match high.take() {
            Some(high) => out.push((high << 4) | value as u8),
            None => high = Some(value as u8),
        }
    }
    if let Some(high) = high {
        out.push(high << 4);
    }
    out
}

/// `ASCII85Decode`: groups of five characters `!`..`u` → four bytes, `z` → four zero bytes,
/// `~>` ends the data. Invalid input just ends the decoding (the check stays lenient).
fn ascii85(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 5 * 4 + 4);
    let data = data.strip_prefix(b"<~").unwrap_or(data);
    let (mut value, mut count) = (0u32, 0usize);
    for &byte in data {
        match byte {
            b'~' => break,
            b'z' if count == 0 => out.extend_from_slice(&[0; 4]),
            b'!'..=b'u' => {
                value = value.wrapping_mul(85).wrapping_add(u32::from(byte - b'!'));
                count += 1;
                if count == 5 {
                    out.extend_from_slice(&value.to_be_bytes());
                    (value, count) = (0, 0);
                }
            }
            byte if byte.is_ascii_whitespace() => {}
            _ => break,
        }
    }
    if count > 1 {
        for _ in count..5 {
            value = value.wrapping_mul(85).wrapping_add(84);
        }
        out.extend_from_slice(&value.to_be_bytes()[..count - 1]);
    }
    out
}

/// The `<< … >>` dictionary that ends right before `keyword` (white space and comment lines
/// allowed in between, as `lopdf` does), matching nested `<< >>` pairs backwards.
fn dictionary_before(bytes: &[u8], keyword: usize) -> Option<&[u8]> {
    let mut end = keyword;
    for _ in 0..8 {
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        if end >= 2 && &bytes[end - 2..end] == b">>" {
            break;
        }
        // A comment ending the line before `stream`: continue before its `%`.
        let line_start = bytes[..end]
            .iter()
            .rposition(|&b| matches!(b, b'\n' | b'\r'))
            .map_or(0, |at| at + 1);
        end = line_start + bytes[line_start..end].iter().position(|&b| b == b'%')?;
    }
    if end < 2 || &bytes[end - 2..end] != b">>" {
        return None;
    }
    let floor = end.saturating_sub(MAX_DICT_BYTES);
    let mut depth = 0usize;
    let mut i = end;
    while i >= floor + 2 {
        match &bytes[i - 2..i] {
            b">>" => {
                depth += 1;
                i -= 2;
            }
            b"<<" => {
                depth -= 1;
                i -= 2;
                if depth == 0 {
                    return Some(&bytes[i..end]);
                }
            }
            _ => i -= 1,
        }
    }
    None
}

/// Bytes `data` inflates to through `layers` zlib layers, counting at most up to `cap`.
/// Data that isn't (or stops being) valid zlib counts what came out until then.
fn inflated_size(data: &[u8], layers: usize, cap: u64) -> u64 {
    let mut reader: Box<dyn Read + '_> = Box::new(data);
    for _ in 0..layers {
        reader = Box::new(ZlibDecoder::new(reader));
    }
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    while size < cap {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => size += n as u64,
        }
    }
    size
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    use super::*;

    const CAPS: Caps = Caps {
        per_stream: 1_000_000,
        total: 10_000_000,
    };

    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn hex(data: &[u8]) -> Vec<u8> {
        let mut out: Vec<u8> = data
            .iter()
            .flat_map(|b| format!("{b:02X}").into_bytes())
            .collect();
        out.push(b'>');
        out
    }

    fn base85(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for chunk in data.chunks(4) {
            let mut group = [0u8; 4];
            group[..chunk.len()].copy_from_slice(chunk);
            let mut value = u32::from_be_bytes(group);
            let mut digits = [0u8; 5];
            for digit in digits.iter_mut().rev() {
                *digit = (value % 85) as u8 + b'!';
                value /= 85;
            }
            out.extend_from_slice(&digits[..chunk.len() + 1]);
        }
        out.extend_from_slice(b"~>");
        out
    }

    /// `1 0 obj <<dict>> stream … endstream endobj` around `data`.
    fn object(dict: &str, data: &[u8]) -> Vec<u8> {
        let mut out =
            format!("1 0 obj\n<< {dict} /Length {} >>\nstream\r\n", data.len()).into_bytes();
        out.extend_from_slice(data);
        out.extend_from_slice(b"\nendstream\nendobj\n");
        out
    }

    fn pdf(objects: &[Vec<u8>]) -> Vec<u8> {
        let mut out = b"%PDF-1.5\n".to_vec();
        for object in objects {
            out.extend_from_slice(object);
        }
        out.extend_from_slice(b"%%EOF\n");
        out
    }

    fn refused(file: &[u8], caps: Caps) -> bool {
        check(file, caps).is_err()
    }

    #[test]
    fn a_stream_that_inflates_past_the_cap_is_refused() {
        let bomb = zlib(&vec![b' '; 2_000_000]);
        assert!(bomb.len() < 10_000, "a real bomb: {} bytes", bomb.len());
        let file = pdf(&[object("/Filter /FlateDecode", &bomb)]);
        let err = check(&file, CAPS).unwrap_err();
        assert!(matches!(err, ExtractError::Failed(m) if m.contains("would expand")));
        let roomy = Caps {
            per_stream: 3_000_000,
            ..CAPS
        };
        assert!(!refused(&file, roomy));
    }

    #[test]
    fn the_total_of_all_streams_is_capped_too() {
        let part = zlib(&vec![b'q'; 600_000]);
        let file = pdf(&[
            object("/Filter [/FlateDecode]", &part),
            object("/Filter/FlateDecode /DecodeParms << /Predictor 1 >>", &part),
        ]);
        let caps = |total| Caps { total, ..CAPS };
        assert!(!refused(&file, caps(1_500_000)));
        assert!(refused(&file, caps(1_000_000)));
    }

    #[test]
    fn chained_flate_layers_are_all_counted() {
        let twice = zlib(&zlib(&vec![b' '; 2_000_000]));
        let file = pdf(&[object("/Filter [/FlateDecode /FlateDecode]", &twice)]);
        assert!(refused(&file, CAPS));
    }

    #[test]
    fn ascii_filters_before_flate_are_decoded_on_the_way() {
        let bomb = zlib(&vec![b' '; 2_000_000]);
        for (dict, data) in [
            ("/Filter [/ASCIIHexDecode /FlateDecode]", hex(&bomb)),
            ("/Filter [/ASCII85Decode /FlateDecode]", base85(&bomb)),
            ("/Filter [/A85 /Fl]", base85(&bomb)),
        ] {
            assert!(refused(&pdf(&[object(dict, &data)]), CAPS), "{dict}");
        }
        // Small content through the same chains passes.
        let small = zlib(b"BT /F1 12 Tf (Alpha) Tj ET");
        let file = pdf(&[
            object("/Filter [/ASCIIHexDecode /FlateDecode]", &hex(&small)),
            object("/Filter [/ASCII85Decode /FlateDecode]", &base85(&small)),
        ]);
        assert!(!refused(&file, CAPS));
        assert_eq!(ascii85(&base85(b"Alpha!")), b"Alpha!");
        assert_eq!(ascii_hex(b"41 6c 7>"), b"Alp");
    }

    #[test]
    fn another_filter_before_flate_is_refused() {
        let data = zlib(b"anything");
        for dict in [
            "/Filter [/LZWDecode /FlateDecode]",
            "/Filter [/FlateDecode /ASCII85Decode /FlateDecode]",
        ] {
            assert!(refused(&pdf(&[object(dict, &data)]), CAPS), "{dict}");
        }
        // After the last Flate anything goes; without Flate nothing is measured.
        let file = pdf(&[
            object("/Filter [/FlateDecode /DCTDecode]", &data),
            object("/Filter /LZWDecode", b"raw"),
            object("/Filter 7 0 R", b"raw"),
        ]);
        assert!(!refused(&file, CAPS));
    }

    #[test]
    fn images_are_left_to_defuse_except_streams_lopdf_decodes_while_loading() {
        let scan = zlib(&vec![0u8; 4_000_000]);
        let image = "/Type /XObject /Subtype /Image /Filter /FlateDecode";
        let pages = pdf(&[object(image, &scan), object(image, &scan)]);
        assert!(!refused(&pages, CAPS), "images are emptied after loading");
        // An object stream is decoded while loading, whatever else its dictionary says.
        let sneaky = "/Type /ObjStm /Subtype /Image /Filter /FlateDecode";
        assert!(refused(&pdf(&[object(sneaky, &scan)]), CAPS));
    }

    #[test]
    fn only_the_top_level_filter_key_counts() {
        let bomb = zlib(&vec![b' '; 2_000_000]);
        for dict in [
            // A longer key that merely starts with "Filter".
            "/FilterX /Nothing /Filter /FlateDecode",
            // A nested dictionary with its own /Filter before the real one.
            "/DecodeParms << /Filter /None >> /Filter /FlateDecode",
            // A /Filter inside a string or a comment is not a key.
            "/Title (/Filter /None) /Filter [/FlateDecode]",
            // An image lookalike in a nested dictionary.
            "/Extra << /Subtype /Image >> /Filter /FlateDecode",
        ] {
            assert!(refused(&pdf(&[object(dict, &bomb)]), CAPS), "{dict}");
        }
        assert_eq!(
            filter_names(b"<< /FilterX /A /Filter [/B /C] >>"),
            Some(vec!["B".to_string(), "C".to_string()])
        );
        assert_eq!(filter_names(b"<< /Filter 7 0 R >>"), None);
        // Unreadable (indirect) filter but /FlateDecode in the dictionary: measured anyway.
        let dict = "/Filter 7 0 R /Note /FlateDecode";
        assert!(refused(&pdf(&[object(dict, &bomb)]), CAPS), "{dict}");
    }

    #[test]
    fn streams_are_found_as_lopdf_finds_them() {
        let bomb = zlib(&vec![b' '; 2_000_000]);
        let mut spaced = b"1 0 obj\n<< /Filter /FlateDecode >>\nstream  \t\r\n".to_vec();
        spaced.extend_from_slice(&bomb);
        spaced.extend_from_slice(b"\nendstream\nendobj\n");
        let mut commented =
            b"1 0 obj\n<< /Filter /FlateDecode >> % made by demo\nstream\n".to_vec();
        commented.extend_from_slice(&bomb);
        commented.extend_from_slice(b"\nendstream\nendobj\n");
        for file in [spaced, commented] {
            assert!(refused(&pdf(&[file]), CAPS));
        }
    }

    #[test]
    fn other_filters_and_garbage_are_not_refused() {
        let file = pdf(&[
            object("/Filter /DCTDecode", &[0xFF, 0xD8, 0xFF]),
            object("/Filter /FlateDecode", b"not zlib at all"),
            b"4 0 obj << /Name /Downstream >> endobj\n".to_vec(),
            // Truncated: no endstream.
            b"5 0 obj << /Filter /FlateDecode >> stream\n\x78".to_vec(),
        ]);
        assert!(!refused(&file, CAPS));
        assert!(check(b"", CAPS).is_ok());
    }
}
