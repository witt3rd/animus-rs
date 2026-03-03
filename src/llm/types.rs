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
            content: vec![UserContent::Text { text: "hi".into() }],
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
