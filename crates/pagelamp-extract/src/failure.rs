//! What an `ExtractError::Failed` message says went wrong, for apps that show a short reason
//! instead of the message. The messages are this crate's own, so the rules live next to
//! them: every format module's failure tests also check the kind here.
//!
//! The kind is read from the stored message rather than stored itself, so it needs no
//! schema or worker-protocol change and applies to failures recorded by earlier versions.

/// Why a file's text could not be read, when the reason is in the file itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// Over a size, page or part limit (including zip-bomb and inflation checks).
    TooLarge,
    /// A PDF that needs a password to open.
    PasswordProtected,
    /// Damaged, not the format its name says, or something the parser cannot handle.
    Malformed,
}

/// Message starts that mean the file was over a limit.
const TOO_LARGE: [&str; 6] = [
    "file is too large to index",
    "PDF has more than ",
    "PDF would expand to more than ",
    "PDF encodes its content in a way whose size can't be checked",
    "document has too many internal parts",
    "document is too large when uncompressed",
];

/// `pdf::load`'s whole message.
pub(crate) const PASSWORD_PROTECTED: &str = "PDF is password-protected";

/// Message starts that mean the file could not be parsed.
const MALFORMED: [&str; 9] = [
    "cannot read ",
    "cannot decompress ",
    "PDF page ",
    "not a readable Office file",
    "not a Word document",
    "malformed XML in ",
    "notebook is not valid JSON",
    "notebook has no \"cells\" list",
    "file looks binary",
];

/// The kind of a `Failed` message from this crate; `None` for text it did not write.
pub fn failure_kind(message: &str) -> Option<FailureKind> {
    if TOO_LARGE.iter().any(|start| message.starts_with(start)) {
        return Some(FailureKind::TooLarge);
    }
    if message == PASSWORD_PROTECTED {
        return Some(FailureKind::PasswordProtected);
    }
    // `util::catch_panic` writes "<format> parser crashed: …".
    let crashed = message
        .split_once(" parser crashed: ")
        .is_some_and(|(format, _)| !format.is_empty() && !format.contains(' '));
    let unparsable = MALFORMED.iter().any(|start| message.starts_with(start));
    (crashed || unparsable).then_some(FailureKind::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_message_shape_has_its_kind() {
        for (message, kind) in [
            (
                "file is too large to index (250 MB; the limit is 200 MB)",
                Some(FailureKind::TooLarge),
            ),
            (
                "PDF has more than 5,000 pages (it has 5,001), so it was not indexed",
                Some(FailureKind::TooLarge),
            ),
            (
                "PDF would expand to more than 64 MB when opened, so it was not indexed",
                Some(FailureKind::TooLarge),
            ),
            (
                "PDF encodes its content in a way whose size can't be checked, so it was not indexed",
                Some(FailureKind::TooLarge),
            ),
            (
                "document has too many internal parts (12,000; the limit is 10,000), so it was not indexed",
                Some(FailureKind::TooLarge),
            ),
            (
                "document is too large when uncompressed (ppt/slides/slide1.xml exceeds the 100 MB limit), so it was not indexed",
                Some(FailureKind::TooLarge),
            ),
            (
                "PDF is password-protected",
                Some(FailureKind::PasswordProtected),
            ),
            (
                "cannot read PDF: invalid file header",
                Some(FailureKind::Malformed),
            ),
            (
                "cannot read PDF page 3: bad font",
                Some(FailureKind::Malformed),
            ),
            (
                "PDF page 2 was skipped: nested too deeply",
                Some(FailureKind::Malformed),
            ),
            (
                "not a readable Office file (damaged, password-protected or an old binary format): invalid Zip archive",
                Some(FailureKind::Malformed),
            ),
            (
                "not a Word document: word/document.xml is missing",
                Some(FailureKind::Malformed),
            ),
            (
                "malformed XML in x.xml at byte 4: bad",
                Some(FailureKind::Malformed),
            ),
            (
                "cannot decompress word/document.xml: bad",
                Some(FailureKind::Malformed),
            ),
            (
                "notebook is not valid JSON: EOF",
                Some(FailureKind::Malformed),
            ),
            (
                "notebook has no \"cells\" list (only nbformat 4 is supported)",
                Some(FailureKind::Malformed),
            ),
            (
                "file looks binary (not text), so it was not indexed",
                Some(FailureKind::Malformed),
            ),
            (
                "PDF parser crashed: index out of bounds",
                Some(FailureKind::Malformed),
            ),
            // Not this crate's words.
            ("the disk is full", None),
            ("the disk parser crashed: somewhere", None),
            ("", None),
        ] {
            assert_eq!(failure_kind(message), kind, "{message}");
        }
    }
}
