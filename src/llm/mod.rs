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
pub trait LlmClient: Send + Sync + std::fmt::Debug {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError>;

    async fn complete_stream(
        &self,
        request: &CompletionRequest,
        tx: &UnboundedSender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError>;
}

/// LLM provider configuration.
#[derive(Debug)]
pub struct LlmConfig {
    pub provider: String,
    pub api_key: SecretString,
    pub base_url: Option<String>,
    pub model: String,
    pub max_tokens: u32,
    pub max_retries: u32,
}

/// Create an LLM client from configuration.
pub fn create_client(config: &LlmConfig) -> Result<Box<dyn LlmClient>, LlmError> {
    let base_url = config
        .base_url
        .clone()
        .unwrap_or_else(|| default_base_url(&config.provider));

    match config.provider.as_str() {
        "openai" | "xai" | "openrouter" | "groq" | "ollama" | "deepseek" => Ok(Box::new(
            openai::OpenAiClient::new(config.api_key.clone(), base_url, config.max_retries),
        )),
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
