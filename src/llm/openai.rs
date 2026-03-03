//! OpenAI Chat Completions wire-format client.
//!
//! Translates our provider-independent [`CompletionRequest`] / [`CompletionResponse`]
//! types to and from the OpenAI Chat Completions JSON format. Handles both
//! synchronous and streaming calls over a thin `reqwest` HTTP client.

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, warn};

use crate::llm::error::LlmError;
use crate::llm::sse::SseParser;
use crate::llm::types::*;

/// Default base URL for the OpenAI API.
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Thin client for the OpenAI Chat Completions endpoint.
///
/// Owns a `reqwest::Client`, an API key, and retry configuration. Implements
/// the [`LlmClient`](super::LlmClient) trait for both non-streaming and
/// streaming completions.
#[derive(Debug)]
pub struct OpenAiClient {
    http: reqwest::Client,
    api_key: SecretString,
    base_url: String,
    max_retries: u32,
}

impl OpenAiClient {
    /// Create a new OpenAI client.
    pub fn new(api_key: SecretString, base_url: String, max_retries: u32) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
            base_url,
            max_retries,
        }
    }

    /// Create a client with the default OpenAI base URL.
    pub fn with_defaults(api_key: SecretString) -> Self {
        Self::new(api_key, DEFAULT_BASE_URL.to_string(), 3)
    }
}

#[async_trait]
impl super::LlmClient for OpenAiClient {
    /// Send a completion request and return the full response.
    ///
    /// Retries on 429 (rate limited) up to `max_retries` times with exponential backoff.
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let body = build_request_body(request, false);
        let url = format!("{}/chat/completions", self.base_url);

        let mut retries = 0;
        loop {
            let resp = self
                .http
                .post(&url)
                .bearer_auth(self.api_key.expose_secret())
                .json(&body)
                .send()
                .await?;

            let status = resp.status();

            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let retry_after = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok());

                if retries >= self.max_retries {
                    return Err(LlmError::RateLimited {
                        retry_after_secs: retry_after,
                    });
                }

                let backoff_secs = retry_after.unwrap_or(2u64.pow(retries));
                warn!(
                    retries,
                    backoff_secs, "Rate limited (429), backing off before retry"
                );
                tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                retries += 1;
                continue;
            }

            if !status.is_success() {
                let message = resp.text().await.unwrap_or_default();
                return Err(LlmError::Api {
                    status: status.as_u16(),
                    message,
                });
            }

            let json: serde_json::Value = resp.json().await?;
            debug!("OpenAI response received");
            return parse_response_body(json);
        }
    }

    /// Send a streaming completion request, emitting events to `tx`.
    ///
    /// Returns the final aggregated response once the stream ends.
    async fn complete_stream(
        &self,
        request: &CompletionRequest,
        tx: &UnboundedSender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError> {
        let body = build_request_body(request, true);
        let url = format!("{}/chat/completions", self.base_url);

        let resp = self
            .http
            .post(&url)
            .bearer_auth(self.api_key.expose_secret())
            .json(&body)
            .send()
            .await?;

        let status = resp.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            return Err(LlmError::RateLimited {
                retry_after_secs: retry_after,
            });
        }

        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api {
                status: status.as_u16(),
                message,
            });
        }

        use futures_util::StreamExt;

        let mut sse_parser = SseParser::new();
        let mut content_blocks: Vec<ContentBlock> = Vec::new();
        let mut current_thinking = String::new();
        let mut current_text = String::new();
        let mut current_tool_id = String::new();
        let mut current_tool_name = String::new();
        let mut current_tool_args = String::new();
        let mut stop_reason = StopReason::EndTurn;
        let mut usage = Usage::default();

        let mut stream = resp.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            let chunk_str = String::from_utf8_lossy(&chunk);

            for event in sse_parser.feed(&chunk_str) {
                if event.data == "[DONE]" {
                    // Flush any pending thinking.
                    if !current_thinking.is_empty() {
                        content_blocks.push(ContentBlock::Thinking {
                            thinking: std::mem::take(&mut current_thinking),
                        });
                    }
                    // Flush any pending text.
                    if !current_text.is_empty() {
                        content_blocks.push(ContentBlock::Text {
                            text: std::mem::take(&mut current_text),
                        });
                    }
                    // Flush any pending tool call.
                    if !current_tool_id.is_empty() {
                        let input: serde_json::Value =
                            serde_json::from_str(&current_tool_args).unwrap_or(json!({}));
                        content_blocks.push(ContentBlock::ToolUse {
                            id: std::mem::take(&mut current_tool_id),
                            name: std::mem::take(&mut current_tool_name),
                            input,
                        });
                        current_tool_args.clear();
                    }
                    let _ = tx.send(StreamEvent::Done);
                    break;
                }

                let data: serde_json::Value = match serde_json::from_str(&event.data) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("Failed to parse SSE data: {e}");
                        continue;
                    }
                };

                // Extract finish_reason if present.
                if let Some(reason) = data["choices"][0]["finish_reason"].as_str() {
                    stop_reason = parse_finish_reason(reason);
                }

                // Extract usage if present (some providers send it in the final chunk).
                if let Some(u) = data.get("usage") {
                    if let Some(pt) = u["prompt_tokens"].as_u64() {
                        usage.input_tokens = pt as u32;
                    }
                    if let Some(ct) = u["completion_tokens"].as_u64() {
                        usage.output_tokens = ct as u32;
                    }
                }

                let delta = &data["choices"][0]["delta"];

                // Reasoning/thinking content delta (reasoning models).
                if let Some(thinking) = delta["reasoning_content"].as_str() {
                    current_thinking.push_str(thinking);
                    let _ = tx.send(StreamEvent::ThinkingDelta {
                        text: thinking.to_string(),
                    });
                }

                // Text content delta.
                if let Some(text) = delta["content"].as_str() {
                    current_text.push_str(text);
                    let _ = tx.send(StreamEvent::TextDelta {
                        text: text.to_string(),
                    });
                }

                // Tool calls delta.
                if let Some(tool_calls) = delta["tool_calls"].as_array() {
                    for tc in tool_calls {
                        // New tool call starting.
                        if let Some(id) = tc["id"].as_str() {
                            // Flush previous tool call if any.
                            if !current_tool_id.is_empty() {
                                let input: serde_json::Value =
                                    serde_json::from_str(&current_tool_args).unwrap_or(json!({}));
                                content_blocks.push(ContentBlock::ToolUse {
                                    id: std::mem::take(&mut current_tool_id),
                                    name: std::mem::take(&mut current_tool_name),
                                    input,
                                });
                                current_tool_args.clear();
                            }
                            current_tool_id = id.to_string();
                            current_tool_name =
                                tc["function"]["name"].as_str().unwrap_or("").to_string();
                            let _ = tx.send(StreamEvent::ToolStart {
                                tool_use_id: current_tool_id.clone(),
                                name: current_tool_name.clone(),
                            });
                        }

                        // Accumulate arguments fragment.
                        if let Some(args) = tc["function"]["arguments"].as_str() {
                            current_tool_args.push_str(args);
                            let _ = tx.send(StreamEvent::ToolInputDelta {
                                tool_use_id: current_tool_id.clone(),
                                json: args.to_string(),
                            });
                        }
                    }
                }
            }
        }

        Ok(CompletionResponse {
            content: content_blocks,
            stop_reason,
            usage,
        })
    }
}

// ---------------------------------------------------------------------------
// Wire-format serialization / deserialization
// ---------------------------------------------------------------------------

/// Build the JSON request body for the OpenAI Chat Completions endpoint.
///
/// Converts our provider-independent [`CompletionRequest`] into the OpenAI
/// wire format. The `stream` flag controls whether `"stream": true` is set.
pub(crate) fn build_request_body(request: &CompletionRequest, stream: bool) -> serde_json::Value {
    let mut messages = Vec::<serde_json::Value>::new();

    // System message — always first.
    messages.push(json!({
        "role": "system",
        "content": request.system,
    }));

    // Convert each canonical Message into one or more OpenAI wire messages.
    for msg in &request.messages {
        match msg {
            Message::System { content } => {
                messages.push(json!({
                    "role": "system",
                    "content": content,
                }));
            }
            Message::User { content } => {
                serialize_user_message(content, &mut messages);
            }
            Message::Assistant { content } => {
                serialize_assistant_message(content, &mut messages);
            }
        }
    }

    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "max_tokens": request.max_tokens,
    });

    if let Some(temp) = request.temperature {
        body["temperature"] = json!(temp);
    }

    if !request.tools.is_empty() {
        let tools: Vec<serde_json::Value> = request
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect();
        body["tools"] = json!(tools);
    }

    if stream {
        body["stream"] = json!(true);
    }

    body
}

/// Serialize a `User` message's content into OpenAI wire messages.
///
/// - `Text` items are combined into a single `{"role":"user", "content":"..."}`.
/// - `ToolResult` items become separate `{"role":"tool", ...}` messages.
/// - A mix produces both (user text first, then tool messages).
fn serialize_user_message(content: &[UserContent], messages: &mut Vec<serde_json::Value>) {
    let mut text_parts = Vec::new();
    let mut tool_results = Vec::new();

    for item in content {
        match item {
            UserContent::Text { text } => {
                text_parts.push(text.clone());
            }
            UserContent::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                tool_results.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_use_id,
                    "content": content,
                }));
            }
            UserContent::Image { media_type, data } => {
                // OpenAI vision format — include as image_url in user message.
                text_parts.push(format!(
                    "[image: {media_type}, {} bytes base64]",
                    data.len()
                ));
            }
        }
    }

    if !text_parts.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": text_parts.join("\n"),
        }));
    }

    messages.extend(tool_results);
}

/// Serialize an `Assistant` message into an OpenAI wire message.
///
/// Text blocks become `"content"`, tool-use blocks become `"tool_calls"`.
fn serialize_assistant_message(content: &[ContentBlock], messages: &mut Vec<serde_json::Value>) {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in content {
        match block {
            ContentBlock::Text { text } => {
                text_parts.push(text.clone());
            }
            ContentBlock::Thinking { .. } => {
                // Thinking blocks are not sent back in conversation history
            }
            ContentBlock::ToolUse { id, name, input } => {
                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": input.to_string(),
                    }
                }));
            }
        }
    }

    let mut msg = json!({ "role": "assistant" });

    let content_str = text_parts.join("\n");
    if !content_str.is_empty() {
        msg["content"] = json!(content_str);
    }

    if !tool_calls.is_empty() {
        msg["tool_calls"] = json!(tool_calls);
    }

    messages.push(msg);
}

/// Parse an OpenAI Chat Completions JSON response into our canonical types.
///
/// Handles text-only responses, tool-call responses, and mixed responses.
pub(crate) fn parse_response_body(json: serde_json::Value) -> Result<CompletionResponse, LlmError> {
    let choice = json["choices"]
        .get(0)
        .ok_or_else(|| LlmError::Stream("No choices in response".to_string()))?;

    let message = &choice["message"];
    let mut content = Vec::new();

    // Reasoning/thinking content (reasoning models: o1, DeepSeek-R1, Grok).
    if let Some(thinking) = message["reasoning_content"].as_str()
        && !thinking.is_empty()
    {
        content.push(ContentBlock::Thinking {
            thinking: thinking.to_string(),
        });
    }

    // Text content.
    if let Some(text) = message["content"].as_str()
        && !text.is_empty()
    {
        content.push(ContentBlock::Text {
            text: text.to_string(),
        });
    }

    // Tool calls.
    if let Some(tool_calls) = message["tool_calls"].as_array() {
        for tc in tool_calls {
            let id = tc["id"].as_str().unwrap_or("").to_string();
            let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
            let arguments_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
            let input: serde_json::Value = serde_json::from_str(arguments_str).unwrap_or(json!({}));

            content.push(ContentBlock::ToolUse { id, name, input });
        }
    }

    // Stop reason.
    let finish_reason = choice["finish_reason"].as_str().unwrap_or("stop");
    let stop_reason = parse_finish_reason(finish_reason);

    // Usage.
    let usage = Usage {
        input_tokens: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
        output_tokens: json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
    };

    Ok(CompletionResponse {
        content,
        stop_reason,
        usage,
    })
}

/// Map an OpenAI finish_reason string to our `StopReason` enum.
fn parse_finish_reason(reason: &str) -> StopReason {
    match reason {
        "stop" => StopReason::EndTurn,
        "length" => StopReason::MaxTokens,
        "tool_calls" => StopReason::ToolUse,
        other => StopReason::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(
            !body
                .get("stream")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        );

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

        let asst = &messages[1];
        assert_eq!(asst["role"], "assistant");
        assert_eq!(asst["content"], "Let me search.");
        let tool_calls = asst["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "call_abc");
        assert_eq!(tool_calls[0]["type"], "function");
        assert_eq!(tool_calls[0]["function"]["name"], "search");

        let tool_msg = &messages[2];
        assert_eq!(tool_msg["role"], "tool");
        assert_eq!(tool_msg["tool_call_id"], "call_abc");
        assert_eq!(tool_msg["content"], "found: rust lang");
    }

    #[test]
    fn parse_text_response() {
        let json = serde_json::json!({
            "id": "chatcmpl-123",
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
        assert!(
            matches!(&resp.content[0], ContentBlock::Text { text } if text == "Hello! How can I help?")
        );
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 8);
    }

    #[test]
    fn parse_tool_call_response() {
        let json = serde_json::json!({
            "choices": [{
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
            "usage": { "prompt_tokens": 20, "completion_tokens": 15, "total_tokens": 35 }
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
        assert_eq!(resp.content.len(), 2);
        assert!(matches!(&resp.content[0], ContentBlock::Text { .. }));
        assert!(matches!(&resp.content[1], ContentBlock::ToolUse { .. }));
    }

    #[test]
    fn parse_reasoning_content() {
        let json = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_content": "Let me think step by step...",
                    "content": "The answer is 4."
                },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30 }
        });
        let resp = parse_response_body(json).unwrap();
        assert_eq!(resp.content.len(), 2);
        assert!(
            matches!(&resp.content[0], ContentBlock::Thinking { thinking } if thinking == "Let me think step by step...")
        );
        assert!(
            matches!(&resp.content[1], ContentBlock::Text { text } if text == "The answer is 4.")
        );
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
