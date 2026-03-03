# `animus llm complete` Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `animus llm complete` CLI command that issues LLM prompts with file context, Tera templates, stdin piping, and streaming output.

**Architecture:** New `Llm` subcommand in the existing clap CLI (`src/bin/animus.rs`). Loads config, assembles system prompt from context files + stdin, renders user prompt via Tera, calls `LlmClient::complete()` or `complete_stream()`, formats output.

**Tech Stack:** clap (CLI framework, already present), tera (template engine, new dep), animus_rs::llm (LlmClient, just built)

**Design Doc:** `docs/plans/2026-03-02-llm-complete-cli-design.md`

---

### Task 1: Add Tera Dependency

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add tera to dependencies**

In `Cargo.toml`, after the `futures-util` line in the LLM section, add:

```toml
tera = { version = "1", default-features = false }
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles with no errors

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add tera template engine for CLI prompt rendering"
```

---

### Task 2: Add Llm Subcommand to CLI (Clap Structure Only)

**Files:**
- Modify: `src/bin/animus.rs`

**Step 1: Write the clap structure**

Add to the `Command` enum after the `Work` variant:

```rust
    /// LLM sub-calls
    Llm {
        #[command(subcommand)]
        action: LlmAction,
    },
```

Add the `LlmAction` enum after `WorkAction`:

```rust
#[derive(Subcommand)]
enum LlmAction {
    /// Run an LLM completion
    Complete {
        /// Inline user prompt
        #[arg(long, group = "prompt_source")]
        prompt: Option<String>,
        /// Tera template file as user prompt
        #[arg(long, group = "prompt_source")]
        prompt_file: Option<PathBuf>,
        /// Template variable (repeatable, KEY=VALUE)
        #[arg(long = "var", value_parser = parse_key_val)]
        vars: Vec<(String, String)>,
        /// File to include in system context (repeatable)
        #[arg(long = "context-file")]
        context_files: Vec<PathBuf>,
        /// Explicit system prompt
        #[arg(long)]
        system: Option<String>,
        /// Override model from config
        #[arg(long)]
        model: Option<String>,
        /// Output format (text, json, yaml)
        #[arg(long, default_value = "text")]
        format: String,
        /// Stream tokens to stdout
        #[arg(long)]
        stream: bool,
        /// Override max tokens from config
        #[arg(long)]
        max_tokens: Option<u32>,
    },
}

/// Parse KEY=VALUE pairs for --var flag.
fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=VALUE: no '=' found in '{s}'"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}
```

Add the match arm in `main()`:

```rust
        Command::Llm { action } => match action {
            LlmAction::Complete {
                prompt,
                prompt_file,
                vars,
                context_files,
                system,
                model,
                format,
                stream,
                max_tokens,
            } => {
                cmd_llm_complete(
                    prompt,
                    prompt_file,
                    vars,
                    context_files,
                    system,
                    model,
                    format,
                    stream,
                    max_tokens,
                )
                .await
            }
        },
```

Add a stub function:

```rust
#[allow(clippy::too_many_arguments)]
async fn cmd_llm_complete(
    _prompt: Option<String>,
    _prompt_file: Option<PathBuf>,
    _vars: Vec<(String, String)>,
    _context_files: Vec<PathBuf>,
    _system: Option<String>,
    _model: Option<String>,
    _format: String,
    _stream: bool,
    _max_tokens: Option<u32>,
) -> anyhow::Result<()> {
    anyhow::bail!("not yet implemented")
}
```

**Step 2: Verify it compiles and help works**

Run: `cargo build`
Run: `cargo run -- llm complete --help`
Expected: shows all flags (--prompt, --prompt-file, --var, --context-file, --system, --model, --format, --stream, --max-tokens)

**Step 3: Commit**

```bash
git add src/bin/animus.rs
git commit -m "feat(cli): add animus llm complete subcommand (stub)"
```

---

### Task 3: Implement System Prompt Assembly (Context Files + Stdin)

**Files:**
- Modify: `src/bin/animus.rs`

This is a pure function — easy to test. Add it as a standalone function.

**Step 1: Write the failing test**

Add to `src/bin/animus.rs` at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn build_system_prompt_empty() {
        let result = build_system_prompt(None, &[], None);
        assert_eq!(result, "");
    }

    #[test]
    fn build_system_prompt_system_only() {
        let result = build_system_prompt(Some("You are helpful."), &[], None);
        assert_eq!(result, "You are helpful.");
    }

    #[test]
    fn build_system_prompt_with_context() {
        let result = build_system_prompt(
            Some("You are helpful."),
            &[("docs/example.md".to_string(), "# Example\nContent here.".to_string())],
            None,
        );
        assert!(result.contains("You are helpful."));
        assert!(result.contains("--- FILE: docs/example.md ---"));
        assert!(result.contains("# Example\nContent here."));
        assert!(result.contains("--- END FILE ---"));
    }

    #[test]
    fn build_system_prompt_with_stdin() {
        let result = build_system_prompt(
            None,
            &[],
            Some("piped content".to_string()),
        );
        assert!(result.contains("--- STDIN ---"));
        assert!(result.contains("piped content"));
    }

    #[test]
    fn build_system_prompt_ordering() {
        let result = build_system_prompt(
            Some("system text"),
            &[("file.md".to_string(), "file content".to_string())],
            Some("stdin content".to_string()),
        );
        let system_pos = result.find("system text").unwrap();
        let file_pos = result.find("file content").unwrap();
        let stdin_pos = result.find("stdin content").unwrap();
        assert!(system_pos < file_pos);
        assert!(file_pos < stdin_pos);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --bin animus`
Expected: FAIL — `build_system_prompt` not defined

**Step 3: Implement**

```rust
/// Assemble the system prompt from explicit text, context files, and stdin.
fn build_system_prompt(
    system: Option<&str>,
    context_files: &[(String, String)],
    stdin_content: Option<String>,
) -> String {
    let mut parts = Vec::new();

    if let Some(sys) = system {
        parts.push(sys.to_string());
    }

    for (path, content) in context_files {
        parts.push(format!("--- FILE: {path} ---\n{content}\n--- END FILE ---"));
    }

    if let Some(stdin) = stdin_content {
        parts.push(format!("--- STDIN ---\n{stdin}\n--- END STDIN ---"));
    }

    parts.join("\n\n")
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --bin animus`
Expected: PASS (all 5 tests)

**Step 5: Commit**

```bash
git add src/bin/animus.rs
git commit -m "feat(cli): implement system prompt assembly with context files + stdin"
```

---

### Task 4: Implement Tera Template Rendering

**Files:**
- Modify: `src/bin/animus.rs`

**Step 1: Write the failing test**

Add to the `tests` module:

```rust
    #[test]
    fn render_prompt_inline() {
        let result = render_prompt(Some("Hello world"), None, &[]).unwrap();
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn render_prompt_template_with_vars() {
        // Write a temp file with Tera template
        let dir = std::env::temp_dir().join("animus_test_render");
        std::fs::create_dir_all(&dir).unwrap();
        let template_path = dir.join("test_template.md");
        std::fs::write(
            &template_path,
            "Hello {{ name }}, you are {{ role }}.",
        )
        .unwrap();

        let result = render_prompt(
            None,
            Some(&template_path),
            &[
                ("name".to_string(), "Kelly".to_string()),
                ("role".to_string(), "an engineer".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(result, "Hello Kelly, you are an engineer.");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn render_prompt_neither_errors() {
        let result = render_prompt(None, None, &[]);
        assert!(result.is_err());
    }
```

**Step 2: Run test to verify it fails**

Run: `cargo test --bin animus`
Expected: FAIL — `render_prompt` not defined

**Step 3: Implement**

```rust
/// Render the user prompt from either inline text or a Tera template file.
fn render_prompt(
    prompt: Option<&str>,
    prompt_file: Option<&PathBuf>,
    vars: &[(String, String)],
) -> anyhow::Result<String> {
    match (prompt, prompt_file) {
        (Some(text), None) => Ok(text.to_string()),
        (None, Some(path)) => {
            let template_str = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("failed to read prompt file {}: {e}", path.display()))?;

            let mut tera = tera::Tera::default();
            tera.add_raw_template("prompt", &template_str)
                .map_err(|e| anyhow::anyhow!("template parse error: {e}"))?;

            let mut context = tera::Context::new();
            for (key, value) in vars {
                context.insert(key, value);
            }

            tera.render("prompt", &context)
                .map_err(|e| anyhow::anyhow!("template render error: {e}"))
        }
        (Some(_), Some(_)) => anyhow::bail!("specify either --prompt or --prompt-file, not both"),
        (None, None) => anyhow::bail!("specify --prompt or --prompt-file"),
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --bin animus`
Expected: PASS (8 tests)

**Step 5: Commit**

```bash
git add src/bin/animus.rs
git commit -m "feat(cli): implement Tera template rendering for prompt files"
```

---

### Task 5: Wire Up cmd_llm_complete (Non-Streaming)

**Files:**
- Modify: `src/bin/animus.rs`

**Step 1: Replace the stub `cmd_llm_complete` with the real implementation**

```rust
#[allow(clippy::too_many_arguments)]
async fn cmd_llm_complete(
    prompt: Option<String>,
    prompt_file: Option<PathBuf>,
    vars: Vec<(String, String)>,
    context_files: Vec<PathBuf>,
    system: Option<String>,
    model_override: Option<String>,
    format: String,
    stream: bool,
    max_tokens_override: Option<u32>,
) -> anyhow::Result<()> {
    let config = Config::from_env()?;
    let llm_config = config
        .llm
        .ok_or_else(|| anyhow::anyhow!("LLM not configured. Set LLM_PROVIDER, LLM_API_KEY, and LLM_MODEL environment variables."))?;

    let client = animus_rs::llm::create_client(&llm_config)
        .map_err(|e| anyhow::anyhow!("failed to create LLM client: {e}"))?;

    // Read context files
    let mut file_contents = Vec::new();
    for path in &context_files {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read context file {}: {e}", path.display()))?;
        file_contents.push((path.display().to_string(), content));
    }

    // Read stdin if not a TTY
    let stdin_content = if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
        if buf.is_empty() { None } else { Some(buf) }
    } else {
        None
    };

    // Assemble system prompt
    let system_prompt = build_system_prompt(
        system.as_deref(),
        &file_contents,
        stdin_content,
    );

    // Render user prompt
    let user_prompt = render_prompt(
        prompt.as_deref(),
        prompt_file.as_ref(),
        &vars,
    )?;

    let model = model_override.unwrap_or(llm_config.model);
    let max_tokens = max_tokens_override.unwrap_or(llm_config.max_tokens);

    let request = animus_rs::llm::CompletionRequest {
        model,
        system: system_prompt,
        messages: vec![animus_rs::llm::Message::User {
            content: vec![animus_rs::llm::UserContent::Text {
                text: user_prompt,
            }],
        }],
        tools: vec![],
        max_tokens,
        temperature: None,
    };

    if stream {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let response_handle = tokio::spawn({
            let client = client;
            let request = request.clone();
            async move { client.complete_stream(&request, &tx).await }
        });

        // Print tokens as they arrive
        while let Some(event) = rx.recv().await {
            match event {
                animus_rs::llm::StreamEvent::TextDelta { text } => {
                    use std::io::Write;
                    print!("{text}");
                    std::io::stdout().flush().ok();
                }
                animus_rs::llm::StreamEvent::Done => break,
                _ => {}
            }
        }
        println!(); // trailing newline

        let response = response_handle
            .await?
            .map_err(|e| anyhow::anyhow!("LLM error: {e}"))?;

        if format == "json" {
            print_json_output(&response)?;
        } else if format == "yaml" {
            print_yaml_output(&response)?;
        }
        // text format already printed via streaming
    } else {
        let response = client
            .complete(&request)
            .await
            .map_err(|e| anyhow::anyhow!("LLM error: {e}"))?;

        match format.as_str() {
            "json" => print_json_output(&response)?,
            "yaml" => print_yaml_output(&response)?,
            _ => {
                // text format — print content blocks
                for block in &response.content {
                    if let animus_rs::llm::ContentBlock::Text { text } = block {
                        println!("{text}");
                    }
                }
            }
        }
    }

    Ok(())
}

fn print_json_output(response: &animus_rs::llm::CompletionResponse) -> anyhow::Result<()> {
    let text: String = response
        .content
        .iter()
        .filter_map(|b| match b {
            animus_rs::llm::ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    let output = serde_json::json!({
        "text": text,
        "usage": {
            "input": response.usage.input_tokens,
            "output": response.usage.output_tokens,
        }
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn print_yaml_output(response: &animus_rs::llm::CompletionResponse) -> anyhow::Result<()> {
    let text: String = response
        .content
        .iter()
        .filter_map(|b| match b {
            animus_rs::llm::ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    // Simple YAML output without pulling in a YAML crate
    println!("text: |");
    for line in text.lines() {
        println!("  {line}");
    }
    println!("usage:");
    println!("  input: {}", response.usage.input_tokens);
    println!("  output: {}", response.usage.output_tokens);
    Ok(())
}
```

Note: `CompletionRequest` needs `Clone`. Add `#[derive(Debug, Clone)]` to it in `src/llm/types.rs` (it already has `Debug, Clone`  — verify).

**Step 2: Add required imports at top of animus.rs**

Make sure these are imported:

```rust
use animus_rs::llm;
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles. If `CompletionRequest` doesn't impl `Clone`, fix in types.rs.

**Step 4: Run all tests**

Run: `cargo test`
Expected: all tests pass (including the 8 bin tests from Tasks 3-4)

**Step 5: Commit**

```bash
git add src/bin/animus.rs
git commit -m "feat(cli): implement animus llm complete with streaming + format options"
```

---

### Task 6: Manual Smoke Test

**Files:** none (manual verification)

**Step 1: Verify help output**

Run: `cargo run -- llm complete --help`
Expected: all flags shown

**Step 2: Test with inline prompt (requires LLM credentials)**

```bash
export LLM_PROVIDER=xai
export LLM_API_KEY=<your-key>
export LLM_MODEL=grok-3-latest

cargo run -- llm complete --prompt "What is 2+2? Answer in one word."
```

Expected: prints the response text

**Step 3: Test with context file**

```bash
cargo run -- llm complete \
  --context-file docs/llm.md \
  --prompt "Summarize this document in 3 bullet points." \
  --max-tokens 256
```

**Step 4: Test with streaming**

```bash
cargo run -- llm complete \
  --prompt "Count from 1 to 10, one number per line." \
  --stream
```

Expected: numbers appear one at a time

**Step 5: Test with stdin**

```bash
cat src/llm/mod.rs | cargo run -- llm complete \
  --prompt "What does this Rust module do? One sentence."
```

**Step 6: Test with template**

Create a temp template:

```bash
echo 'Explain {{ topic }} in {{ style }} style.' > /tmp/test_prompt.md
cargo run -- llm complete \
  --prompt-file /tmp/test_prompt.md \
  --var topic="monads" \
  --var style="pirate"
```

**Step 7: Test JSON format**

```bash
cargo run -- llm complete \
  --prompt "Say hello" \
  --format json
```

Expected: `{"text": "...", "usage": {"input": N, "output": N}}`

**Step 8: Commit (no code changes, just verification)**

No commit needed — this is manual verification only.

---

### Task 7: Update docs/cli.md

**Files:**
- Modify: `docs/cli.md:153-176`

**Step 1: Update the `animus llm complete` section**

Replace the current section (lines 153-176) with the updated interface that includes all new flags:

```markdown
### `animus llm complete`

Run an LLM completion. Used by hook scripts and interactively.

```
animus llm complete [OPTIONS]
```

| Flag | Default | Description |
|---|---|---|
| `--prompt TEXT` | - | Inline user prompt (mutually exclusive with --prompt-file) |
| `--prompt-file PATH` | - | Tera template file as user prompt |
| `--var KEY=VALUE` | - | Template variable (repeatable) |
| `--context-file PATH` | - | File to include in system context (repeatable, ordered) |
| `--system TEXT` | - | Explicit system prompt (prepended before context files) |
| `--model MODEL` | from config | Override LLM_MODEL |
| `--format FORMAT` | `text` | Output format (text, json, yaml) |
| `--stream` | false | Stream tokens to stdout as they arrive |
| `--max-tokens N` | from config | Override LLM_MAX_TOKENS |

Stdin: if not a TTY, read and include as context after `--context-file` contents.

```sh
# Inline prompt with context files
animus llm complete \
  --context-file docs/llm.md \
  --prompt "Summarize this document"

# Template with variables
animus llm complete \
  --prompt-file skills/engage/prompt/classify-inbound.md \
  --var person="kelly" \
  --var message="hey cookie!" \
  --format yaml

# Pipe stdin + streaming
cat src/llm/openai.rs | animus llm complete \
  --prompt "Review this code" \
  --stream
```
```

**Step 2: Update "What to Build Now" section**

Change item 3 from "needed for orient/consolidate" to "implemented".

**Step 3: Commit**

```bash
git add docs/cli.md
git commit -m "docs: update cli.md with implemented animus llm complete interface"
```

---

## Summary

| Task | What | Tests |
|------|------|-------|
| 1 | Add tera dependency | cargo build |
| 2 | Clap subcommand structure (stub) | cargo build + --help |
| 3 | System prompt assembly | 5 unit tests |
| 4 | Tera template rendering | 3 unit tests |
| 5 | Full cmd_llm_complete wiring | cargo build + existing tests |
| 6 | Manual smoke test | 7 manual verifications |
| 7 | Docs update | — |

Total: ~8 unit tests, 7 tasks, 7 commits.
