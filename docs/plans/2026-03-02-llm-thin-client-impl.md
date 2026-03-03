# LLM Thin Client Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace rig-core LLM calls with a thin reqwest-based OpenAI Chat Completions client supporting multiple providers via base URL routing.

**Architecture:** Single `LlmClient` trait with one concrete implementation (`OpenAiClient`) that speaks the OpenAI Chat Completions wire format. Factory function maps provider names (openai, xai, openrouter, groq, ollama, deepseek) to the client with appropriate base URLs. Our canonical types (`Message`, `ContentBlock`, etc.) are mapped to/from OpenAI JSON at the boundary.

**Tech Stack:** reqwest (HTTP + streaming), serde/serde_json (serialization), tokio (async runtime + mpsc channels), thiserror (error types), secrecy (API keys), async-trait (trait object dispatch)

**Design Doc:** `docs/plans/2026-03-02-llm-thin-client-design.md`

---

### Task 1: Add Dependencies to Cargo.toml

**Files:**
- Modify: `Cargo.toml:41-43` (promote reqwest from dev-dependencies, add async-trait + futures-util)

**Step 1: Update Cargo.toml**

Move `reqwest` from `[dev-dependencies]` to `[dependencies]` with stream feature. Add `async-trait` and `futures-util`.

In `[dependencies]`, after the `rig-postgres` line, add:

```toml
# LLM — thin provider clients
reqwest = { version = "0.12", features = ["json", "stream"] }
async-trait = "0.1"
futures-util = "0.3"
```

In `[dev-dependencies]`, remove the `reqwest` line (it's now a regular dependency).

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles with no errors

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: promote reqwest, add async-trait + futures-util for LLM client"
```

---

### Task 2: Create LLM Error Type

**Files:**
- Create: `src/llm/error.rs`
- Test: unit tests inline (`#[cfg(test)]`)

**Step 1: Write the failing test**

Create `src/llm/error.rs` with only tests first:

```rust
//! LLM-specific error types.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limited_display_with_retry() {
        let err = LlmError::RateLimited {
            retry_after_secs: Some(30),
        };
        assert_eq!(err.to_string(), "Rate limited (retry after Some(30)s)");
    }

    #[test]
    fn rate_limited_display_without_retry() {
        let err = LlmError::RateLimited {
            retry_after_secs: None,
        };
        assert_eq!(err.to_string(), "Rate limited (retry after Nones)");
    }

    #[test]
    fn api_error_display() {
        let err = LlmError::Api {
            status: 400,
            message: "invalid request".to_string(),
        };
        assert_eq!(err.to_string(), "API error (400): invalid request");
    }

    #[test]
    fn unsupported_provider_display() {
        let err = LlmError::UnsupportedProvider("mystery".to_string());
        assert_eq!(err.to_string(), "Unsupported provider: mystery");
    }

    #[test]
    fn from_serde_json_error() {
        let json_err = serde_json::from_str::<String>("not json").unwrap_err();
        let err: LlmError = json_err.into();
        assert!(matches!(err, LlmError::Json(_)));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib llm::error`
Expected: FAIL — `LlmError` not defined

**Step 3: Write the implementation above the tests**

```rust
//! LLM-specific error types.

use thiserror::Error;

/// Errors from LLM provider calls.
#[derive(Debug, Error)]
pub enum LlmError {
    /// HTTP transport failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// LLM API returned an error response.
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    /// Rate limited (429). Includes retry-after hint if provided.
    #[error("Rate limited (retry after {retry_after_secs:?}s)")]
    RateLimited { retry_after_secs: Option<u64> },

    /// JSON serialization/deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// SSE stream parsing error.
    #[error("Stream error: {0}")]
    Stream(String),

    /// Unknown provider name in config.
    #[error("Unsupported provider: {0}")]
    UnsupportedProvider(String),
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib llm::error`
Expected: PASS (all 5 tests)

**Step 5: Commit**

```bash
git add src/llm/error.rs
git commit -m "feat(llm): add LlmError type with provider-specific variants"
```

---

### Task 3: Create LLM Types

**Files:**
- Create: `src/llm/types.rs`
- Test: unit tests inline

**Step 1: Write the failing test**

Create `src/llm/types.rs` with tests:

```rust
//! Canonical LLM types — provider-independent.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_request_defaults() {
        let req = CompletionRequest {
            model: "gpt-4o".into(),
            system: "You are helpful.".into(),
            messages: vec![],
            tools: vec![],
            max_tokens: 8192,
            temperature: None,
        };
        assert_eq!(req.model, "gpt-4o");
        assert!(req.temperature.is_none());
    }

    #[test]
    fn stop_reason_variants() {
        assert!(matches!(StopReason::EndTurn, StopReason::EndTurn));
        assert!(matches!(StopReason::ToolUse, StopReason::ToolUse));
        assert!(matches!(StopReason::MaxTokens, StopReason::MaxTokens));
        assert!(matches!(
            StopReason::Other("custom".into()),
            StopReason::Other(_)
        ));
    }

    #[test]
    fn content_block_text() {
        let block = ContentBlock::Text {
            text: "hello".into(),
        };
        if let ContentBlock::Text { text } = &block {
            assert_eq!(text, "hello");
        } else {
            panic!("expected Text variant");
        }
    }

    #[test]
    fn content_block_tool_use() {
        let block = ContentBlock::ToolUse {
            id: "call_123".into(),
            name: "search".into(),
            input: serde_json::json!({"query": "rust"}),
        };
        if let ContentBlock::ToolUse { id, name, input } = &block {
            assert_eq!(id, "call_123");
            assert_eq!(name, "search");
            assert_eq!(input["query"], "rust");
        } else {
            panic!("expected ToolUse variant");
        }
    }

    #[test]
    fn message_variants() {
        let sys = Message::System {
            content: "system".into(),
        };
        assert!(matches!(sys, Message::System { .. }));

        let user = Message::User {
            content: vec![UserContent::Text {
                text: "hi".into(),
            }],
        };
        assert!(matches!(user, Message::User { .. }));

        let asst = Message::Assistant {
            content: vec![ContentBlock::Text {
                text: "hello".into(),
            }],
        };
        assert!(matches!(asst, Message::Assistant { .. }));
    }

    #[test]
    fn user_content_tool_result() {
        let result = UserContent::ToolResult {
            tool_use_id: "call_123".into(),
            content: "42".into(),
            is_error: false,
        };
        if let UserContent::ToolResult {
            tool_use_id,
            is_error,
            ..
        } = &result
        {
            assert_eq!(tool_use_id, "call_123");
            assert!(!is_error);
        } else {
            panic!("expected ToolResult variant");
        }
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib llm::types`
Expected: FAIL — types not defined

**Step 3: Write the implementation above the tests**

```rust
//! Canonical LLM types — provider-independent.
//!
//! These types are our own. They map cleanly to the OpenAI Chat Completions API
//! without being tied to it. The act loop works with these types — never with
//! provider-specific wire formats.

use serde::{Deserialize, Serialize};

/// A completion request sent to the LLM.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Model identifier (e.g., "gpt-4o", "grok-3-latest").
    pub model: String,
    /// System prompt.
    pub system: String,
    /// Conversation messages.
    pub messages: Vec<Message>,
    /// Tool definitions available for this call.
    pub tools: Vec<ToolDefinition>,
    /// Maximum tokens to generate.
    pub max_tokens: u32,
    /// Sampling temperature. None = provider default.
    pub temperature: Option<f64>,
}

/// The LLM's response.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    /// Content blocks returned by the model.
    pub content: Vec<ContentBlock>,
    /// Why the model stopped generating.
    pub stop_reason: StopReason,
    /// Token usage for this call.
    pub usage: Usage,
}

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    /// System message.
    System { content: String },
    /// User message — text, tool results, or a mix.
    User { content: Vec<UserContent> },
    /// Assistant message — text, tool calls, or a mix.
    Assistant { content: Vec<ContentBlock> },
}

/// Content in a user message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserContent {
    /// Plain text.
    Text { text: String },
    /// Result of a tool call.
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
    /// Image data (base64).
    Image { media_type: String, data: String },
}

/// Content block in an assistant response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text output.
    Text { text: String },
    /// Tool call request.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// Why the model stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// Normal completion.
    EndTurn,
    /// The model wants to call tools.
    ToolUse,
    /// Hit the max_tokens limit.
    MaxTokens,
    /// Unknown or provider-specific reason.
    Other(String),
}

/// Token usage for a single LLM call.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// A tool definition sent to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Events emitted during streaming.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A chunk of text output.
    TextDelta { text: String },
    /// Partial JSON for a tool call's input.
    ToolInputDelta { tool_use_id: String, json: String },
    /// A tool call is starting.
    ToolStart { tool_use_id: String, name: String },
    /// Stream complete.
    Done,
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib llm::types`
Expected: PASS (all 6 tests)

**Step 5: Commit**

```bash
git add src/llm/types.rs
git commit -m "feat(llm): add canonical LLM types (Message, ContentBlock, etc.)"
```

---

### Task 4: Create SSE Stream Parser

**Files:**
- Create: `src/llm/sse.rs`
- Test: unit tests inline

**Step 1: Write the failing test**

Create `src/llm/sse.rs` with tests:

```rust
//! Server-Sent Events parser for LLM streaming responses.

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
        assert_eq!(
            events[0].event_type.as_deref(),
            Some("content_block_delta")
        );
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

        // First chunk: incomplete event
        let events = parser.feed("data: hel");
        assert!(events.is_empty());

        // Second chunk: completes the event
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
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib llm::sse`
Expected: FAIL — `SseParser` not defined

**Step 3: Write the implementation above the tests**

```rust
//! Server-Sent Events parser for LLM streaming responses.
//!
//! Both OpenAI and Anthropic use SSE for streaming. This parser handles the
//! common framing (event type, data lines, blank-line delimiters). Each
//! provider interprets the parsed events differently.

/// A parsed SSE event.
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// Optional event type (from `event:` line).
    pub event_type: Option<String>,
    /// The data payload (from `data:` lines, joined with newlines).
    pub data: String,
}

/// Incremental SSE parser. Feed it chunks of bytes as they arrive from the
/// HTTP response; it returns any complete events.
pub struct SseParser {
    buffer: String,
    current_event_type: Option<String>,
    current_data: Vec<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            current_event_type: None,
            current_data: Vec::new(),
        }
    }

    /// Feed a chunk of text from the HTTP response body.
    /// Returns any complete SSE events found in this chunk.
    pub fn feed(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.buffer.push_str(chunk);
        let mut events = Vec::new();

        // Process complete lines (terminated by \n)
        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos].trim_end_matches('\r').to_string();
            self.buffer = self.buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                // Blank line = event boundary
                if !self.current_data.is_empty() {
                    events.push(SseEvent {
                        event_type: self.current_event_type.take(),
                        data: self.current_data.join("\n"),
                    });
                    self.current_data.clear();
                }
            } else if let Some(comment) = line.strip_prefix(':') {
                // Comment line — ignore
                let _ = comment;
            } else if let Some(event_type) = line.strip_prefix("event: ") {
                self.current_event_type = Some(event_type.to_string());
            } else if let Some(event_type) = line.strip_prefix("event:") {
                self.current_event_type = Some(event_type.to_string());
            } else if let Some(data) = line.strip_prefix("data: ") {
                self.current_data.push(data.to_string());
            } else if let Some(data) = line.strip_prefix("data:") {
                self.current_data.push(data.to_string());
            }
            // Other lines (id:, retry:, unknown) are ignored per SSE spec
        }

        events
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib llm::sse`
Expected: PASS (all 8 tests)

**Step 5: Commit**

```bash
git add src/llm/sse.rs
git commit -m "feat(llm): add SSE stream parser for LLM streaming responses"
```

---

### Task 5: Create OpenAI Client — Non-Streaming

**Files:**
- Create: `src/llm/openai.rs`
- Test: unit tests inline (JSON serialization roundtrips, no live HTTP)

This task builds the request serialization, response parsing, and non-streaming `complete()`. Streaming is Task 6.

**Step 1: Write the failing tests**

Create `src/llm/openai.rs` with tests that verify JSON serialization/deserialization:

```rust
//! OpenAI Chat Completions API client.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::*;

    #[test]
    fn serialize_text_only_request() {
        let req = CompletionRequest {
            model: "gpt-4o".into(),
            system: "You are helpful.".into(),
            messages: vec![Message::User {
                content: vec![UserContent::Text {
                    text: "Hello".into(),
                }],
            }],
            tools: vec![],
            max_tokens: 1024,
            temperature: Some(0.7),
        };
        let body = build_request_body(&req, false);
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["temperature"], 0.7);
        assert!(!body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false));

        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2); // system + user
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "You are helpful.");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "Hello");
    }

    #[test]
    fn serialize_request_with_tools() {
        let req = CompletionRequest {
            model: "gpt-4o".into(),
            system: "sys".into(),
            messages: vec![],
            tools: vec![ToolDefinition {
                name: "search".into(),
                description: "Search the web".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"]
                }),
            }],
            max_tokens: 1024,
            temperature: None,
        };
        let body = build_request_body(&req, false);
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "search");
        assert_eq!(tools[0]["function"]["description"], "Search the web");
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn serialize_assistant_with_tool_calls() {
        let req = CompletionRequest {
            model: "gpt-4o".into(),
            system: "sys".into(),
            messages: vec![
                Message::Assistant {
                    content: vec![
                        ContentBlock::Text {
                            text: "Let me search.".into(),
                        },
                        ContentBlock::ToolUse {
                            id: "call_abc".into(),
                            name: "search".into(),
                            input: serde_json::json!({"query": "rust"}),
                        },
                    ],
                },
                Message::User {
                    content: vec![UserContent::ToolResult {
                        tool_use_id: "call_abc".into(),
                        content: "found: rust lang".into(),
                        is_error: false,
                    }],
                },
            ],
            tools: vec![],
            max_tokens: 1024,
            temperature: None,
        };
        let body = build_request_body(&req, false);
        let messages = body["messages"].as_array().unwrap();

        // system + assistant + tool result
        assert_eq!(messages.len(), 3);

        // Assistant message with tool_calls
        let asst = &messages[1];
        assert_eq!(asst["role"], "assistant");
        assert_eq!(asst["content"], "Let me search.");
        let tool_calls = asst["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "call_abc");
        assert_eq!(tool_calls[0]["type"], "function");
        assert_eq!(tool_calls[0]["function"]["name"], "search");

        // Tool result message
        let tool_msg = &messages[2];
        assert_eq!(tool_msg["role"], "tool");
        assert_eq!(tool_msg["tool_call_id"], "call_abc");
        assert_eq!(tool_msg["content"], "found: rust lang");
    }

    #[test]
    fn parse_text_response() {
        let json = serde_json::json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 8,
                "total_tokens": 18
            }
        });
        let resp = parse_response_body(json).unwrap();
        assert_eq!(resp.content.len(), 1);
        assert!(matches!(&resp.content[0], ContentBlock::Text { text } if text == "Hello! How can I help?"));
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 8);
    }

    #[test]
    fn parse_tool_call_response() {
        let json = serde_json::json!({
            "id": "chatcmpl-456",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_xyz",
                        "type": "function",
                        "function": {
                            "name": "search",
                            "arguments": "{\"query\":\"rust\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 20,
                "completion_tokens": 15,
                "total_tokens": 35
            }
        });
        let resp = parse_response_body(json).unwrap();
        assert_eq!(resp.content.len(), 1);
        if let ContentBlock::ToolUse { id, name, input } = &resp.content[0] {
            assert_eq!(id, "call_xyz");
            assert_eq!(name, "search");
            assert_eq!(input["query"], "rust");
        } else {
            panic!("expected ToolUse");
        }
        assert_eq!(resp.stop_reason, StopReason::ToolUse);
    }

    #[test]
    fn parse_mixed_text_and_tool_calls() {
        let json = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "I'll search for that.",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "search",
                            "arguments": "{\"q\":\"test\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 10, "total_tokens": 15 }
        });
        let resp = parse_response_body(json).unwrap();
        assert_eq!(resp.content.len(), 2); // text + tool_use
        assert!(matches!(&resp.content[0], ContentBlock::Text { .. }));
        assert!(matches!(&resp.content[1], ContentBlock::ToolUse { .. }));
    }

    #[test]
    fn parse_max_tokens_stop_reason() {
        let json = serde_json::json!({
            "choices": [{
                "message": { "role": "assistant", "content": "partial..." },
                "finish_reason": "length"
            }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 100, "total_tokens": 105 }
        });
        let resp = parse_response_body(json).unwrap();
        assert_eq!(resp.stop_reason, StopReason::MaxTokens);
    }

    #[test]
    fn streaming_body_sets_stream_true() {
        let req = CompletionRequest {
            model: "gpt-4o".into(),
            system: "sys".into(),
            messages: vec![],
            tools: vec![],
            max_tokens: 1024,
            temperature: None,
        };
        let body = build_request_body(&req, true);
        assert_eq!(body["stream"], true);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib llm::openai`
Expected: FAIL — `build_request_body` and `parse_response_body` not defined

**Step 3: Write the implementation**

Above the tests in `src/llm/openai.rs`:

```rust
//! OpenAI Chat Completions API client.
//!
//! Implements `LlmClient` for any provider that speaks the OpenAI Chat
//! Completions wire format: OpenAI, xAI (Grok), OpenRouter, Groq, Ollama,
//! DeepSeek, and others.

use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::mpsc::UnboundedSender;

use super::error::LlmError;
use super::sse::SseParser;
use super::types::*;

/// OpenAI-compatible Chat Completions client.
pub struct OpenAiClient {
    http: reqwest::Client,
    api_key: SecretString,
    base_url: String,
    max_retries: u32,
}

impl OpenAiClient {
    pub fn new(api_key: SecretString, base_url: String, max_retries: u32) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
            base_url,
            max_retries,
        }
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

#[async_trait]
impl super::LlmClient for OpenAiClient {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let body = build_request_body(request, false);
        let mut retries = 0;

        loop {
            let resp = self
                .http
                .post(self.chat_url())
                .header("Authorization", format!("Bearer {}", self.api_key.expose_secret()))
                .json(&body)
                .send()
                .await?;

            match resp.status().as_u16() {
                200 => {
                    let json: serde_json::Value = resp.json().await?;
                    return parse_response_body(json);
                }
                429 if retries < self.max_retries => {
                    let retry_after = resp
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok());
                    let backoff = retry_after.unwrap_or(2u64.pow(retries));
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                    retries += 1;
                }
                429 => {
                    return Err(LlmError::RateLimited {
                        retry_after_secs: None,
                    });
                }
                status => {
                    let message = resp.text().await.unwrap_or_default();
                    return Err(LlmError::Api { status, message });
                }
            }
        }
    }

    async fn complete_stream(
        &self,
        request: &CompletionRequest,
        tx: &UnboundedSender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError> {
        let body = build_request_body(request, true);

        let resp = self
            .http
            .post(self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key.expose_secret()))
            .json(&body)
            .send()
            .await?;

        if resp.status().as_u16() != 200 {
            let status = resp.status().as_u16();
            if status == 429 {
                return Err(LlmError::RateLimited {
                    retry_after_secs: None,
                });
            }
            let message = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api { status, message });
        }

        let mut parser = SseParser::new();
        let mut stream = resp.bytes_stream();

        let mut content_blocks: Vec<ContentBlock> = Vec::new();
        let mut current_tool_id: Option<String> = None;
        let mut current_tool_name: Option<String> = None;
        let mut current_tool_args = String::new();
        let mut text_buffer = String::new();
        let mut stop_reason = StopReason::EndTurn;
        let mut usage = Usage::default();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(LlmError::Http)?;
            let chunk_str =
                std::str::from_utf8(&chunk).map_err(|e| LlmError::Stream(e.to_string()))?;

            for event in parser.feed(chunk_str) {
                if event.data == "[DONE]" {
                    // Flush any pending tool call
                    if let Some(id) = current_tool_id.take() {
                        let args: serde_json::Value = serde_json::from_str(&current_tool_args)
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                        content_blocks.push(ContentBlock::ToolUse {
                            id,
                            name: current_tool_name.take().unwrap_or_default(),
                            input: args,
                        });
                        current_tool_args.clear();
                    }
                    // Flush text buffer
                    if !text_buffer.is_empty() {
                        content_blocks.push(ContentBlock::Text {
                            text: std::mem::take(&mut text_buffer),
                        });
                    }
                    let _ = tx.send(StreamEvent::Done);
                    break;
                }

                let data: serde_json::Value = serde_json::from_str(&event.data)
                    .map_err(|e| LlmError::Stream(format!("invalid SSE JSON: {e}")))?;

                // Extract finish_reason
                if let Some(reason) = data
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("finish_reason"))
                    .and_then(|r| r.as_str())
                {
                    stop_reason = parse_finish_reason(reason);
                }

                // Extract usage if present (some providers send it in the final chunk)
                if let Some(u) = data.get("usage") {
                    usage = Usage {
                        input_tokens: u
                            .get("prompt_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32,
                        output_tokens: u
                            .get("completion_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32,
                    };
                }

                let delta = data
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("delta"));

                if let Some(delta) = delta {
                    // Text content
                    if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                        text_buffer.push_str(text);
                        let _ = tx.send(StreamEvent::TextDelta {
                            text: text.to_string(),
                        });
                    }

                    // Tool calls
                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                        for tc in tool_calls {
                            // New tool call starting
                            if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                                // Flush previous tool call if any
                                if let Some(prev_id) = current_tool_id.take() {
                                    let args: serde_json::Value =
                                        serde_json::from_str(&current_tool_args).unwrap_or(
                                            serde_json::Value::Object(serde_json::Map::new()),
                                        );
                                    content_blocks.push(ContentBlock::ToolUse {
                                        id: prev_id,
                                        name: current_tool_name.take().unwrap_or_default(),
                                        input: args,
                                    });
                                    current_tool_args.clear();
                                }

                                let name = tc
                                    .get("function")
                                    .and_then(|f| f.get("name"))
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                current_tool_id = Some(id.to_string());
                                current_tool_name = Some(name.clone());
                                let _ = tx.send(StreamEvent::ToolStart {
                                    tool_use_id: id.to_string(),
                                    name,
                                });
                            }

                            // Argument delta
                            if let Some(args) = tc
                                .get("function")
                                .and_then(|f| f.get("arguments"))
                                .and_then(|a| a.as_str())
                            {
                                current_tool_args.push_str(args);
                                if let Some(ref id) = current_tool_id {
                                    let _ = tx.send(StreamEvent::ToolInputDelta {
                                        tool_use_id: id.clone(),
                                        json: args.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Flush remaining text if stream ended without [DONE]
        if !text_buffer.is_empty() {
            content_blocks.push(ContentBlock::Text {
                text: text_buffer,
            });
        }

        Ok(CompletionResponse {
            content: content_blocks,
            stop_reason,
            usage,
        })
    }
}

// --- Request serialization ---

/// Build the JSON request body for the OpenAI Chat Completions API.
pub(crate) fn build_request_body(
    request: &CompletionRequest,
    stream: bool,
) -> serde_json::Value {
    let mut messages = Vec::new();

    // System message
    if !request.system.is_empty() {
        messages.push(serde_json::json!({
            "role": "system",
            "content": request.system
        }));
    }

    // Conversation messages
    for msg in &request.messages {
        match msg {
            Message::System { content } => {
                messages.push(serde_json::json!({
                    "role": "system",
                    "content": content
                }));
            }
            Message::User { content } => {
                // Check if this contains tool results — they become separate "tool" messages
                for item in content {
                    match item {
                        UserContent::Text { text } => {
                            messages.push(serde_json::json!({
                                "role": "user",
                                "content": text
                            }));
                        }
                        UserContent::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } => {
                            messages.push(serde_json::json!({
                                "role": "tool",
                                "tool_call_id": tool_use_id,
                                "content": content
                            }));
                        }
                        UserContent::Image { media_type, data } => {
                            messages.push(serde_json::json!({
                                "role": "user",
                                "content": [{
                                    "type": "image_url",
                                    "image_url": {
                                        "url": format!("data:{media_type};base64,{data}")
                                    }
                                }]
                            }));
                        }
                    }
                }
            }
            Message::Assistant { content } => {
                let mut text_parts = Vec::new();
                let mut tool_calls = Vec::new();

                for block in content {
                    match block {
                        ContentBlock::Text { text } => {
                            text_parts.push(text.clone());
                        }
                        ContentBlock::ToolUse { id, name, input } => {
                            tool_calls.push(serde_json::json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": input.to_string()
                                }
                            }));
                        }
                    }
                }

                let mut msg = serde_json::json!({
                    "role": "assistant",
                    "content": if text_parts.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(text_parts.join("")) }
                });

                if !tool_calls.is_empty() {
                    msg["tool_calls"] = serde_json::Value::Array(tool_calls);
                }

                messages.push(msg);
            }
        }
    }

    let mut body = serde_json::json!({
        "model": request.model,
        "max_tokens": request.max_tokens,
        "messages": messages,
    });

    if let Some(temp) = request.temperature {
        body["temperature"] = serde_json::json!(temp);
    }

    if !request.tools.is_empty() {
        let tools: Vec<serde_json::Value> = request
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema
                    }
                })
            })
            .collect();
        body["tools"] = serde_json::Value::Array(tools);
    }

    if stream {
        body["stream"] = serde_json::json!(true);
    }

    body
}

// --- Response parsing ---

/// Parse the JSON response body from the OpenAI Chat Completions API.
pub(crate) fn parse_response_body(json: serde_json::Value) -> Result<CompletionResponse, LlmError> {
    let choice = json
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or_else(|| LlmError::Stream("no choices in response".to_string()))?;

    let message = choice
        .get("message")
        .ok_or_else(|| LlmError::Stream("no message in choice".to_string()))?;

    let mut content = Vec::new();

    // Text content
    if let Some(text) = message.get("content").and_then(|c| c.as_str()) {
        if !text.is_empty() {
            content.push(ContentBlock::Text {
                text: text.to_string(),
            });
        }
    }

    // Tool calls
    if let Some(tool_calls) = message.get("tool_calls").and_then(|t| t.as_array()) {
        for tc in tool_calls {
            let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
            let function = tc.get("function").unwrap_or(&serde_json::Value::Null);
            let name = function
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = function
                .get("arguments")
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            let input: serde_json::Value =
                serde_json::from_str(arguments).unwrap_or(serde_json::json!({}));

            content.push(ContentBlock::ToolUse { id, name, input });
        }
    }

    let stop_reason = choice
        .get("finish_reason")
        .and_then(|r| r.as_str())
        .map(parse_finish_reason)
        .unwrap_or(StopReason::EndTurn);

    let usage = json.get("usage").map_or(Usage::default(), |u| Usage {
        input_tokens: u
            .get("prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        output_tokens: u
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    });

    Ok(CompletionResponse {
        content,
        stop_reason,
        usage,
    })
}

fn parse_finish_reason(reason: &str) -> StopReason {
    match reason {
        "stop" => StopReason::EndTurn,
        "tool_calls" => StopReason::ToolUse,
        "length" => StopReason::MaxTokens,
        other => StopReason::Other(other.to_string()),
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib llm::openai`
Expected: PASS (all 9 tests)

**Step 5: Commit**

```bash
git add src/llm/openai.rs
git commit -m "feat(llm): add OpenAI Chat Completions client with serialization + parsing"
```

---

### Task 6: Wire Up mod.rs — Trait, Factory, Re-exports

**Files:**
- Modify: `src/llm/mod.rs` (replace entire contents)
- Test: unit test for factory error case

**Step 1: Write the failing test**

Replace `src/llm/mod.rs` with tests first:

```rust
//! LLM abstraction — thin provider clients.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_client_unsupported_provider() {
        let config = LlmConfig {
            provider: "mystery".into(),
            api_key: SecretString::from("key"),
            base_url: None,
            model: "model".into(),
            max_tokens: 1024,
            max_retries: 3,
        };
        let result = create_client(&config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            LlmError::UnsupportedProvider(_)
        ));
    }

    #[test]
    fn create_client_known_providers() {
        for provider in &["openai", "xai", "openrouter", "groq", "ollama", "deepseek"] {
            let config = LlmConfig {
                provider: provider.to_string(),
                api_key: SecretString::from("key"),
                base_url: None,
                model: "model".into(),
                max_tokens: 1024,
                max_retries: 3,
            };
            assert!(
                create_client(&config).is_ok(),
                "failed to create client for provider: {provider}"
            );
        }
    }

    #[test]
    fn create_client_custom_base_url() {
        let config = LlmConfig {
            provider: "openai".into(),
            api_key: SecretString::from("key"),
            base_url: Some("http://localhost:8080/v1".into()),
            model: "model".into(),
            max_tokens: 1024,
            max_retries: 3,
        };
        assert!(create_client(&config).is_ok());
    }

    #[test]
    fn default_base_urls_populated() {
        assert!(default_base_url("openai").contains("openai.com"));
        assert!(default_base_url("xai").contains("x.ai"));
        assert!(default_base_url("openrouter").contains("openrouter"));
        assert!(default_base_url("groq").contains("groq.com"));
        assert!(default_base_url("ollama").contains("localhost"));
        assert!(default_base_url("deepseek").contains("deepseek"));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib llm::tests`
Expected: FAIL — `LlmClient`, `create_client`, etc. not defined

**Step 3: Write the implementation above the tests**

```rust
//! LLM abstraction — thin provider clients.
//!
//! Provides a single `LlmClient` trait with `complete()` and `complete_stream()`
//! methods. One concrete implementation (`OpenAiClient`) handles all providers
//! that speak the OpenAI Chat Completions wire format.

pub mod error;
pub mod openai;
pub mod sse;
pub mod types;

use async_trait::async_trait;
use secrecy::SecretString;
use tokio::sync::mpsc::UnboundedSender;

pub use error::LlmError;
pub use types::*;

/// Thin LLM client. One method for non-streaming, one for streaming.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Send a completion request, return the full response.
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, LlmError>;

    /// Send a completion request with streaming. Emits events via `tx` as they
    /// arrive. Returns the assembled final response when the stream completes.
    async fn complete_stream(
        &self,
        request: &CompletionRequest,
        tx: &UnboundedSender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError>;
}

/// LLM provider configuration.
pub struct LlmConfig {
    /// Provider name: "openai", "xai", "openrouter", "groq", "ollama", "deepseek".
    pub provider: String,
    /// API key.
    pub api_key: SecretString,
    /// Optional base URL override.
    pub base_url: Option<String>,
    /// Default model for completions.
    pub model: String,
    /// Default max tokens.
    pub max_tokens: u32,
    /// Max retries on 429.
    pub max_retries: u32,
}

/// Create an LLM client from configuration.
pub fn create_client(config: &LlmConfig) -> Result<Box<dyn LlmClient>, LlmError> {
    let base_url = config
        .base_url
        .clone()
        .unwrap_or_else(|| default_base_url(&config.provider));

    match config.provider.as_str() {
        "openai" | "xai" | "openrouter" | "groq" | "ollama" | "deepseek" => {
            Ok(Box::new(openai::OpenAiClient::new(
                config.api_key.clone(),
                base_url,
                config.max_retries,
            )))
        }
        other => Err(LlmError::UnsupportedProvider(other.to_string())),
    }
}

fn default_base_url(provider: &str) -> String {
    match provider {
        "openai" => "https://api.openai.com/v1",
        "xai" => "https://api.x.ai/v1",
        "openrouter" => "https://openrouter.ai/api/v1",
        "groq" => "https://api.groq.com/openai/v1",
        "ollama" => "http://localhost:11434/v1",
        "deepseek" => "https://api.deepseek.com",
        _ => "",
    }
    .to_string()
}
```

**Step 4: Run all LLM tests to verify they pass**

Run: `cargo test --lib llm`
Expected: PASS (all tests across error, types, sse, openai, mod)

**Step 5: Commit**

```bash
git add src/llm/mod.rs
git commit -m "feat(llm): add LlmClient trait, factory, and re-exports"
```

---

### Task 7: Update Config for LLM Provider Settings

**Files:**
- Modify: `src/config/mod.rs` — replace `anthropic_api_key` with `LlmConfig` fields
- Modify: `tests/config_test.rs` — update test env vars
- Modify: `.env.example` — update env var names

**Step 1: Update the config test**

Edit `tests/config_test.rs`:

```rust
use animus_rs::config::Config;

#[test]
fn config_from_env_loads_required_fields() {
    unsafe {
        std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");
    }

    let config = Config::from_env().unwrap();
    assert!(!config.log_level.is_empty());
    // LLM config is optional
    assert!(config.llm.is_none());

    unsafe {
        std::env::remove_var("DATABASE_URL");
    }
}

#[test]
fn config_from_env_fails_without_required() {
    unsafe {
        std::env::remove_var("DATABASE_URL");
    }

    let result = Config::from_env();
    assert!(result.is_err());
}

#[test]
fn config_loads_llm_when_provider_set() {
    unsafe {
        std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");
        std::env::set_var("LLM_PROVIDER", "xai");
        std::env::set_var("LLM_API_KEY", "xai-test-key");
        std::env::set_var("LLM_MODEL", "grok-3-latest");
    }

    let config = Config::from_env().unwrap();
    let llm = config.llm.unwrap();
    assert_eq!(llm.provider, "xai");
    assert_eq!(llm.model, "grok-3-latest");
    assert_eq!(llm.max_tokens, 8192); // default
    assert_eq!(llm.max_retries, 3); // default

    unsafe {
        std::env::remove_var("DATABASE_URL");
        std::env::remove_var("LLM_PROVIDER");
        std::env::remove_var("LLM_API_KEY");
        std::env::remove_var("LLM_MODEL");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test config_test`
Expected: FAIL — `config.llm` doesn't exist

**Step 3: Update `src/config/mod.rs`**

```rust
//! Typed configuration from environment variables.
//!
//! Loads once at startup, fails fast if required vars are missing.
//! Sensitive values wrapped in secrecy::SecretString to prevent log leaks.

pub mod secrets;

use crate::error::{Error, Result};
use crate::llm::LlmConfig;
use secrecy::SecretString;

#[derive(Debug)]
pub struct Config {
    pub database_url: SecretString,
    /// LLM configuration — optional until a skill actually needs LLM access.
    pub llm: Option<LlmConfig>,
    pub otel_endpoint: Option<String>,
    pub log_level: String,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// In local dev, call `dotenvy::dotenv().ok()` before this.
    /// In production, systemd EnvironmentFile provides the vars.
    pub fn from_env() -> Result<Self> {
        let llm = match std::env::var("LLM_PROVIDER").ok() {
            Some(provider) => {
                let api_key = SecretString::from(required_var("LLM_API_KEY")?);
                let model = required_var("LLM_MODEL")?;
                Some(LlmConfig {
                    provider,
                    api_key,
                    base_url: std::env::var("LLM_BASE_URL").ok(),
                    model,
                    max_tokens: std::env::var("LLM_MAX_TOKENS")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(8192),
                    max_retries: std::env::var("LLM_MAX_RETRIES")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(3),
                })
            }
            None => None,
        };

        Ok(Self {
            database_url: SecretString::from(required_var("DATABASE_URL")?),
            llm,
            otel_endpoint: std::env::var("OTEL_ENDPOINT").ok(),
            log_level: std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
        })
    }
}

fn required_var(name: &str) -> Result<String> {
    std::env::var(name)
        .map_err(|_| Error::Config(format!("required environment variable {name} is not set")))
}
```

Note: `LlmConfig` needs `Debug`. Add `#[derive(Debug)]` to it in `src/llm/mod.rs`:

```rust
#[derive(Debug)]
pub struct LlmConfig {
    // ... fields unchanged
}
```

**Step 4: Update `.env.example`**

Replace the `ANTHROPIC_API_KEY` line with:

```
# LLM provider (openai, xai, openrouter, groq, ollama, deepseek)
# LLM_PROVIDER=xai
# LLM_API_KEY=xai-your-key-here
# LLM_MODEL=grok-3-latest
# LLM_BASE_URL=              # optional override
# LLM_MAX_TOKENS=8192        # optional, default 8192
# LLM_MAX_RETRIES=3          # optional, default 3
```

**Step 5: Run tests**

Run: `cargo test --test config_test`
Expected: PASS (all 3 tests)

Then: `cargo test`
Expected: PASS (all tests)

**Step 6: Commit**

```bash
git add src/config/mod.rs src/llm/mod.rs tests/config_test.rs .env.example
git commit -m "feat(config): replace anthropic_api_key with generic LLM provider config"
```

---

### Task 8: Update lib.rs Doc Comment and Clean Up

**Files:**
- Modify: `src/lib.rs:8` — remove "rig-core" mention
- Verify: no remaining direct `rig-core` imports in `src/llm/`

**Step 1: Update doc comment**

In `src/lib.rs`, change line 8 from:

```rust
//! plane (queue watching, resource gating, focus spawning), faculties
//! (pluggable cognitive specializations), LLM abstraction (rig-core), and
```

to:

```rust
//! plane (queue watching, resource gating, focus spawning), faculties
//! (pluggable cognitive specializations), LLM abstraction, and
```

**Step 2: Verify no rig-core imports in src/llm/**

Run: `grep -r "rig::" src/llm/`
Expected: no output (no rig-core imports remain)

**Step 3: Run full test + clippy**

Run: `cargo test && cargo clippy -- -D warnings`
Expected: all tests pass, no clippy warnings

**Step 4: Commit**

```bash
git add src/lib.rs
git commit -m "chore: remove rig-core reference from lib.rs doc comment"
```

---

### Task 9: Add Integration Test (Ignored by Default)

**Files:**
- Create: `tests/llm_test.rs` — integration test that calls a real LLM API (ignored without env vars)

**Step 1: Write the integration test**

```rust
//! LLM integration tests.
//!
//! These tests call real LLM APIs and require credentials.
//! Run with: cargo test --test llm_test -- --ignored --nocapture

use animus_rs::llm::{
    create_client, CompletionRequest, LlmConfig, Message, StopReason, StreamEvent, UserContent,
};
use secrecy::SecretString;

fn llm_config_from_env() -> Option<LlmConfig> {
    let provider = std::env::var("LLM_PROVIDER").ok()?;
    let api_key = std::env::var("LLM_API_KEY").ok()?;
    let model = std::env::var("LLM_MODEL").ok()?;
    Some(LlmConfig {
        provider,
        api_key: SecretString::from(api_key),
        base_url: std::env::var("LLM_BASE_URL").ok(),
        model,
        max_tokens: 256,
        max_retries: 2,
    })
}

#[tokio::test]
#[ignore]
async fn complete_simple_text() {
    let config = llm_config_from_env().expect("LLM_PROVIDER, LLM_API_KEY, LLM_MODEL must be set");
    let client = create_client(&config).unwrap();

    let request = CompletionRequest {
        model: config.model.clone(),
        system: "You are a helpful assistant. Respond in one short sentence.".into(),
        messages: vec![Message::User {
            content: vec![UserContent::Text {
                text: "What is 2+2?".into(),
            }],
        }],
        tools: vec![],
        max_tokens: config.max_tokens,
        temperature: Some(0.0),
    };

    let response = client.complete(&request).await.unwrap();
    assert!(!response.content.is_empty());
    assert_eq!(response.stop_reason, StopReason::EndTurn);
    assert!(response.usage.input_tokens > 0);
    assert!(response.usage.output_tokens > 0);
    println!("Response: {:?}", response.content);
    println!("Usage: {:?}", response.usage);
}

#[tokio::test]
#[ignore]
async fn complete_stream_simple_text() {
    let config = llm_config_from_env().expect("LLM_PROVIDER, LLM_API_KEY, LLM_MODEL must be set");
    let client = create_client(&config).unwrap();

    let request = CompletionRequest {
        model: config.model.clone(),
        system: "You are a helpful assistant. Respond in one short sentence.".into(),
        messages: vec![Message::User {
            content: vec![UserContent::Text {
                text: "What is 2+2?".into(),
            }],
        }],
        tools: vec![],
        max_tokens: config.max_tokens,
        temperature: Some(0.0),
    };

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let response = client.complete_stream(&request, &tx).await.unwrap();

    // Collect stream events
    drop(tx); // close the sender so rx terminates
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    assert!(!events.is_empty(), "should have received stream events");
    assert!(
        events.iter().any(|e| matches!(e, StreamEvent::Done)),
        "should have received Done event"
    );
    assert!(!response.content.is_empty());
    println!("Stream events: {}", events.len());
    println!("Response: {:?}", response.content);
}
```

**Step 2: Run test (should be skipped without --ignored)**

Run: `cargo test --test llm_test`
Expected: 0 tests run (both ignored)

**Step 3: Run with credentials (manual verification)**

Run: `cargo test --test llm_test -- --ignored --nocapture`
Expected: PASS if LLM_PROVIDER/LLM_API_KEY/LLM_MODEL are set

**Step 4: Commit**

```bash
git add tests/llm_test.rs
git commit -m "test(llm): add integration tests for LLM client (ignored by default)"
```

---

### Task 10: Update docs/llm.md — Mark OpenAI-Only Scope

**Files:**
- Modify: `docs/llm.md` — add note at top about current scope

**Step 1: Add scope note**

At the top of `docs/llm.md`, after the frontmatter, add:

```markdown
> **Current scope (2026-03-02):** Only the OpenAI-compatible provider is implemented.
> All providers (OpenAI, xAI, OpenRouter, Groq, Ollama, DeepSeek) use the same
> `OpenAiClient` with different base URLs. Anthropic Messages API provider deferred.
> See `docs/plans/2026-03-02-llm-thin-client-design.md` for the decision record.
```

**Step 2: Commit**

```bash
git add docs/llm.md
git commit -m "docs: mark llm.md with current OpenAI-only scope"
```

---

## Summary

| Task | What | Files | Tests |
|------|------|-------|-------|
| 1 | Dependencies | `Cargo.toml` | cargo build |
| 2 | Error type | `src/llm/error.rs` | 5 unit tests |
| 3 | Canonical types | `src/llm/types.rs` | 6 unit tests |
| 4 | SSE parser | `src/llm/sse.rs` | 8 unit tests |
| 5 | OpenAI client | `src/llm/openai.rs` | 9 unit tests |
| 6 | Trait + factory | `src/llm/mod.rs` | 4 unit tests |
| 7 | Config migration | `src/config/mod.rs`, `tests/config_test.rs`, `.env.example` | 3 tests |
| 8 | Cleanup | `src/lib.rs` | cargo test + clippy |
| 9 | Integration test | `tests/llm_test.rs` | 2 ignored tests |
| 10 | Docs update | `docs/llm.md` | — |

Total: ~32 unit tests, 2 integration tests, 10 commits.
