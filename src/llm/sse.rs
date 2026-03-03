//! Server-Sent Events parser for LLM streaming responses.

/// A parsed SSE event.
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event_type: Option<String>,
    pub data: String,
}

/// Incremental SSE parser.
pub struct SseParser {
    _buffer: String,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            _buffer: String::new(),
        }
    }

    pub fn feed(&mut self, _chunk: &str) -> Vec<SseEvent> {
        // Stub: returns empty vec so tests compile but fail assertions
        Vec::new()
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
