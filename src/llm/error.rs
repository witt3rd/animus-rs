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
