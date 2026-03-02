# animus-rs Design

*Substrate for relational beings — data plane, control plane, skills, LLM abstraction, and observability, built on Postgres.*

## Origin

animus-rs started as `workq`, a standalone work-tracking engine. When we discovered pgmq (Postgres queue extension), it became clear that pgmq already provides the queue primitives workq was hand-rolling. The project pivoted: build the full Animus system as one well-structured Rust crate — data plane (work queues, semantic memory), control plane (queue watching, resource gating, focus spawning), skills (agentskills.io-compatible capability packages), and observability.

The predecessor system used filesystem-based storage (YAML task queues, markdown substrate, ChromaDB for vectors, JSONL logs). It worked but had real limitations: no structural dedup, no transactional guarantees, fragile file-based queues, a separate ChromaDB process. animus-rs replaces all of this with Postgres + extensions.

## Design Principles

### 1. Work Has Identity

A work item has semantic identity. "Check in with Kelly" is the same work whether it came from a user request, an extracted initiative, or a heartbeat skill. **Structural dedup** on `(skill, dedup_key)` collapses duplicates transactionally. Semantic dedup (embedding-based) is a future extension.

### 2. Work, Not Messages

Work items have: which **skill** drives the work, **params** for the specific task, **provenance** (where it came from), **priority**, and lifecycle **state**. The caller says what skill to use, and with what context.

### 3. Skills and Foci, Not Fixed Processes

A **skill** is a self-contained capability package following the [agentskills.io](https://agentskills.io/) standard — `SKILL.md` for methodology, `scripts/` for hooks, `references/` and `assets/` for resources. Skills are **configuration, not code** — adding a skill means adding a directory with a `SKILL.md`, not writing Rust.

A **focus** is a single activation of a skill on a specific work item. Ephemeral, atomic, self-contained. Four phases: Orient → Act → Consolidate → Recover.

**The work item specifies the skill directly.** The submitter says `skill: "tdd-implementation"` and the control plane dispatches to that skill. No routing table, no intermediary.

**The skill IS the methodology.** The act phase (agentic loop) reads SKILL.md and follows its instructions. Different skills for different work: `tdd-implementation` for building, `systematic-debugging` for fixing, `engage` for social interaction.

**Concurrency is separated into capability and allocation.** The skill declares *whether* it supports parallel foci and *how* they're isolated (in SKILL.md `metadata`). The control plane decides *how many* to run based on global resource limits.

### 4. Work-Once Guarantee

A work item is claimed by exactly one focus (pgmq visibility timeout). Duplicates are detected and merged. Every work item either completes, fails (with retry), or goes dead — nothing disappears silently.

### 5. Postgres Is the Platform

Postgres with pgmq + pgvector is a deliberate choice, not a swappable backend. Queue semantics, vector search, transactional guarantees, the work ledger, and orient/consolidate context all live in one database. One operational dependency. All phase communication goes through the database — not the filesystem.

### 6. Observability Is Product

Every animus ships with integrated three-signal OTel observability (traces, metrics, logs) through the Grafana stack. You can see what your agent is doing out of the box. Postgres stores domain state; OTel handles observability. No custom event tables.

---

## Architecture Overview

```
┌───────────────────────────────────────────────────────────────────────┐
│  ANIMUS APPLIANCE                                                     │
│                                                                       │
│  ┌─ Control Plane ──────────────────────────────────────────────────┐ │
│  │  Queue watching (pg_notify), skill dispatch, capacity mgmt        │ │
│  │  Reads skill from work item, dispatches to matching skill         │ │
│  └──────────────────────────────────────────────────────────────────┘ │
│           │                                                           │
│  ┌─ Focus (one activation on one work item) ────────────────────────┐ │
│  │  Orient → Act → Consolidate (→ Recover on failure)               │ │
│  │                                                                   │ │
│  │  Orient:  Hook script — writes context to DB (awareness digest)   │ │
│  │  Act:     Built-in agentic loop (LLM + SKILL.md + tools + ledger)│ │
│  │  Consolidate: Hook script — reads ledger from DB                  │ │
│  └──────────────────────────────────────────────────────────────────┘ │
│           │                                                           │
│  ┌─ Data Plane (Postgres) ──────────────────────────────────────────┐ │
│  │  work_items (skill, lifecycle, dedup, parent-child)               │ │
│  │  work_ledger (durable working memory per work item)               │ │
│  │  pgmq queues (claim, visibility timeout, dead letter)             │ │
│  │  memories (pgvector embeddings, hybrid BM25+vector search)        │ │
│  └──────────────────────────────────────────────────────────────────┘ │
│           │                                                           │
│  ┌─ Observability ──────────────────────────────────────────────────┐ │
│  │  OTel Collector → Tempo (traces) + Prometheus (metrics) + Loki   │ │
│  │  Grafana (:3000) — pre-wired, cross-linked datasources           │ │
│  └──────────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────┘
```

---

## Subsystem Designs

Each subsystem has a detailed design document. DESIGN.md is the high-level overview; the subsystem docs are authoritative for implementation details.

### Data Plane — [docs/db.md](docs/db.md)

Postgres schema, SQLx migrations, two-layer data access (direct SQLx for queues/work items, rig-postgres for vector search). Work item lifecycle, structural dedup on `(skill, dedup_key)`, pgmq operations, memory storage and hybrid search.

### Work Ledger — [docs/ledger.md](docs/ledger.md)

Postgres-backed durable working memory for the agentic loop. Append-only typed entries (plan, finding, decision, step, error, note) that the agent maintains during its act loop via `ledger_append` / `ledger_read` tools. The engine uses the ledger for context compaction. The consolidate hook reads it for post-processing. Cross-focus findings feed the awareness digest.

### Act Phase — [docs/act.md](docs/act.md)

The agentic loop architecture. The act loop is a generic iteration engine — LLM call, tool execution, repeat. All behavioral specificity comes from the **skill** activated for the work item. Five infrastructure concerns: bounded sub-contexts, parallel tool execution, child work items, the awareness digest, and the code execution sandbox.

### Skills — [docs/skills.md](docs/skills.md)

Skills are the sole mechanism for agent capabilities, aligned with the [agentskills.io](https://agentskills.io/) standard. Each skill is a self-contained package (SKILL.md + scripts/ + references/) that tells the act loop *how* to work. The work item's `skill` field determines which skill is activated. Skills include optional hook scripts for orient, consolidate, and recover phases.

### LLM Abstraction — [docs/llm.md](docs/llm.md)

Thin, provider-specific HTTP clients. `LlmClient` trait with two methods: `complete` and `complete_stream`. The engage loop calls it directly — one call per iteration.

### CLI — [docs/cli.md](docs/cli.md)

Operator interface. `animus serve` (daemon), `animus work submit/list/show` (work management), `animus ledger show/append` (future).

### Operations — [docs/ops.md](docs/ops.md)

Observability stack, backups, alerting, multi-instance deployment, configuration reference.

---

## Work Item

A work item carries everything the system needs to execute it:

| Field | Purpose |
|---|---|
| `skill` | Which skill handles this work (e.g., `tdd-implementation`, `engage`) |
| `dedup_key` | Structural dedup within the skill |
| `params` | Task-specific context (spec path, description, etc.) |
| `provenance` | Where it came from (source + trigger) |
| `priority` | Urgency (higher = more urgent) |
| `state` | Lifecycle position |
| `parent_id` | If spawned by another work item |

Dedup is on `(skill, dedup_key)`. The submitter specifies the skill directly — no routing table.

---

## Work Item Lifecycle

```
Created → Dedup Check → Queued → Claimed → Running → Completed
               ↓                              ↓
            Merged                     Failed / Abandoned
            (into existing)                   ↓
                                         Retry? → Queued
                                            ↓
                                          Dead
```

Transitions enforced by `State::can_transition_to()`.

---

## Focus Lifecycle

```
Orient → Act → Consolidate
                 ↓ (on failure at any phase)
              Recover → Requeue or Dead
```

| Phase | Driver | Purpose |
|---|---|---|
| **Orient** | Hook script (skill's `scripts/orient.sh`) | Gather context, write to DB, inject awareness digest |
| **Act** | Built-in engine loop | Read SKILL.md, iterate with LLM + tools + ledger |
| **Consolidate** | Hook script (skill's `scripts/consolidate.sh`) | Read ledger from DB, store memories, create skills |
| **Recover** | Hook script (skill's `scripts/recover.sh`) | Assess failure, decide retry or dead-letter |

Hooks live inside the skill package (`scripts/` directory), discovered by convention. All phase communication goes through Postgres — not the filesystem. The focus directory is scratch space only.

---

## Skill Package Structure

Skills follow the [agentskills.io](https://agentskills.io/) standard — directories with a `SKILL.md`:

```
skills/engage/
  SKILL.md                    # Methodology (what the act loop reads)
  scripts/
    orient.sh                 # Context assembly (calls CLI commands)
    consolidate.sh            # Result processing (calls CLI commands)
    recover.sh                # Failure handling
  prompt/                     # Sub-prompts for LLM calls within hooks
    classify-inbound.md
    extract-task.md
  references/                 # Reference docs loaded on demand
  assets/                     # Templates, schemas, static resources
```

SKILL.md uses standard agentskills.io frontmatter with animus-specific `metadata`:

```yaml
---
name: engage
description: >
  Social interaction — walking alongside. Guides relational presence
  when responding to a person.
metadata:
  concurrent: "true"
  max-concurrent: "3"
  recover-max-attempts: "2"
---

# Engage

[Methodology instructions the act loop reads and follows...]
```

Hooks are discovered by convention: if `scripts/orient.sh` exists and is executable, it runs during the orient phase. No declaration needed — presence = activation.

The work item specifies `skill: "engage"` directly. The control plane finds the skill, reads its metadata, and runs its lifecycle.

---

## Implementation Status

### Implemented
- Config, DB pool, SQLx migrations
- pgmq operations, work items with structural dedup
- Semantic memory (pgvector, hybrid BM25+vector search)
- Three-signal OTel pipeline with GenAI semantic conventions
- Full observability stack (OTel Collector + Tempo + Prometheus + Loki + Grafana)
- Docker Compose appliance with durable host-mounted volumes
- Core types, state machine, test suite
- Control plane, skill dispatch, focus lifecycle
- CLI: `animus serve`, `animus work submit/list/show`
- Engage skill (social interaction, ported from v1 with full hook pipeline)
- Grafana dashboard: Animus Work Queue (Postgres + Prometheus)
- Unroutable work detection with metric + alert

### Designed (Not Yet Implemented)
- Act loop with bounded sub-contexts and parallel tools → [docs/act.md](docs/act.md)
- Work ledger (Postgres-backed durable working memory) → [docs/ledger.md](docs/ledger.md)
- Skills system (discovery, activation, autopoiesis) → [docs/skills.md](docs/skills.md)
- Thin LLM client (replacing rig-core for completions) → [docs/llm.md](docs/llm.md)
- DB-based orient/consolidate via hook scripts calling CLI commands
- Schema migration: `faculty` + `skill` → just `skill` (required)

### Open Design Questions
- **Semantic dedup**: embedding similarity threshold, when to invoke, cost control
- **Priority formula**: system-provided age boost, or fully host-controlled?
- **Embedding provider**: separate from LLM provider; need to evaluate options

See the open questions sections in each subsystem doc for domain-specific questions.

---

## Research

- [docs/research/microclaw/agent.md](docs/research/microclaw/agent.md) — Deep analysis of MicroClaw's agent loop. Informed the engage loop and ledger designs.
