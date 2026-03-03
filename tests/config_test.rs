use animus_rs::config::Config;

#[test]
fn config_from_env_loads_required_fields() {
    unsafe {
        std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");
    }

    let config = Config::from_env().unwrap();
    assert!(!config.log_level.is_empty());
    assert!(config.llm.is_none()); // LLM config is optional

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
    assert_eq!(llm.max_tokens, 8192);
    assert_eq!(llm.max_retries, 3);

    unsafe {
        std::env::remove_var("DATABASE_URL");
        std::env::remove_var("LLM_PROVIDER");
        std::env::remove_var("LLM_API_KEY");
        std::env::remove_var("LLM_MODEL");
    }
}
