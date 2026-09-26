//! Decompression-bomb check for PDFs, run before `lopdf` sees the file.
//!
//! `lopdf` inflates streams without any size limit (object streams while loading, page
//! contents, fonts and colour profiles while `pdf-extract` reads a page), so a few hundred KB
//! of PDF can grow to gigabytes of memory. Here every stream with `FlateDecode` in its filter
//! chain is decoded first with a cap, counting bytes only (nothing is kept beyond one 64 KB
//! buffer, plus an ASCII-decoded copy that is smaller than the raw stream), and the file is
//! refused when one stream or all of them together would grow too large.
//!
//! The scan works on the raw bytes: each `<< … >> stream EOL … endstream`, reading `/Filter`
//! in order. `ASCIIHexDecode`/`ASCII85Decode` before the first `FlateDecode` are decoded on
//! the way; any other filter before a `FlateDecode` makes the file refused (its expansion
//! can't be measured). Image streams (`/Subtype /Image`) are measured against their own,
//! larger per-stream cap and don't count towards the total (scanned pages are legitimately
//! large, and text extraction never decodes them). Anything else the scan can't make sense
//! of is skipped, never refused. Known gaps, until extraction runs in a separate process:
//! filters other than Flate/ASCII are not measured on their own (`LZWDecode`'s worst ratio
//! is far smaller), and encrypted streams (ciphertext does not inflate) are not measured.

use std::borrow::Cow;
use std::io::Read;

use flate2::read::ZlibDecoder;

use crate::ExtractError;
use crate::util::{failed, size_text};

/// Longest stream dictionary we look back over (they are normally < 1 KB).
const MAX_DICT_BYTES: usize = 64 * 1024;
/// Most `FlateDecode` layers applied to one stream (a chain can multiply the ratio).
const MAX_LAYERS: usize = 4;

/// How far one stream, one image stream, and all non-image streams together may expand.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Caps {
    pub per_stream: u64,
    pub per_image: u64,
    pub total: u64,
}

/// Refuse the PDF in `bytes` if a stream would inflate past `caps` (see the module docs).
pub(crate) fn check(bytes: &[u8], caps: Caps) -> Result<(), ExtractError> {
    let too_large = |limit: u64| {
        failed(format!(
            "PDF would expand to more than {} when opened, so it was not indexed",
            size_text(limit)
        ))
    };
    let mut sum = 0u64;
    let mut pos = 0;
    while let Some(found) = find(&bytes[pos..], b"stream") {
        let keyword = pos + found;
        pos = keyword + b"stream".len();
        if keyword >= 3 && &bytes[keyword - 3..keyword] == b"end" {
            continue; // the end of a stream, not its start
        }
        // The keyword is followed by CRLF or LF (a lone CR is tolerated).
        let mut start = pos;
        if bytes.get(start) == Some(&b'\r') {
            start += 1;
        }
        if bytes.get(start) == Some(&b'\n') {
            start += 1;
        }
        if start == pos {
            continue; // e.g. a name like /Downstream
        }
        let Some(dict) = dictionary_before(bytes, keyword) else {
            continue;
        };
        let end = find(&bytes[start..], b"endstream").map_or(bytes.len(), |e| start + e);
        pos = end;
        let (ascii, layers) = match plan(dict) {
            Plan::Skip => continue,
            Plan::Refuse => {
                return Err(failed(
                    "PDF encodes its content in a way whose size can't be checked, so it was not \
                     indexed",
                ));
            }
            Plan::Measure { ascii, layers } => (ascii, layers),
        };
        let mut data = Cow::Borrowed(&bytes[start..end]);
        for filter in ascii {
            data = Cow::Owned(filter.decode(&data));
        }
        let image = is_image(dict);
        let cap = if image {
            caps.per_image
        } else {
            caps.per_stream
        };
        let size = inflated_size(&data, layers, cap.saturating_add(1));
        if size > cap {
            return Err(too_large(cap));
        }
        if !image {
            sum += size;
            if sum > caps.total {
                return Err(too_large(caps.total));
            }
        }
    }
    Ok(())
}

/// An ASCII filter we can undo on the way to the first `FlateDecode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AsciiFilter {
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

/// What to do with one stream, from its `/Filter` chain.
#[derive(Debug, PartialEq, Eq)]
enum Plan {
    /// No `FlateDecode` in the chain (or no readable chain).
    Skip,
    /// Undo `ascii`, then inflate `layers` times.
    Measure {
        ascii: Vec<AsciiFilter>,
        layers: usize,
    },
    /// Another filter comes before a `FlateDecode`.
    Refuse,
}

fn plan(dict: &[u8]) -> Plan {
    let Some(filters) = filter_names(dict) else {
        return Plan::Skip;
    };
    let is_flate = |f: &str| matches!(f, "FlateDecode" | "Fl");
    let Some(last_flate) = filters.iter().rposition(|f| is_flate(f)) else {
        return Plan::Skip;
    };
    let mut ascii = Vec::new();
    let mut layers = 0;
    for filter in &filters[..=last_flate] {
        match filter.as_str() {
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

/// The names of `/Filter` (a single `/Name` or an array of them), in order; `None` when the
/// dictionary has no filter we can read (e.g. an indirect reference).
fn filter_names(dict: &[u8]) -> Option<Vec<String>> {
    let at = find(dict, b"/Filter")?;
    let rest = dict[at + b"/Filter".len()..].trim_ascii_start();
    let name = |bytes: &[u8]| -> Option<(String, usize)> {
        let body = bytes.strip_prefix(b"/")?;
        let len = body
            .iter()
            .position(|b| b.is_ascii_whitespace() || b"()<>[]{}/%".contains(b))
            .unwrap_or(body.len());
        (len > 0).then(|| (String::from_utf8_lossy(&body[..len]).into_owned(), len + 1))
    };
    if let Some(mut list) = rest.strip_prefix(b"[") {
        let mut names = Vec::new();
        loop {
            list = list.trim_ascii_start();
            if list.starts_with(b"]") {
                return Some(names);
            }
            let (filter, used) = name(list)?;
            names.push(filter);
            list = &list[used..];
        }
    }
    name(rest).map(|(filter, _)| vec![filter])
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

/// The `<< … >>` dictionary that ends right before `keyword` (whitespace allowed), matching
/// nested `<< >>` pairs backwards.
fn dictionary_before(bytes: &[u8], keyword: usize) -> Option<&[u8]> {
    let mut end = keyword;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
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

/// `/Subtype /Image` (any whitespace between the two names).
fn is_image(dict: &[u8]) -> bool {
    let mut rest = dict;
    while let Some(at) = find(rest, b"/Subtype") {
        rest = &rest[at + b"/Subtype".len()..];
        let name = rest.trim_ascii_start();
        if name.starts_with(b"/Image") {
            return true;
        }
    }
    false
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

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    use super::*;

    const CAPS: Caps = Caps {
        per_stream: 1_000_000,
        per_image: 3_000_000,
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
    fn images_have_their_own_larger_cap_outside_the_total() {
        let scan = zlib(&vec![0u8; 2_000_000]);
        let image = "/Type /XObject /Subtype /Image /Filter /FlateDecode";
        let pages = pdf(&[
            object(image, &scan),
            object(image, &scan),
            object(image, &scan),
            object(image, &scan),
            object(image, &scan),
            object(image, &scan),
        ]);
        let tight_total = Caps {
            total: 1_000_000,
            ..CAPS
        };
        assert!(!refused(&pages, tight_total), "12 MB of images, total 1 MB");
        let bomb = zlib(&vec![0u8; 4_000_000]);
        assert!(refused(&pdf(&[object(image, &bomb)]), CAPS), "past 3 MB");
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
