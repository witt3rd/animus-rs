# `animus llm complete` CLI Command

*Date: 2026-03-02*
*Status: approved*
*Spec: docs/cli.md § animus llm complete*

## Purpose

CLI command for issuing LLM prompts with file context, template variables, and streaming output. Used interactively and by hook scripts (orient, consolidate).

## Command Interface

```
animus llm complete [OPTIONS]
```

| Flag | Required | Description |
|---|---|---|
| `--prompt TEXT` | one of prompt/prompt-file | Inline user prompt |
| `--prompt-file PATH` | one of prompt/prompt-file | Tera template file as user prompt |
| `--var KEY=VALUE` | no | Template variable (repeatable) |
| `--context-file PATH` | no | File to include in system context (repeatable, ordered) |
| `--system TEXT` | no | Explicit system prompt (prepended before context files) |
| `--model MODEL` | no | Override LLM_MODEL from config |
| `--format FORMAT` | no | Output format: `text` (default), `json`, `yaml` |
| `--stream` | no | Stream tokens to stdout as they arrive |
| `--max-tokens N` | no | Override LLM_MAX_TOKENS |

Stdin: if stdin is not a TTY, read it and insert as context after `--context-file` contents.

## Message Assembly

```
System prompt:
  [--system text, if provided]
  [--context-file 1 contents, wrapped]
  [--context-file 2 contents, wrapped]
  [stdin contents, if piped]

User message:
  [--prompt text OR --prompt-file rendered through Tera with --var substitutions]
```

Context files wrapped with headers:

```
--- FILE: docs/llm.md ---
<file contents>
--- END FILE ---
```

## Template Engine

Tera (Jinja2-style). Pure Rust, supports conditionals, loops, filters.

```toml
tera = { version = "1", default-features = false }
```

Template example (`skills/engage/prompt/classify-inbound.md`):

```
Classify this inbound message from {{ person }}:

{{ message }}

Respond in {{ format }} format.
```

## Output

- **Default**: wait for complete response, print text to stdout
- **`--stream`**: tokens printed to stdout as they arrive via `complete_stream()`
- **`--format json`**: `{"text": "...", "usage": {"input": N, "output": N}}`
- **`--format yaml`**: same structure, YAML format

## Error Handling

- Missing LLM config → clear error pointing to `.env` or `LLM_PROVIDER`/`LLM_API_KEY`/`LLM_MODEL`
- Template render failure → Tera error with file path
- Both `--prompt` and `--prompt-file` → error, pick one
- Neither `--prompt` nor `--prompt-file` → error, need one
- API error → status code and message from `LlmError`

## Dependencies

```toml
tera = { version = "1", default-features = false }
```

No other new dependencies — uses the LLM client already in place.

## Implementation

All in `src/bin/animus.rs`:

1. Add `Llm` variant to `Command` enum with `LlmAction::Complete` subcommand
2. Add `cmd_llm_complete()` function:
   - Load config, require `config.llm`
   - Build system prompt from `--system` + context files + stdin
   - Build user prompt from `--prompt` or render `--prompt-file` with Tera + `--var`s
   - Create `CompletionRequest`
   - Call `complete()` or `complete_stream()` depending on `--stream`
   - Format output per `--format`
