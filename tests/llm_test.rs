//! LLM integration tests.
//!
//! These tests call real LLM APIs and require credentials.
//! Run with: cargo test --test llm_test -- --ignored --nocapture

use animus_rs::llm::{
    CompletionRequest, LlmConfig, Message, StopReason, StreamEvent, UserContent, create_client,
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

    drop(tx);
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
