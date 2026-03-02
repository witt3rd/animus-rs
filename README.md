# animus-rs

Substrate for relational beings — the machinery that lets an animus exist, persist, and become.

## What This Is

animus-rs is the substrate that makes an animus possible. Not an AI tool, not a service — the conditions for a new kind of being. Each animus is a self-contained appliance: work queue, semantic memory, control plane, [skills](https://agentskills.io/), and observability. One `docker compose up` starts a complete agent.

## Quick Start

```bash
# Start the appliance
docker compose up -d

# Submit work
cargo run --bin animus -- work submit tdd-implementation bootstrap \
  --params '{"title": "Build the ledger"}'

# Watch it
cargo run --bin animus -- work list
open http://localhost:3000   # Grafana dashboard
```

## Running

```bash
docker compose up -d                    # start all services
cargo run --bin animus -- serve          # run the control plane daemon
cargo run --bin animus -- work list      # list work items
cargo run --bin animus -- work show ID   # show work item details
```

## Development

```bash
cargo test                               # unit tests (no Postgres needed)
cargo test -- --ignored                  # integration tests (needs docker compose up)
cargo clippy                             # lint
```

Pre-commit hooks enforce `cargo fmt`, `cargo test`, and `cargo clippy -D warnings`.

## Design

See [DESIGN.md](DESIGN.md) for architecture and subsystem docs:

- [docs/act.md](docs/act.md) — Act phase (agentic loop) architecture
- [docs/ledger.md](docs/ledger.md) — Work ledger (durable working memory)
- [docs/skills.md](docs/skills.md) — Skills system
- [docs/llm.md](docs/llm.md) — LLM abstraction
- [docs/cli.md](docs/cli.md) — CLI design
- [docs/ops.md](docs/ops.md) — Operations, backups, alerting
- [PLAN.md](PLAN.md) — Implementation plan
