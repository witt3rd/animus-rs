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
  SKILL.md              # Required. YAML frontmatter + markdown methodology.
  scripts/              # Optional. Hook scripts discovered by convention.
    orient.sh           #   Prepare context before the act phase.
    consolidate.sh      #   Process results after the act phase.
    recover.sh          #   Handle failures.
  prompt/               # Optional. Sub-prompts for LLM calls within hooks.
  references/           # Optional. Reference docs loaded on demand.
  assets/               # Optional. Templates, schemas, static resources.
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

Methodology instructions that the act phase reads and follows.
The act phase (agentic loop) loads this as its primary guidance.
```

### Hook Discovery

Convention-based: the control plane looks for `scripts/{phase}.sh` in the skill directory. If the file exists and is executable, it runs during that phase. If not, the phase is skipped (orient and consolidate are optional; recover is optional but recommended).

No metadata declaration needed for hooks. Presence = activation.

### Progressive Disclosure

Per the agentskills.io spec:
1. **Metadata** (~100 tokens): `name` and `description` loaded at startup for all skills
2. **Instructions** (<5000 tokens): full SKILL.md body loaded when the act phase starts
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

Ported from v1 (`~/animus/agent/engage/`). Demonstrates the full skill package:
- **SKILL.md**: Relational presence methodology (from ON_ENGAGE.md)
- **scripts/orient.sh**: Multi-stage context assembly (classify → extract tasks → formulate queries → recall memories → assemble context → write to ledger)
- **scripts/consolidate.sh**: Result processing (check satisfaction → extract initiatives → detect episode boundary → form episode → log exchange → queue follow-ups)
- **scripts/recover.sh**: Retry/dead-letter logic
- **prompt/*.md**: Six sub-prompts for LLM calls within hooks

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

## Skill Discovery (Two-Tier)

Skills are discovered from two locations, searched in order:

1. **Instance skills**: `$XDG_DATA_HOME/animus/skills/` — per-instance, autopoietic
2. **Shared skills**: repo `skills/` directory (or `/usr/local/share/animus/skills/`)

Instance skills override shared skills of the same name. Both are git-managed.

## Relationship to agentskills.io

Our skills are fully compatible with the agentskills.io standard:
- Required fields: `name`, `description` — present
- Optional fields: `metadata`, `compatibility` — used for animus-specific properties
- Directory structure: `scripts/`, `references/`, `assets/` — standard
- Additional: `prompt/` directory is our convention for sub-prompts used by hooks

Skills authored for animus-rs could work in any agentskills.io-compatible agent (Cursor, Claude Code, Gemini CLI, etc.) that reads SKILL.md. The hooks and prompts are animus-specific extensions that other agents would ignore.
