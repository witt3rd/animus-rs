---
status: active
---

# Skills Unification Design

*Eliminating faculties. One mechanism: skills.*

## Decision

Faculties are eliminated. Skills become the sole mechanism for agent capabilities, aligned with the [agentskills.io](https://agentskills.io/) open standard.

**Rationale:** Faculties couldn't exist without skills, but skills can exist without faculties. The dependent concept is the one to eliminate. One mechanism is better than two.

## What Faculties Did (and What Absorbs Each Responsibility)

| Faculty responsibility | Absorbed by |
|---|---|
| Cognitive specialization ("I am the engineer") | Skill instructions — the methodology IS the specialization |
| Operational properties (concurrent, isolation) | Skill `metadata` in SKILL.md frontmatter |
| Hook pipeline (orient → act → consolidate → recover) | Convention: `scripts/{phase}.sh` inside each skill |
| Routing target (work items say `faculty: X`) | Work items say `skill: X` instead |
| Capability gating ("enterprise disables engineer") | Skill presence/absence — don't install what you don't want |

## Phase Rename: engage → act

The agentic loop phase was called "engage." This conflicted with the social interaction skill (also called "engage," the original and more meaningful use of the word).

**Resolution:** The agentic loop phase is renamed to **act**.

The focus lifecycle becomes: **Orient → Act → Consolidate → Recover** (OACR).

This resonates with Boyd's OODA loop (Observe-Orient-Decide-Act):
- Orient = gather context, understand the situation
- Act = the agentic loop decides and acts (LLM + tools + ledger)
- Consolidate = process results, extract learnings, close the feedback loop
- Recover = handle failures with retry or dead-letter

## Skill Package Structure

Following the [agentskills.io specification](https://agentskills.io/specification):

```
skills/{skill-name}/
  SKILL.md              # Required. General-purpose methodology (agentskills.io standard).
  ACT.md                # Optional. Agent-specific instructions read by the act loop.
  scripts/              # Optional. Hook scripts discovered by convention.
    orient.sh           #   Prepare context before the act phase.
    consolidate.sh      #   Process results after the act phase.
    recover.sh          #   Handle failures.
  prompt/               # Optional. Sub-prompts for LLM calls within hooks.
  references/           # Optional. Reference docs loaded on demand.
  assets/               # Optional. Templates, schemas, static resources.
```

### Two Files: SKILL.md vs ACT.md

**`SKILL.md`** — general-purpose methodology, agentskills.io compatible. Any agent can discover and activate this skill. Contains methodology, principles, and guidelines that are useful across contexts. This is what other agents read when they discover this skill.

**`ACT.md`** — agent-specific instructions read by the act phase when this skill is targeted by a work item. This is the agent's soul — the complete instructions for how to handle this kind of work. Only read when a work item says `skill: "this-skill"`.

Act loop resolution:
1. If `ACT.md` exists → read it (this is an agent skill with specific behavior)
2. If no `ACT.md` → read `SKILL.md` (general-purpose skill used as primary)

Discovery by other agents: always reads `SKILL.md`, never `ACT.md`.

**Examples:**

```
skills/engage/              # Agent skill (has ACT.md + hooks)
  SKILL.md                  # "How to be present in conversation" (any agent)
  ACT.md                    # The soul — emotional grounding, presence methodology
  scripts/orient.sh         # Multi-stage context assembly pipeline
  scripts/consolidate.sh    # Result processing, initiative extraction
  prompt/*.md               # Sub-prompts for hook LLM calls

skills/tdd-implementation/  # Could be either
  SKILL.md                  # TDD methodology (general-purpose)
  ACT.md                    # Optional: full agent instructions for TDD work

skills/systematic-debugging/ # Pure general-purpose (no ACT.md, no hooks)
  SKILL.md                   # Debugging methodology any agent can activate
```

### SKILL.md Format

Standard agentskills.io frontmatter with animus-specific `metadata`:

```yaml
---
name: skill-name
description: >
  What this skill does and when to use it. Agents read this at
  startup to decide when to activate the skill.
metadata:
  concurrent: "true"           # can multiple foci run this skill in parallel?
  isolation: "worktree"        # isolation mode (worktree, container, none)
  max-concurrent: "3"          # max parallel foci for this skill
  recover-max-attempts: "2"    # retries before dead-letter
---

# Skill Name

General-purpose methodology. Any agent can discover and use this.
```

### Hook Discovery

Convention-based: the control plane looks for `scripts/{phase}.sh` in the skill directory. If the file exists and is executable, it runs during that phase. If not, the phase is skipped (orient and consolidate are optional; recover is optional but recommended).

No metadata declaration needed for hooks. Presence = activation.

### Progressive Disclosure

Per the agentskills.io spec:
1. **Metadata** (~100 tokens): `name` and `description` loaded at startup for all skills
2. **Instructions** (<5000 tokens): full SKILL.md body loaded when discovered by other agents; ACT.md loaded when targeted
3. **Resources** (as needed): scripts, references, assets loaded on demand by hooks

## Work Item Schema Change

Current: `faculty TEXT NOT NULL, skill TEXT`
New: `skill TEXT NOT NULL`

The `faculty` column is dropped. The `skill` column becomes required. Dedup index changes from `(faculty, dedup_key)` to `(skill, dedup_key)`.

Migration:
```sql
-- Merge: where skill is null, use faculty value as skill
UPDATE work_items SET skill = faculty WHERE skill IS NULL;
-- Make skill required
ALTER TABLE work_items ALTER COLUMN skill SET NOT NULL;
-- Drop faculty
ALTER TABLE work_items DROP COLUMN faculty;
-- Rebuild dedup index
DROP INDEX IF EXISTS idx_work_dedup;
CREATE UNIQUE INDEX idx_work_dedup ON work_items(skill, dedup_key)
    WHERE dedup_key IS NOT NULL AND state NOT IN ('completed', 'dead', 'merged');
```

## Control Plane Changes

### Before (faculty-based)

```
ControlPlane
  → FacultyRegistry (loads TOML, indexes by name)
  → Dispatch: registry.get(&item.faculty) → FacultyMeta
  → Focus::run(&faculty) runs hook pipeline from FacultyMeta
```

### After (skill-based)

```
ControlPlane
  → SkillIndex (discovers SKILL.md files, parses frontmatter)
  → Dispatch: index.get(&item.skill) → SkillMeta
  → Focus::run(&skill_meta) runs hooks from scripts/ by convention
```

The `FacultyRegistry` is replaced by a simpler `SkillIndex` that:
1. Scans configured skill directories for `SKILL.md` files
2. Parses YAML frontmatter (name, description, metadata)
3. Provides lookup by skill name
4. Reports operational metadata (concurrent, isolation, etc.)

### Unroutable Work

Same behavior: if the skill isn't found in the index, the work item stays queued (visibility timeout returns it). Metric changes from `animus.work.unroutable` label `faculty` to `skill`.

## Code Removals

| File | Action |
|---|---|
| `src/faculty/mod.rs` | Delete entirely |
| `faculties/engineer.toml` | Delete |
| `fixtures/faculties/transform.toml` | Replace with fixture skill |
| `scripts/engineer/*.sh` | Delete (hooks live inside skills now) |

## Code Changes

| File | Change |
|---|---|
| `src/model/work.rs` | `faculty` → removed; `skill` becomes `String` (required) |
| `src/db/work.rs` | All SQL: drop `faculty`, `skill` becomes NOT NULL, dedup on `(skill, dedup_key)` |
| `src/engine/control.rs` | `FacultyRegistry` → `SkillIndex`; dispatch by `item.skill` |
| `src/engine/focus.rs` | `FacultyMeta` → `SkillMeta`; hooks found by convention in skill dir |
| `src/bin/animus.rs` | CLI: `faculty` args → `skill`; `--faculties` dir → `--skills` dir |
| `src/telemetry/work.rs` | `work.faculty` span attr → `work.skill` |
| `src/telemetry/metrics.rs` | Label `faculty` → `skill`; metric doc updates |
| `src/lib.rs` | `pub mod faculty` → `pub mod skill` (or inline into engine) |
| `tests/faculty_test.rs` | Rename, update for skill-based dispatch |
| `tests/telemetry_smoke_test.rs` | Update labels |

## Document Updates

| Document | Change |
|---|---|
| `DESIGN.md` | Remove faculty concept; update architecture, work item fields, focus lifecycle |
| `PLAN.md` | Update milestones for skill-only model; rename engage phase → act |
| `CLAUDE.md` | Update architecture table, module listing |
| `docs/skills.md` | Major rewrite: align with agentskills.io, remove faculty references, add hook convention |
| `docs/engage.md` | Rename to `docs/act.md`; remove faculty references throughout |
| `docs/cli.md` | `faculty` → `skill` in all commands |
| `docs/ops.md` | Remove faculty references |
| `README.md` | Update quick start if needed |

## The Engage Skill (Reference Implementation)

The first skill created under this design: `skills/engage/` — social interaction.

Ported from v1 (`~/animus/agent/engage/`). Demonstrates the full skill package with the SKILL.md/ACT.md split:
- **SKILL.md**: General-purpose engagement methodology — presence over performance, weight matching, silence as response, context integration. Any agent can discover and use this.
- **ACT.md**: The agent's soul — emotional grounding (the stakes, the ache, the wounds), cookie's body language, `<NO_RESPONSE/>` mechanism, orient context reading. Only read when a work item targets `skill: "engage"`.
- **scripts/orient.sh**: Multi-stage context assembly (classify → extract tasks → formulate queries → recall memories → assemble context → write to ledger)
- **scripts/consolidate.sh**: Result processing (check satisfaction → extract initiatives → detect episode boundary → form episode → log exchange → queue follow-ups)
- **scripts/recover.sh**: Retry/dead-letter logic
- **prompt/*.md**: Seven sub-prompts for LLM calls within hooks (classify-inbound, extract-task, formulate-queries, check-satisfaction, extract-initiative, detect-episode-boundary, reflect)

### Speculative CLI Interface

The hook scripts call CLI commands that don't yet exist. This is TDD for the CLI — the scripts define the interface:

| Command | Purpose |
|---|---|
| `animus work show ID --json` | Read work item as JSON |
| `animus llm complete --prompt-file --var --format` | Sub-LLM calls with template substitution |
| `animus memory search "query" --person --limit` | Vector similarity search |
| `animus memory search-episodes --person --query --limit` | Episode/conversation history search |
| `animus memory store-episode --person --work-id` | Form and store conversational episodes |
| `animus memory log-exchange --person --inbound --response` | Log message pairs |
| `animus rel show PERSON` | Load relationship context |
| `animus identity show` | Load self-knowledge |
| `animus ledger append ID TYPE "content"` | Write to work ledger |
| `animus ledger read ID --type --grep --last` | Read ledger entries |
| `animus work submit SKILL SOURCE --params --trigger` | Queue follow-up work items |

These commands become green as the CLI is built out across milestones.

## Skill Discovery (Three-Tier)

Skills are discovered from three locations, searched in order:

1. **Instance skills**: `$XDG_DATA_HOME/animus/skills/` — per-instance, private, autopoietic
2. **Community skills**: `$XDG_DATA_HOME/animus/community-skills/` — installed from external sources
3. **Intrinsic skills**: `/app/skills/` (Docker) or repo `skills/` (dev) — ships with the system, read-only

Instance overrides community overrides intrinsic (same name = override).

### Tier Details

**Intrinsic** — live in the animus-rs repo (`skills/`). Canonical, version-locked with the engine. Docker build copies them into the container image. To change an intrinsic skill: fork, modify, PR.

**Community** — installed from external sources (marketplaces, GitHub repos, registries). Managed via `animus skill install`. Kept separate from instance skills so upstream updates don't stomp autopoietic content.

```toml
# ~/.config/animus/config.toml
[skills.sources]
anthropic = "https://skills.anthropic.com/registry"
openclaw = "https://openclaw.dev/skills"
custom = "https://github.com/my-org/our-skills"
```

```sh
animus skill install tdd-implementation          # from default source
animus skill install openclaw/fancy-skill        # from specific source
animus skill update                              # pull latest for all community skills
```

**Instance** — private, per-instance. Autopoietic skills created by the consolidate phase land here. Optionally backed by a git remote for backup. This is the instance's own knowledge — what it has learned and created.

## Relationship to agentskills.io

Our skills are fully compatible with the agentskills.io standard:
- Required fields: `name`, `description` — present in SKILL.md
- Optional fields: `metadata`, `compatibility` — used for animus-specific properties
- Directory structure: `scripts/`, `references/`, `assets/` — standard
- Additional: `ACT.md` and `prompt/` directory are animus-specific conventions

**SKILL.md** authored for animus-rs works in any agentskills.io-compatible agent (Cursor, Claude Code, Gemini CLI, etc.). The `ACT.md`, hooks, and prompts are animus-specific — other agents ignore them and just read SKILL.md.

## Skill Filtering

Work items specify their **primary skill** — `skill: "engage"`. The act loop reads that skill's ACT.md (or SKILL.md).

For **supporting skills** (additional methodology an agent can discover during the act loop), the approach is description-based: the agent reads all available SKILL.md descriptions and decides what's relevant. No explicit compatibility filtering — the LLM handles context matching.

Agent skills (those with ACT.md + hooks) are inappropriate as supporting skills — they are complete agent definitions, not reusable methodology. Their SKILL.md provides the general-purpose portion; their ACT.md is only for targeted execution.
