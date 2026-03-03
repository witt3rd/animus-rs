# LLM Thin Client — OpenAI-Compatible Only

*Date: 2026-03-02*
*Status: approved*
*Spec: docs/llm.md § Design (simplified — Anthropic provider deferred)*

## Decision

Drop `rig-core` for LLM completion calls. Replace with a thin `reqwest`-based client implementing the OpenAI Chat Completions wire format. All providers (OpenAI, xAI, OpenRouter, Groq, Ollama, DeepSeek) speak this format — just with different base URLs.

Anthropic-specific Messages API support deferred. Anthropic models accessible via OpenRouter.

## Module Structure

```
src/llm/
    mod.rs          — LlmClient trait, LlmConfig, create_client factory, re-exports
    types.rs        — Message, ContentBlock, ToolDefinition, Usage, StopReason, StreamEvent
    error.rs        — LlmError enum
    openai.rs       — OpenAI Chat Completions client (reqwest + SSE)
    sse.rs          — SSE stream parser
```

~300-400 lines total.

## The Trait

```rust
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, LlmError>;

    async fn complete_stream(
        &self,
        request: &CompletionRequest,
        tx: &tokio::sync::mpsc::UnboundedSender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError>;
}
```

## Types

Canonical types owned by us, mapped to OpenAI wire format by the client:

- `CompletionRequest` — model, system, messages, tools, max_tokens, temperature
- `CompletionResponse` — content blocks, stop_reason, usage
- `Message` — System / User / Assistant variants
- `UserContent` — Text / ToolResult / Image
- `ContentBlock` — Text / ToolUse
- `ToolDefinition` — name, description, input_schema (JSON Value)
- `StopReason` — EndTurn / ToolUse / MaxTokens / Other
- `Usage` — input_tokens, output_tokens
- `StreamEvent` — TextDelta / ToolInputDelta / ToolStart / Done

Full type definitions in docs/llm.md § Types.

## Error Type

```rust
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    Http(reqwest::Error),
    Api { status: u16, message: String },
    RateLimited { retry_after_secs: Option<u64> },
    Json(serde_json::Error),
    Stream(String),
    UnsupportedProvider(String),
}
```

## Provider Routing

Single client, factory selects base URL:

```rust
pub fn create_client(config: &LlmConfig) -> Result<Box<dyn LlmClient>, LlmError> {
    let base_url = config.base_url.clone()
        .unwrap_or_else(|| default_base_url(&config.provider));
    Ok(Box::new(OpenAiClient::new(config.api_key.clone(), base_url, config.max_retries)))
}

fn default_base_url(provider: &str) -> String {
    match provider {
        "openai"     => "https://api.openai.com/v1",
        "xai"        => "https://api.x.ai/v1",
        "openrouter" => "https://openrouter.ai/api/v1",
        "groq"       => "https://api.groq.com/openai/v1",
        "ollama"     => "http://localhost:11434/v1",
        "deepseek"   => "https://api.deepseek.com",
        _            => panic!("unknown provider"),
    }.to_string()
}
```

## OpenAI Wire Format Mapping

| Our type | OpenAI wire format |
|---|---|
| `Message::System { content }` | `{ "role": "system", "content": "..." }` |
| `Message::User { content: [Text] }` | `{ "role": "user", "content": "..." }` |
| `Message::User { content: [ToolResult] }` | `{ "role": "tool", "tool_call_id": "...", "content": "..." }` |
| `Message::Assistant { content }` | `{ "role": "assistant", "content": "...", "tool_calls": [...] }` |
| `ContentBlock::ToolUse` | `tool_calls[].function` |
| `StopReason::ToolUse` | `finish_reason: "tool_calls"` |
| `StopReason::EndTurn` | `finish_reason: "stop"` |

## Config

```rust
pub struct LlmConfig {
    pub provider: String,           // LLM_PROVIDER
    pub api_key: SecretString,      // LLM_API_KEY
    pub base_url: Option<String>,   // LLM_BASE_URL (optional override)
    pub model: String,              // LLM_MODEL
    pub max_tokens: u32,            // LLM_MAX_TOKENS (default 8192)
    pub max_retries: u32,           // LLM_MAX_RETRIES (default 3)
}
```

Replaces `anthropic_api_key: Option<SecretString>` in `Config`.

## Dependencies Change

```toml
# Add
reqwest = { version = "0.12", features = ["json", "stream"] }
async-trait = "0.1"
futures-util = "0.3"

# Keep (transitive for rig-postgres embedding search)
rig-core = "0.31"
rig-postgres = "0.1"
```

Stop importing `rig-core` directly. It remains only as a transitive dep for `rig-postgres`.

## Migration

1. Promote `reqwest` to `[dependencies]`, add `async-trait` + `futures-util`
2. Implement `src/llm/error.rs`, `src/llm/types.rs`, `src/llm/sse.rs`, `src/llm/openai.rs`
3. Replace `src/llm/mod.rs` — trait + factory + re-exports
4. Update `src/config/mod.rs` — `LlmConfig` struct, new env vars
5. Update `.env.example` with new env var names
6. Remove direct `rig-core` imports from `src/llm/`
7. Update `src/lib.rs` doc comment (remove "rig-core" mention)
8. Verify `cargo build`, `cargo test`, `cargo clippy`

## What We're Not Building

- Anthropic Messages API provider (deferred)
- Agent runtime (that's the act loop)
- Tool execution (act loop via JoinSet)
- Context management (act loop)
- Embedding models (rig-postgres handles this)
- Batch API
