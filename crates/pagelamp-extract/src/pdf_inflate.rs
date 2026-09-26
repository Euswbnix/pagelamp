//! Decompression-bomb check for PDFs, run before `lopdf` sees the file.
//!
//! `lopdf` inflates streams without any size limit (object streams while loading, page
//! contents, fonts and colour profiles while `pdf-extract` reads a page), so a few hundred KB
//! of PDF can grow to gigabytes of memory. Here every `FlateDecode` stream is inflated first
//! with a cap, counting bytes only (nothing is kept, memory stays at one 64 KB buffer), and
//! the file is refused when one stream or all of them together would grow too large.
//!
//! The scan works on the raw bytes: each `<< … >> stream EOL … endstream`. It is a
//! heuristic, deliberately lenient (anything it can't make sense of is skipped, never
//! refused). Known gaps, until extraction runs in a separate process:
//! - streams whose dictionary says `/Subtype /Image` are skipped: text extraction does not
//!   decode images, and scanned pages are legitimately large (a crafted file could label a
//!   page's content stream as an image);
//! - only `FlateDecode` is measured (not `LZWDecode`, whose worst ratio is far smaller);
//! - encrypted streams (ciphertext does not inflate) are not measured.

use std::io::Read;

use flate2::read::ZlibDecoder;

use crate::ExtractError;
use crate::util::{failed, size_text};

/// Longest stream dictionary we look back over (they are normally < 1 KB).
const MAX_DICT_BYTES: usize = 64 * 1024;
/// Most `FlateDecode` layers applied to one stream (a chain can multiply the ratio).
const MAX_LAYERS: usize = 4;

/// Refuse the PDF in `bytes` if one `FlateDecode` stream would inflate to more than
/// `per_stream` bytes, or all of them together to more than `total`.
pub(crate) fn check(bytes: &[u8], per_stream: u64, total: u64) -> Result<(), ExtractError> {
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
        let layers = count(dict, b"/FlateDecode").min(MAX_LAYERS);
        if layers == 0 || is_image(dict) {
            continue;
        }
        let size = inflated_size(&bytes[start..end], layers, per_stream.saturating_add(1));
        if size > per_stream {
            return Err(too_large(per_stream));
        }
        sum += size;
        if sum > total {
            return Err(too_large(total));
        }
    }
    Ok(())
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

    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
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

    #[test]
    fn a_stream_that_inflates_past_the_cap_is_refused() {
        let bomb = zlib(&vec![b' '; 2_000_000]);
        assert!(bomb.len() < 10_000, "a real bomb: {} bytes", bomb.len());
        let file = pdf(&[object("/Filter /FlateDecode", &bomb)]);
        let err = check(&file, 1_000_000, 10_000_000).unwrap_err();
        assert!(matches!(err, ExtractError::Failed(m) if m.contains("would expand")));
        assert!(check(&file, 3_000_000, 10_000_000).is_ok());
    }

    #[test]
    fn the_total_of_all_streams_is_capped_too() {
        let part = zlib(&vec![b'q'; 600_000]);
        let file = pdf(&[
            object("/Filter [/FlateDecode]", &part),
            object("/Filter/FlateDecode /DecodeParms << /Predictor 1 >>", &part),
        ]);
        assert!(check(&file, 1_000_000, 1_500_000).is_ok());
        assert!(check(&file, 1_000_000, 1_000_000).is_err());
    }

    #[test]
    fn chained_flate_layers_are_all_counted() {
        let twice = zlib(&zlib(&vec![b' '; 2_000_000]));
        let file = pdf(&[object("/Filter [/FlateDecode /FlateDecode]", &twice)]);
        assert!(check(&file, 1_000_000, 10_000_000).is_err());
    }

    #[test]
    fn images_other_filters_and_garbage_are_not_refused() {
        let big = zlib(&vec![0u8; 2_000_000]);
        let file = pdf(&[
            object("/Type /XObject /Subtype /Image /Filter /FlateDecode", &big),
            object("/Filter /DCTDecode", &[0xFF, 0xD8, 0xFF]),
            object("/Filter /FlateDecode", b"not zlib at all"),
            b"4 0 obj << /Name /Downstream >> endobj\n".to_vec(),
            // Truncated: no endstream.
            b"5 0 obj << /Filter /FlateDecode >> stream\n\x78".to_vec(),
        ]);
        assert!(check(&file, 1_000_000, 1_000_000).is_ok());
        assert!(check(b"", 1, 1).is_ok());
    }
}
