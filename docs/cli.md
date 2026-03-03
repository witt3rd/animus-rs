---
status: active
milestone: kernel
spec: null
code: src/bin/animus.rs
---

# CLI Design

*The operator interface to the animus appliance.*

## Principle

The CLI is how you interact with a running animus instance. It connects to the same Postgres database as the control plane daemon and operates on the same tables. No separate API server — the CLI talks directly to the database.

This means the CLI works whether the daemon is running or not. You can submit work, inspect state, and read the ledger even when the control plane is down. The database is the source of truth, not the daemon.

The CLI also serves as the building block for skill hook scripts. Orient and consolidate hooks call CLI commands to perform sub-LLM calls, memory search, ledger operations, and work submission. See `skills/engage/scripts/` for the reference implementation.

## Commands

### `animus serve`

Run the control plane daemon. Watches queues, discovers skills, spawns foci.

```
animus serve [--skills DIR] [--max-concurrent N]
```

| Flag | Default | Description |
|---|---|---|
| `--skills` | `./skills` | Directory containing skill packages (SKILL.md) |
| `--max-concurrent` | `4` | Global maximum concurrent foci |

### `animus work submit`

Submit a work item to the queue.

```
animus work submit <skill> <source> [OPTIONS]
```

| Argument / Flag | Required | Description |
|---|---|---|
| `<skill>` | yes | Which skill handles this work (must match a SKILL.md name) |
| `<source>` | yes | Provenance source (e.g., "bootstrap", "heartbeat", "user") |
| `--dedup-key` | no | Structural dedup key |
| `--trigger` | no | Provenance trigger info |
| `--params` | no | JSON object with work parameters |
| `--priority` | no | Priority (default: 0, higher = more urgent) |

```sh
# Submit a work item using the tdd-implementation skill
animus work submit tdd-implementation bootstrap \
  --dedup-key "milestone=M4-work-ledger" \
  --trigger "PLAN.md" \
  --priority 10 \
  --params '{"milestone": "M4", "title": "Work Ledger", "spec": "docs/ledger.md"}'

# Submit a social interaction
animus work submit engage user \
  --params '{"person": "kelly", "text": "hey cookie!"}'
```

Output: the work item ID and whether it was created or merged.

### `animus work list`

List work items.

```
animus work list [OPTIONS]
```

| Flag | Default | Description |
|---|---|---|
| `--state` | all | Filter by state (queued, running, completed, failed, dead, merged) |
| `--skill` | all | Filter by skill |
| `--limit` | 20 | Max items to show |
| `--parent` | none | Show children of a specific work item |

```sh
animus work list
animus work list --state queued
animus work list --skill tdd-implementation
```

Output: table with id (short), skill, state, priority, created_at.

### `animus work show`

Show full details of a work item.

```
animus work show <id>
```

Shows: all fields, provenance, outcome (if terminal), parent/child links, and ledger entries (once the ledger exists). Supports `--json` flag for machine-readable output (used by hook scripts).

### `animus ledger show`

Show ledger entries for a work item.

```
animus ledger show <work_item_id> [OPTIONS]
```

| Flag | Default | Description |
|---|---|---|
| `--type` | all | Filter by entry type (plan, finding, decision, step, error, note) |
| `--last` | all | Show only the last N entries |
| `--formatted` | false | Grouped by type (the compaction format) |

```sh
animus ledger show abc123
animus ledger show abc123 --type finding
animus ledger show abc123 --formatted
```

*Available after M4 (work ledger) is implemented.*

### `animus ledger append`

Manually append a ledger entry. Useful during bootstrap when the engage loop doesn't exist yet.

```
animus ledger append <work_item_id> <entry_type> <content>
```

```sh
animus ledger append abc123 decision "Using reqwest instead of rig-core for LLM calls"
animus ledger append abc123 finding "SSE parser needs to handle partial JSON across chunks"
animus ledger append abc123 step "Implemented SseParser::feed(), unit tests green"
```

*Available after M4.*

### `animus skill list`

List discovered skills and their metadata.

```
animus skill list [--dir DIR]
```

```sh
$ animus skill list
NAME                  CONCURRENT  ISOLATION  HOOKS
engage                true        -          orient, consolidate, recover
tdd-implementation    true        worktree   orient, consolidate
```

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

### `animus memory search`

Vector similarity search across stored memories.

```
animus memory search "query" [--person NAME] [--limit N] [--format FORMAT]
```

### `animus rel show`

Display relationship data for a person.

```
animus rel show <person> [--format FORMAT]
```

### `animus identity show`

Display the animus's self-knowledge.

```
animus identity show [--format FORMAT]
```

### `animus status`

Show the appliance status: database connectivity, queue depth, active foci, discovered skills, unroutable work.

```
animus status
```

```
Database:    connected (28 work items, 2 memories)
Queue:       4 messages (3 visible, 1 in-flight)
Skills:      2 discovered (engage, tdd-implementation)
Active foci: 0 / 4
Unroutable:  3 items (no matching skill)
```

---

## Implementation

Uses `clap` with derive macros for the subcommand structure:

```rust
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
        #[arg(long, default_value = "./skills")]
        skills: PathBuf,
        #[arg(long, default_value_t = 4)]
        max_concurrent: usize,
    },
    /// Work item operations
    Work {
        #[command(subcommand)]
        action: WorkAction,
    },
    /// Ledger operations
    Ledger {
        #[command(subcommand)]
        action: LedgerAction,
    },
    /// Skill operations
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
    /// LLM sub-calls (used by hook scripts)
    Llm {
        #[command(subcommand)]
        action: LlmAction,
    },
    /// Memory operations
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// Show appliance status
    Status,
}

#[derive(Subcommand)]
enum WorkAction {
    Submit { ... },
    List { ... },
    Show { id: String },
}

#[derive(Subcommand)]
enum LedgerAction {
    Show { work_item_id: String, ... },
    Append { work_item_id: String, entry_type: String, content: String },
}
```

Every command connects to Postgres via `DATABASE_URL`, runs migrations, and operates directly on the tables. No daemon needed for CLI commands (except `serve`).

---

## What to Build Now

The CLI grows with the system. Current priority:

1. **`animus serve`** — control plane daemon (exists)
2. **`animus work submit/list/show`** — work management (exists)
3. **`animus llm complete`** — sub-LLM calls for hook scripts (exists)
4. **`animus memory search`** — vector search for hook scripts
5. **`animus ledger append/show`** — ledger operations (needed for M4)
6. **`animus skill list`** — skill discovery
7. **`animus rel show`**, **`animus identity show`** — context loading for hooks

The engage skill's hook scripts (`skills/engage/scripts/`) define the full CLI interface needed. Each command is "red" until implemented.

---

## Dependencies

```toml
clap = { version = "4", features = ["derive"] }
```
