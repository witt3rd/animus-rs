//! animus CLI — operator interface to the animus appliance.

use animus_rs::config::Config;
use animus_rs::db::Db;
use animus_rs::engine::{ControlConfig, ControlPlane};
use animus_rs::faculty::FacultyRegistry;
use animus_rs::model::work::{NewWorkItem, State};
use animus_rs::telemetry::{TelemetryConfig, init_telemetry};
use clap::{Parser, Subcommand};
use secrecy::ExposeSecret;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "animus", about = "Substrate for relational beings")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the control plane daemon
    Serve {
        /// Directory containing faculty TOML configs
        #[arg(long, default_value = "faculties")]
        faculties: PathBuf,
        /// Global maximum concurrent foci
        #[arg(long, default_value_t = 4)]
        max_concurrent: usize,
    },
    /// Work item operations
    Work {
        #[command(subcommand)]
        action: WorkAction,
    },
    /// LLM sub-commands
    Llm {
        #[command(subcommand)]
        action: LlmAction,
    },
}

#[derive(Subcommand)]
enum WorkAction {
    /// Submit a new work item
    Submit {
        /// Which faculty handles this work
        faculty: String,
        /// Provenance source
        source: String,
        /// Skill that drives the methodology
        #[arg(long)]
        skill: Option<String>,
        /// Structural dedup key
        #[arg(long)]
        dedup_key: Option<String>,
        /// Provenance trigger info
        #[arg(long)]
        trigger: Option<String>,
        /// JSON parameters
        #[arg(long)]
        params: Option<String>,
        /// Priority (higher = more urgent)
        #[arg(long, default_value_t = 0)]
        priority: i32,
    },
    /// List work items
    List {
        /// Filter by state
        #[arg(long)]
        state: Option<String>,
        /// Filter by faculty
        #[arg(long)]
        faculty: Option<String>,
        /// Maximum items to show
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    /// Show a work item
    Show {
        /// Work item ID (full UUID or prefix)
        id: String,
    },
}

#[derive(Subcommand)]
enum LlmAction {
    /// Run an LLM completion
    Complete {
        /// Inline prompt text (mutually exclusive with --prompt-file)
        #[arg(long, group = "prompt_source")]
        prompt: Option<String>,
        /// Path to a Tera template file (mutually exclusive with --prompt)
        #[arg(long, group = "prompt_source")]
        prompt_file: Option<PathBuf>,
        /// Template variable (repeatable): KEY=VALUE
        #[arg(long = "var", value_parser = parse_key_val)]
        vars: Vec<(String, String)>,
        /// Context file to include in system prompt (repeatable)
        #[arg(long = "context-file")]
        context_files: Vec<PathBuf>,
        /// Explicit system prompt text
        #[arg(long)]
        system: Option<String>,
        /// Override model from config
        #[arg(long)]
        model: Option<String>,
        /// Output format: text, json, yaml
        #[arg(long, default_value = "text")]
        format: String,
        /// Stream output tokens as they arrive
        #[arg(long)]
        stream: bool,
        /// Override max tokens from config
        #[arg(long)]
        max_tokens: Option<u32>,
    },
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=VALUE: no '=' found in '{s}'"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Init tracing subscriber from RUST_LOG (default: warn).
    // `serve` overrides this with OTel; other commands use this basic stderr logger.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Serve {
            faculties,
            max_concurrent,
        } => cmd_serve(faculties, max_concurrent).await,
        Command::Work { action } => {
            let config = Config::from_env()?;
            let db = Db::connect(config.database_url.expose_secret()).await?;
            db.migrate().await?;
            db.create_queue("work").await?;

            match action {
                WorkAction::Submit {
                    faculty,
                    source,
                    skill,
                    dedup_key,
                    trigger,
                    params,
                    priority,
                } => {
                    cmd_work_submit(
                        &db, faculty, source, skill, dedup_key, trigger, params, priority,
                    )
                    .await
                }
                WorkAction::List {
                    state,
                    faculty,
                    limit,
                } => cmd_work_list(&db, state, faculty, limit).await,
                WorkAction::Show { id } => cmd_work_show(&db, id).await,
            }
        }
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
    }
}

async fn cmd_serve(faculties: PathBuf, max_concurrent: usize) -> anyhow::Result<()> {
    let config = Config::from_env()?;

    let _guard = init_telemetry(TelemetryConfig {
        endpoint: config.otel_endpoint.clone(),
        service_name: "animus".to_string(),
    })?;

    let db = Db::connect(config.database_url.expose_secret()).await?;
    db.migrate().await?;
    db.create_queue("work").await?;

    let registry = FacultyRegistry::load_from_dir(&faculties)?;

    let control = ControlPlane::new(
        Arc::new(db),
        Arc::new(registry),
        ControlConfig::default(),
        max_concurrent,
    );

    let ctrl = control.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        ctrl.shutdown();
    });

    control.run().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_work_submit(
    db: &Db,
    faculty: String,
    source: String,
    skill: Option<String>,
    dedup_key: Option<String>,
    trigger: Option<String>,
    params: Option<String>,
    priority: i32,
) -> anyhow::Result<()> {
    let params: serde_json::Value = match params {
        Some(json) => serde_json::from_str(&json)?,
        None => serde_json::json!({}),
    };

    let mut new = NewWorkItem::new(&faculty, &source)
        .params(params)
        .priority(priority);

    if let Some(ref s) = skill {
        new = new.skill(s);
    }
    if let Some(ref key) = dedup_key {
        new = new.dedup_key(key);
    }
    if let Some(ref trig) = trigger {
        new = new.trigger(trig);
    }

    let result = db.submit_work(new).await?;

    match result {
        animus_rs::db::work::SubmitResult::Created(item) => {
            println!("Created: {} (state: {})", item.id, item.state);
        }
        animus_rs::db::work::SubmitResult::Merged {
            new_id,
            canonical_id,
        } => {
            println!("Merged: {new_id} → canonical {canonical_id}");
        }
    }

    Ok(())
}

async fn cmd_work_list(
    db: &Db,
    state: Option<String>,
    faculty: Option<String>,
    limit: i64,
) -> anyhow::Result<()> {
    let state_filter: Option<State> = match state {
        Some(s) => Some(
            s.parse()
                .map_err(|_| anyhow::anyhow!("invalid state: {s}"))?,
        ),
        None => None,
    };

    let items = db
        .list_work_items(state_filter, faculty.as_deref(), limit)
        .await?;

    if items.is_empty() {
        println!("No work items found.");
        return Ok(());
    }

    // Header
    println!(
        "{:<8}  {:<12}  {:<20}  {:<10}  {:<4}  CREATED",
        "ID", "FACULTY", "SKILL", "STATE", "PRI"
    );
    println!("{}", "-".repeat(90));

    for item in &items {
        let short_id = &item.id.to_string()[..8];
        let skill = item.skill.as_deref().unwrap_or("-");
        let skill_display = if skill.len() > 20 {
            &skill[..20]
        } else {
            skill
        };
        println!(
            "{:<8}  {:<12}  {:<20}  {:<10}  {:<4}  {}",
            short_id,
            item.faculty,
            skill_display,
            item.state,
            item.priority,
            item.created_at.format("%Y-%m-%d %H:%M")
        );
    }

    println!("\n{} item(s)", items.len());
    Ok(())
}

async fn cmd_work_show(db: &Db, id_str: String) -> anyhow::Result<()> {
    // Support prefix matching — find the work item whose ID starts with the given string
    let id = if id_str.len() < 36 {
        // Prefix search
        let items = db.list_work_items(None, None, 100).await?;
        let matches: Vec<_> = items
            .iter()
            .filter(|item| item.id.to_string().starts_with(&id_str))
            .collect();
        match matches.len() {
            0 => anyhow::bail!("no work item matching prefix '{id_str}'"),
            1 => matches[0].id,
            n => anyhow::bail!("{n} work items match prefix '{id_str}' — be more specific"),
        }
    } else {
        let uuid = uuid::Uuid::parse_str(&id_str)?;
        animus_rs::model::work::WorkId(uuid)
    };

    let item = db.get_work_item(id).await?;

    println!("ID:         {}", item.id);
    println!("Faculty:    {}", item.faculty);
    println!("Skill:      {}", item.skill.as_deref().unwrap_or("-"));
    println!("State:      {}", item.state);
    println!("Priority:   {}", item.priority);
    println!("Dedup Key:  {}", item.dedup_key.as_deref().unwrap_or("-"));
    println!("Source:     {}", item.provenance.source);
    println!(
        "Trigger:    {}",
        item.provenance.trigger.as_deref().unwrap_or("-")
    );
    println!(
        "Params:     {}",
        serde_json::to_string_pretty(&item.params)?
    );
    println!("Attempts:   {}", item.attempts);
    println!(
        "Max Tries:  {}",
        item.max_attempts
            .map(|n| n.to_string())
            .unwrap_or("-".to_string())
    );
    println!("Created:    {}", item.created_at);
    println!("Updated:    {}", item.updated_at);
    if let Some(resolved) = item.resolved_at {
        println!("Resolved:   {resolved}");
    }
    if let Some(parent) = item.parent_id {
        println!("Parent:     {parent}");
    }
    if let Some(merged) = item.merged_into {
        println!("Merged Into: {merged}");
    }
    if let Some(ref outcome) = item.outcome {
        println!("---");
        println!(
            "Outcome:    {}",
            if outcome.success {
                "success"
            } else {
                "failure"
            }
        );
        if let Some(ref data) = outcome.data {
            println!("Data:       {}", serde_json::to_string_pretty(data)?);
        }
        if let Some(ref err) = outcome.error {
            println!("Error:      {err}");
        }
        println!("Duration:   {}ms", outcome.duration_ms);
    }

    Ok(())
}

/// Assemble a system prompt from explicit text, context files, and stdin.
///
/// Order: system text, then files (each wrapped in markers), then stdin.
fn build_system_prompt(
    system: Option<&str>,
    context_files: &[(String, String)],
    stdin: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(text) = system {
        parts.push(text.to_string());
    }

    for (path, content) in context_files {
        parts.push(format!("--- FILE: {path} ---\n{content}\n--- END FILE ---"));
    }

    if let Some(input) = stdin {
        parts.push(format!("--- STDIN ---\n{input}\n--- END STDIN ---"));
    }

    parts.join("\n\n")
}

/// Render the user prompt from inline text or a Tera template file with variable substitutions.
fn render_prompt(
    prompt: Option<&str>,
    prompt_file: Option<&std::path::Path>,
    vars: &[(String, String)],
) -> anyhow::Result<String> {
    match (prompt, prompt_file) {
        (Some(text), None) => {
            // Even inline prompts may contain Tera expressions
            let mut tera = tera::Tera::default();
            tera.add_raw_template("inline", text)?;
            let mut ctx = tera::Context::new();
            for (k, v) in vars {
                ctx.insert(k.as_str(), v);
            }
            Ok(tera.render("inline", &ctx)?)
        }
        (None, Some(path)) => {
            let template = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("cannot read prompt file {}: {e}", path.display()))?;
            let mut tera = tera::Tera::default();
            tera.add_raw_template("file", &template)?;
            let mut ctx = tera::Context::new();
            for (k, v) in vars {
                ctx.insert(k.as_str(), v);
            }
            Ok(tera.render("file", &ctx)?)
        }
        (None, None) => anyhow::bail!("one of --prompt or --prompt-file is required"),
        (Some(_), Some(_)) => anyhow::bail!("--prompt and --prompt-file are mutually exclusive"),
    }
}

/// Extract text content from a completion response.
fn response_text(response: &animus_rs::llm::CompletionResponse) -> String {
    response
        .content
        .iter()
        .filter_map(|block| match block {
            animus_rs::llm::ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Extract thinking/reasoning content from a completion response.
fn response_thinking(response: &animus_rs::llm::CompletionResponse) -> String {
    response
        .content
        .iter()
        .filter_map(|block| match block {
            animus_rs::llm::ContentBlock::Thinking { thinking } => Some(thinking.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Format output according to the requested format.
fn format_output(
    text: &str,
    thinking: &str,
    usage: &animus_rs::llm::Usage,
    format: &str,
) -> anyhow::Result<String> {
    match format {
        "text" => Ok(text.to_string()),
        "json" => {
            let mut obj = serde_json::json!({
                "text": text,
                "usage": {
                    "input": usage.input_tokens,
                    "output": usage.output_tokens,
                }
            });
            if !thinking.is_empty() {
                obj["thinking"] = serde_json::json!(thinking);
            }
            Ok(serde_json::to_string_pretty(&obj)?)
        }
        "yaml" => {
            // Hand-formatted YAML — no extra dependency
            let mut out = String::new();
            if !thinking.is_empty() {
                let indented: String = thinking
                    .lines()
                    .map(|line| format!("  {line}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                out.push_str(&format!("thinking: |\n{indented}\n"));
            }
            let indented: String = text
                .lines()
                .map(|line| format!("  {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            out.push_str(&format!(
                "text: |\n{indented}\nusage:\n  input: {}\n  output: {}",
                usage.input_tokens, usage.output_tokens,
            ));
            Ok(out)
        }
        other => anyhow::bail!("unknown format '{other}' — expected text, json, or yaml"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn cmd_llm_complete(
    prompt: Option<String>,
    prompt_file: Option<PathBuf>,
    vars: Vec<(String, String)>,
    context_files: Vec<PathBuf>,
    system: Option<String>,
    model: Option<String>,
    format: String,
    stream: bool,
    max_tokens: Option<u32>,
) -> anyhow::Result<()> {
    // Load LLM config from env (does not require DATABASE_URL)
    let provider = std::env::var("LLM_PROVIDER")
        .map_err(|_| anyhow::anyhow!("LLM_PROVIDER is not set — configure LLM env vars"))?;
    let api_key = secrecy::SecretString::from(
        std::env::var("LLM_API_KEY").map_err(|_| anyhow::anyhow!("LLM_API_KEY is not set"))?,
    );
    let config_model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());
    let config_max_tokens: u32 = std::env::var("LLM_MAX_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8192);
    let max_retries: u32 = std::env::var("LLM_MAX_RETRIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    let llm_config = animus_rs::llm::LlmConfig {
        provider,
        api_key,
        base_url: std::env::var("LLM_BASE_URL").ok(),
        model: model.unwrap_or(config_model),
        max_tokens: max_tokens.unwrap_or(config_max_tokens),
        max_retries,
    };

    let client = animus_rs::llm::create_client(&llm_config)?;

    // Read context files
    let mut file_contents: Vec<(String, String)> = Vec::new();
    for path in &context_files {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("cannot read context file {}: {e}", path.display()))?;
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

    // Build system prompt
    let system_prompt =
        build_system_prompt(system.as_deref(), &file_contents, stdin_content.as_deref());

    // Render user prompt
    let user_text = render_prompt(prompt.as_deref(), prompt_file.as_deref(), &vars)?;

    // Build completion request
    let request = animus_rs::llm::CompletionRequest {
        model: llm_config.model.clone(),
        system: system_prompt,
        messages: vec![animus_rs::llm::Message::User {
            content: vec![animus_rs::llm::UserContent::Text { text: user_text }],
        }],
        tools: vec![],
        max_tokens: llm_config.max_tokens,
        temperature: None,
    };

    if stream {
        // Streaming path
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let stream_client = client;
        let stream_request = request.clone();
        let handle =
            tokio::spawn(async move { stream_client.complete_stream(&stream_request, &tx).await });

        // Print deltas as they arrive
        let mut full_text = String::new();
        let mut full_thinking = String::new();
        let mut in_thinking = false;
        while let Some(event) = rx.recv().await {
            match event {
                animus_rs::llm::StreamEvent::ThinkingDelta { text } => {
                    full_thinking.push_str(&text);
                    if format == "text" {
                        use std::io::Write;
                        if !in_thinking {
                            in_thinking = true;
                            eprint!("\x1b[2m"); // dim
                        }
                        eprint!("{text}");
                        std::io::stderr().flush().ok();
                    }
                }
                animus_rs::llm::StreamEvent::TextDelta { text } => {
                    full_text.push_str(&text);
                    if format == "text" {
                        use std::io::Write;
                        if in_thinking {
                            in_thinking = false;
                            eprintln!("\x1b[0m"); // reset, newline
                        }
                        print!("{text}");
                        std::io::stdout().flush().ok();
                    }
                }
                animus_rs::llm::StreamEvent::Done => break,
                _ => {}
            }
        }
        if in_thinking {
            eprint!("\x1b[0m");
        }

        let response = handle.await??;

        if format == "text" {
            // We already printed the text, just add a trailing newline
            println!();
        } else {
            let output = format_output(&full_text, &full_thinking, &response.usage, &format)?;
            println!("{output}");
        }
    } else {
        // Non-streaming path
        let response = client.complete(&request).await?;
        let thinking = response_thinking(&response);
        if !thinking.is_empty() && format == "text" {
            // Print thinking dimmed to stderr
            eprintln!("\x1b[2m{thinking}\x1b[0m");
        }
        let text = response_text(&response);
        let output = format_output(&text, &thinking, &response.usage, &format)?;
        println!("{output}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── build_system_prompt ──────────────────────────────────────────

    #[test]
    fn build_system_prompt_empty_inputs() {
        let result = build_system_prompt(None, &[], None);
        assert_eq!(result, "");
    }

    #[test]
    fn build_system_prompt_system_only() {
        let result = build_system_prompt(Some("You are a helpful assistant."), &[], None);
        assert_eq!(result, "You are a helpful assistant.");
    }

    #[test]
    fn build_system_prompt_with_context_files() {
        let files = vec![
            ("README.md".to_string(), "# Hello".to_string()),
            ("notes.txt".to_string(), "some notes".to_string()),
        ];
        let result = build_system_prompt(None, &files, None);
        assert!(result.contains("--- FILE: README.md ---"));
        assert!(result.contains("# Hello"));
        assert!(result.contains("--- END FILE ---"));
        assert!(result.contains("--- FILE: notes.txt ---"));
        assert!(result.contains("some notes"));
    }

    #[test]
    fn build_system_prompt_with_stdin() {
        let result = build_system_prompt(None, &[], Some("piped input"));
        assert!(result.contains("--- STDIN ---"));
        assert!(result.contains("piped input"));
        assert!(result.contains("--- END STDIN ---"));
    }

    #[test]
    fn build_system_prompt_ordering() {
        let files = vec![("f.txt".to_string(), "file content".to_string())];
        let result = build_system_prompt(Some("system text"), &files, Some("stdin text"));

        let sys_pos = result.find("system text").unwrap();
        let file_pos = result.find("--- FILE: f.txt ---").unwrap();
        let stdin_pos = result.find("--- STDIN ---").unwrap();

        assert!(sys_pos < file_pos, "system should come before files");
        assert!(file_pos < stdin_pos, "files should come before stdin");
    }

    // ── render_prompt ────────────────────────────────────────────────

    #[test]
    fn render_prompt_inline() {
        let result = render_prompt(Some("Hello, world!"), None, &[]).unwrap();
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn render_prompt_inline_with_vars() {
        let vars = vec![("name".to_string(), "Alice".to_string())];
        let result = render_prompt(Some("Hello, {{ name }}!"), None, &vars).unwrap();
        assert_eq!(result, "Hello, Alice!");
    }

    #[test]
    fn render_prompt_file() {
        let dir = std::env::temp_dir().join("animus_test_render_prompt");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("prompt.txt");
        std::fs::write(&file, "Summarize {{ topic }} in {{ style }} style.").unwrap();

        let vars = vec![
            ("topic".to_string(), "Rust".to_string()),
            ("style".to_string(), "concise".to_string()),
        ];
        let result = render_prompt(None, Some(&file), &vars).unwrap();
        assert_eq!(result, "Summarize Rust in concise style.");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn render_prompt_neither_provided() {
        let result = render_prompt(None, None, &[]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("--prompt or --prompt-file")
        );
    }

    #[test]
    fn render_prompt_both_provided() {
        let result = render_prompt(Some("inline"), Some(std::path::Path::new("dummy.txt")), &[]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("mutually exclusive")
        );
    }

    // ── format_output ─────────────────────────────────────────────────

    #[test]
    fn format_output_text() {
        let usage = animus_rs::llm::Usage {
            input_tokens: 10,
            output_tokens: 20,
        };
        let result = format_output("hello world", "", &usage, "text").unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn format_output_json() {
        let usage = animus_rs::llm::Usage {
            input_tokens: 10,
            output_tokens: 20,
        };
        let result = format_output("hello", "", &usage, "json").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["text"], "hello");
        assert_eq!(parsed["usage"]["input"], 10);
        assert_eq!(parsed["usage"]["output"], 20);
        assert!(parsed.get("thinking").is_none());
    }

    #[test]
    fn format_output_json_with_thinking() {
        let usage = animus_rs::llm::Usage {
            input_tokens: 10,
            output_tokens: 20,
        };
        let result = format_output("answer", "let me think...", &usage, "json").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["text"], "answer");
        assert_eq!(parsed["thinking"], "let me think...");
    }

    #[test]
    fn format_output_yaml() {
        let usage = animus_rs::llm::Usage {
            input_tokens: 10,
            output_tokens: 20,
        };
        let result = format_output("hello\nworld", "", &usage, "yaml").unwrap();
        assert!(result.contains("text: |"));
        assert!(result.contains("  hello"));
        assert!(result.contains("  world"));
        assert!(result.contains("input: 10"));
        assert!(result.contains("output: 20"));
        assert!(!result.contains("thinking"));
    }

    #[test]
    fn format_output_unknown() {
        let usage = animus_rs::llm::Usage {
            input_tokens: 0,
            output_tokens: 0,
        };
        let result = format_output("x", "", &usage, "xml");
        assert!(result.is_err());
    }

    // ── response_text ───────────────────────────────────────────────

    #[test]
    fn response_text_extracts_text_blocks() {
        let response = animus_rs::llm::CompletionResponse {
            content: vec![
                animus_rs::llm::ContentBlock::Text {
                    text: "hello ".into(),
                },
                animus_rs::llm::ContentBlock::Text {
                    text: "world".into(),
                },
            ],
            stop_reason: animus_rs::llm::StopReason::EndTurn,
            usage: animus_rs::llm::Usage::default(),
        };
        assert_eq!(response_text(&response), "hello world");
    }

    #[test]
    fn response_text_skips_tool_use() {
        let response = animus_rs::llm::CompletionResponse {
            content: vec![
                animus_rs::llm::ContentBlock::Text {
                    text: "result".into(),
                },
                animus_rs::llm::ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "search".into(),
                    input: serde_json::json!({}),
                },
            ],
            stop_reason: animus_rs::llm::StopReason::EndTurn,
            usage: animus_rs::llm::Usage::default(),
        };
        assert_eq!(response_text(&response), "result");
    }

    // ── parse_key_val ────────────────────────────────────────────────

    #[test]
    fn parse_key_val_valid() {
        let (k, v) = parse_key_val("name=Alice").unwrap();
        assert_eq!(k, "name");
        assert_eq!(v, "Alice");
    }

    #[test]
    fn parse_key_val_value_with_equals() {
        let (k, v) = parse_key_val("expr=a=b").unwrap();
        assert_eq!(k, "expr");
        assert_eq!(v, "a=b");
    }

    #[test]
    fn parse_key_val_no_equals() {
        let result = parse_key_val("noequals");
        assert!(result.is_err());
    }
}
