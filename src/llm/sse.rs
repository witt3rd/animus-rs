//! Server-Sent Events parser for LLM streaming responses.

/// A parsed SSE event.
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event_type: Option<String>,
    pub data: String,
}

/// Incremental SSE parser.
///
/// Buffers incoming bytes and emits complete events as they arrive.
/// An event is considered complete when an empty line (`\n\n`) is encountered.
pub struct SseParser {
    buffer: String,
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Feed a chunk of SSE text. Returns any complete events found.
    ///
    /// Partial data is buffered until a blank line terminates an event block.
    pub fn feed(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.buffer.push_str(chunk);
        let mut events = Vec::new();

        // Split on double-newline boundaries (event terminators).
        while let Some(pos) = self.buffer.find("\n\n") {
            let block = self.buffer[..pos].to_string();
            self.buffer = self.buffer[pos + 2..].to_string();

            // Skip empty blocks (extra blank lines between events).
            if block.trim().is_empty() {
                continue;
            }

            let mut event_type: Option<String> = None;
            let mut data_lines: Vec<String> = Vec::new();

            for line in block.split('\n') {
                // Skip comment lines (start with ':').
                if line.starts_with(':') {
                    continue;
                }

                if let Some(value) = line.strip_prefix("event: ") {
                    event_type = Some(value.to_string());
                } else if let Some(value) = line.strip_prefix("event:") {
                    event_type = Some(value.to_string());
                } else if let Some(value) = line.strip_prefix("data: ") {
                    data_lines.push(value.to_string());
                } else if let Some(value) = line.strip_prefix("data:") {
                    data_lines.push(value.to_string());
                }
                // Ignore other field names (id:, retry:, etc.)
            }

            if !data_lines.is_empty() {
                events.push(SseEvent {
                    event_type,
                    data: data_lines.join("\n"),
                });
            }
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_event() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: {\"hello\":\"world\"}\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "{\"hello\":\"world\"}");
        assert!(events[0].event_type.is_none());
    }

    #[test]
    fn parse_event_with_type() {
        let mut parser = SseParser::new();
        let events = parser.feed("event: content_block_delta\ndata: {\"text\":\"hi\"}\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type.as_deref(), Some("content_block_delta"));
        assert_eq!(events[0].data, "{\"text\":\"hi\"}");
    }

    #[test]
    fn parse_done_event() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: [DONE]\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "[DONE]");
    }

    #[test]
    fn parse_multiple_events() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: first\n\ndata: second\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "first");
        assert_eq!(events[1].data, "second");
    }

    #[test]
    fn parse_partial_then_complete() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: hel");
        assert!(events.is_empty());
        let events = parser.feed("lo\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn skip_comments() {
        let mut parser = SseParser::new();
        let events = parser.feed(": this is a comment\ndata: real\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "real");
    }

    #[test]
    fn skip_empty_lines_between_events() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: one\n\n\n\ndata: two\n\n");
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn multiline_data() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: line1\ndata: line2\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2");
    }
}
