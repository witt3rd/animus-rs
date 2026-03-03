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
