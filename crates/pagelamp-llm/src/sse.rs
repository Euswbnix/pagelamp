//! Stream framing: server-sent events (the WHATWG event-stream format) and newline-delimited
//! JSON (Ollama). Fed with whatever byte chunks the connection delivers; lines may be split
//! anywhere, including between `\r` and `\n` and inside a UTF-8 character.

/// Longest line or event accepted (a stream past this is broken or hostile).
const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Framing {
    Sse,
    // Ollama's native stream; its driver lands next in M1.
    #[allow(dead_code)]
    Ndjson,
}

/// One event: an SSE event (`event` name, joined `data` lines) or one NDJSON line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawEvent {
    pub event: Option<String>,
    pub data: String,
}

/// The stream broke the framing (a line or event past `MAX_LINE_BYTES`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TooLong;

pub(crate) struct EventParser {
    framing: Framing,
    /// Bytes of the current, unfinished line.
    line: Vec<u8>,
    /// The previous chunk ended with `\r`: a `\n` starting the next one belongs to it.
    after_cr: bool,
    /// Still at the start of the stream (a UTF-8 BOM is dropped there).
    at_start: bool,
    event: Option<String>,
    data: String,
    has_data: bool,
}

impl EventParser {
    pub(crate) fn new(framing: Framing) -> EventParser {
        EventParser {
            framing,
            line: Vec::new(),
            after_cr: false,
            at_start: true,
            event: None,
            data: String::new(),
            has_data: false,
        }
    }

    /// The events completed by `bytes`.
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Result<Vec<RawEvent>, TooLong> {
        let mut events = Vec::new();
        let mut bytes = bytes;
        if self.at_start && !bytes.is_empty() {
            self.line.extend_from_slice(bytes);
            if self.line.len() < 3 && b"\xEF\xBB\xBF".starts_with(&self.line) {
                return Ok(events); // maybe a BOM, split across chunks
            }
            self.at_start = false;
            let pending = std::mem::take(&mut self.line);
            let pending = pending
                .strip_prefix(b"\xEF\xBB\xBF")
                .unwrap_or(&pending)
                .to_vec();
            return self.feed_lines(&pending, events);
        }
        if self.after_cr && bytes.first() == Some(&b'\n') {
            bytes = &bytes[1..];
        }
        self.after_cr = false;
        let owned = bytes.to_vec();
        events = self.feed_lines(&owned, events)?;
        Ok(events)
    }

    fn feed_lines(
        &mut self,
        bytes: &[u8],
        mut events: Vec<RawEvent>,
    ) -> Result<Vec<RawEvent>, TooLong> {
        let mut start = 0;
        let mut index = 0;
        while index < bytes.len() {
            let byte = bytes[index];
            if byte == b'\n' || byte == b'\r' {
                self.line.extend_from_slice(&bytes[start..index]);
                let line = std::mem::take(&mut self.line);
                self.on_line(&line, &mut events)?;
                if byte == b'\r' {
                    if index + 1 == bytes.len() {
                        self.after_cr = true;
                    } else if bytes[index + 1] == b'\n' {
                        index += 1;
                    }
                }
                start = index + 1;
            }
            index += 1;
        }
        self.line.extend_from_slice(&bytes[start..]);
        if self.line.len() > MAX_LINE_BYTES {
            return Err(TooLong);
        }
        Ok(events)
    }

    /// The stream ended: what the last line completes. (An SSE event without its closing
    /// blank line is dropped, as the event-stream spec says: the connection broke mid-event.)
    pub(crate) fn finish(&mut self) -> Result<Vec<RawEvent>, TooLong> {
        let mut events = Vec::new();
        if !self.line.is_empty() {
            let line = std::mem::take(&mut self.line);
            self.on_line(&line, &mut events)?;
        }
        Ok(events)
    }

    fn on_line(&mut self, line: &[u8], events: &mut Vec<RawEvent>) -> Result<(), TooLong> {
        let line = String::from_utf8_lossy(line);
        match self.framing {
            Framing::Ndjson => {
                if !line.trim().is_empty() {
                    events.push(RawEvent {
                        event: None,
                        data: line.into_owned(),
                    });
                }
            }
            Framing::Sse => {
                if line.is_empty() {
                    if self.has_data {
                        events.push(RawEvent {
                            event: self.event.take(),
                            data: std::mem::take(&mut self.data),
                        });
                    }
                    self.event = None;
                    self.data.clear();
                    self.has_data = false;
                    return Ok(());
                }
                if line.starts_with(':') {
                    return Ok(()); // a comment (keep-alive)
                }
                let (field, value) = match line.split_once(':') {
                    Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
                    None => (line.as_ref(), ""),
                };
                match field {
                    "event" => self.event = Some(value.to_string()),
                    "data" => {
                        if self.has_data {
                            self.data.push('\n');
                        }
                        self.data.push_str(value);
                        self.has_data = true;
                        if self.data.len() > MAX_LINE_BYTES {
                            return Err(TooLong);
                        }
                    }
                    _ => {} // id, retry and unknown fields don't matter here
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sse(event: Option<&str>, data: &str) -> RawEvent {
        RawEvent {
            event: event.map(str::to_string),
            data: data.to_string(),
        }
    }

    /// Every event of `stream` fed in pieces of `size` bytes.
    fn parse_in_pieces(framing: Framing, stream: &[u8], size: usize) -> Vec<RawEvent> {
        let mut parser = EventParser::new(framing);
        let mut events = Vec::new();
        for piece in stream.chunks(size) {
            events.extend(parser.feed(piece).unwrap());
        }
        events.extend(parser.finish().unwrap());
        events
    }

    #[test]
    fn events_survive_any_chunking_and_line_ending() {
        let stream = "\u{feff}: keep-alive\r\n\
                      event: message_start\r\ndata: {\"a\":1}\r\n\r\n\
                      event: delta\ndata: first line\ndata: second é line\n\n\
                      data:no space\rid: 7\rretry: 10\r\r\
                      event: ignored-without-data\n\n\
                      event: tail\ndata: dropped at eof without blank line\n";
        let expected = vec![
            sse(Some("message_start"), "{\"a\":1}"),
            sse(Some("delta"), "first line\nsecond é line"),
            sse(None, "no space"),
        ];
        for size in 1..=stream.len() {
            assert_eq!(
                parse_in_pieces(Framing::Sse, stream.as_bytes(), size),
                expected,
                "pieces of {size}"
            );
        }
    }

    #[test]
    fn ndjson_gives_one_event_per_nonblank_line() {
        let stream = "{\"a\":1}\n\n{\"b\":2}\r\n{\"c\":3}";
        for size in 1..=stream.len() {
            let events = parse_in_pieces(Framing::Ndjson, stream.as_bytes(), size);
            let data: Vec<&str> = events.iter().map(|e| e.data.as_str()).collect();
            assert_eq!(
                data,
                ["{\"a\":1}", "{\"b\":2}", "{\"c\":3}"],
                "pieces of {size}"
            );
        }
    }

    #[test]
    fn an_endless_line_is_refused() {
        let mut parser = EventParser::new(Framing::Sse);
        let chunk = vec![b'x'; 1024 * 1024];
        let mut result = Ok(Vec::new());
        for _ in 0..=16 {
            result = parser.feed(&chunk);
            if result.is_err() {
                break;
            }
        }
        assert_eq!(result, Err(TooLong));
    }
}
