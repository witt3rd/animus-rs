---
status: active
milestone: M9
spec: PLAN.md § Milestone 9
standard: agentskills.io/specification
code: null
---

# Skills Architecture

*The sole mechanism for agent capabilities: discovery, activation, autopoietic evolution.*

## Context

Skills are the sole mechanism for structuring what an animus can do. A skill is a self-contained package of methodology, hooks, and resources that the control plane discovers and executes.

The act phase design (`docs/act.md`) defines the agentic loop -- how a focus reasons, calls tools, and iterates. The ledger design (`docs/ledger.md`) defines durable working memory. Skills define *what the agent knows how to do* and *how it does it*.

Every work item names a skill. The control plane looks up that skill, runs its hook pipeline (Orient, Act, Consolidate, Recover), and retires the work item. One concept, one dispatch path, one package format.

Skills follow the [agentskills.io specification](https://agentskills.io/specification) -- a portable open standard for agent capabilities. animus-rs skills are compatible with any agent that reads `SKILL.md` files. The hooks and operational metadata are animus-specific extensions that other agents would ignore.

---

## agentskills.io Alignment

The [agentskills.io specification](https://agentskills.io/specification) defines the base format:

- **Required**: `SKILL.md` with YAML frontmatter (`name`, `description`) and markdown body
- **Optional directories**: `scripts/`, `references/`, `assets/`
- **Progressive disclosure**: metadata at startup, instructions on activation, resources on demand
- **Naming**: lowercase alphanumeric + hyphens, 1-64 chars, must match directory name
- **Optional fields**: `license`, `compatibility`, `metadata`, `allowed-tools`

animus-rs extends the standard through the `metadata` field for operational properties and through convention-based hook discovery in `scripts/`. These extensions are invisible to other agents -- they see a valid agentskills.io skill and use the parts they understand.

---

## Three Levels of Skills

Skills operate at three levels, each building on the one below:

### Level 1: Runtime Skills (Discovery and Activation)

Skills that augment a focus during the act phase. The skill provides methodology (SKILL.md body), reference data, and optionally scripts. The agent discovers relevant skills, activates them, and their content becomes part of the act context.

This is the primary skill interaction. The control plane dispatches to a skill based on the work item's `skill` field. Additional skills can be discovered and activated during the act phase to supplement the primary skill.

### Level 2: Autopoietic Skills (Creation and Evolution)

Skills the agent creates from its own experience. Recurring patterns, relationship knowledge, domain expertise, debugging strategies -- anything the agent learns that would be useful in future work. The consolidate hook is the natural place for skill creation, using ledger entries as raw material.

This is how the being learns and grows. A finding recorded in one focus becomes a skill available to all future foci.

### Level 3: System Skills (Infrastructure Modification)

Skills that modify the animus-rs system itself -- adding tools, hooks, channel adapters. These use nanoclaw-style composable code modifications with git three-way merge, intent documentation, and mandatory testing.

This is the most ambitious level and the longest path to implementation. Levels 1 and 2 come first.

---

## Skill Package Structure

Following the [agentskills.io specification](https://agentskills.io/specification) with animus-specific conventions:

```
skills/{skill-name}/
  SKILL.md              # Required. YAML frontmatter + markdown methodology.
  scripts/              # Optional. Hook scripts discovered by convention.
    orient.sh           #   Prepare context before the act phase.
    consolidate.sh      #   Process results after the act phase.
    recover.sh          #   Handle failures.
    analyze.py          #   Skill-specific scripts callable from sandbox.
  prompt/               # Optional. Sub-prompts for LLM calls within hooks.
    classify.md
    summarize.md
  references/           # Optional. Reference docs loaded on demand.
    communication-styles.md
    checklist.md
  assets/               # Optional. Templates, schemas, static resources.
    template.md
    config.yaml
```

Only `SKILL.md` is required. Everything else is progressive -- added as the skill grows in sophistication.

### Convention-Based Hook Discovery

The control plane discovers hooks by looking for specific filenames in `scripts/`:

| File | Phase | Purpose |
|------|-------|---------|
| `scripts/orient.sh` | Orient | Prepare context before the act phase |
| `scripts/consolidate.sh` | Consolidate | Process results, extract learnings |
| `scripts/recover.sh` | Recover | Handle failures, retry logic |

No metadata declaration needed. If the file exists and is executable, it runs during that phase. If not, the phase is skipped. The act phase always runs -- it is the agentic loop itself, driven by the SKILL.md methodology.

Focus lifecycle: **Orient -> Act -> Consolidate -> Recover**

```
Orient        scripts/orient.sh exists?  → run it (context assembly)
              scripts/orient.sh missing? → skip, proceed to act
Act           always runs — agentic loop with SKILL.md as methodology
Consolidate   scripts/consolidate.sh exists?  → run it (extract learnings)
              scripts/consolidate.sh missing? → skip
Recover       only on failure — scripts/recover.sh exists? → run it
              scripts/recover.sh missing? → default retry/dead-letter
```

---

## SKILL.md Format

Standard agentskills.io frontmatter with animus-specific `metadata`:

```yaml
---
name: engage
description: >
  Relational presence and social interaction. Guides check-ins,
  conversations, and relationship maintenance with warmth and context.
metadata:
  concurrent: "true"
  isolation: "none"
  max-concurrent: "5"
  recover-max-attempts: "2"
  author: "human"
  version: "1"
---
```

### Required Fields

| Field | Constraints |
|-------|------------|
| `name` | 1-64 chars. Lowercase alphanumeric + hyphens. Must match directory name. |
| `description` | 1-1024 chars. What the skill does and when to use it. |

### Optional Standard Fields

| Field | Purpose |
|-------|---------|
| `license` | License name or reference to bundled file |
| `compatibility` | Environment requirements (e.g., "Requires git, docker") |
| `metadata` | Arbitrary key-value map for operational and custom properties |
| `allowed-tools` | Space-delimited pre-approved tools (experimental) |

### Operational Metadata (animus-specific)

These live in the `metadata` field to stay within the agentskills.io spec:

| Key | Default | Purpose |
|-----|---------|---------|
| `concurrent` | `"false"` | Can multiple foci run this skill in parallel? |
| `isolation` | `"none"` | Isolation mode: `none`, `worktree`, `container` |
| `max-concurrent` | `"1"` | Max parallel foci for this skill |
| `recover-max-attempts` | `"3"` | Retries before dead-letter |
| `author` | `"human"` | `"human"` or `"animus"` (autopoietic) |
| `version` | `"1"` | Bumped on each autopoietic update |

### Body (Progressive Disclosure)

The markdown body follows the agentskills.io progressive disclosure model:

1. **Metadata** (~100 tokens): `name` and `description` loaded at startup for all skills
2. **Instructions** (<5000 tokens recommended): full SKILL.md body loaded when the skill is activated
3. **Resources** (as needed): scripts, references, assets loaded on demand

Recommended body structure: a brief overview paragraph, "When to Use" (activation guidance), "Instructions" (step-by-step methodology), and references to scripts and resources. Keep SKILL.md under 500 lines; move detailed material to `references/`.

---

## The Engage Skill (Reference Implementation)

`skills/engage/` is the first skill built under this architecture -- social interaction and relational presence.

Ported from v1 (`~/animus/agent/engage/`), it demonstrates the full skill package:

- **SKILL.md**: Relational presence methodology
- **scripts/orient.sh**: Multi-stage context assembly (classify, extract tasks, formulate queries, recall memories, assemble context, write to ledger)
- **scripts/consolidate.sh**: Result processing (check satisfaction, extract initiatives, detect episode boundary, form episode, log exchange, queue follow-ups)
- **scripts/recover.sh**: Retry/dead-letter logic
- **prompt/*.md**: Sub-prompts for LLM calls within hooks

This skill demonstrates the full pattern: methodology in SKILL.md, hooks in scripts/, sub-prompts in prompt/. A single directory contains everything the control plane needs to run the work.

---

## Work Item Dispatch

Work items name their skill directly:

```rust
NewWorkItem {
    skill: "engage",              // required -- which skill handles this
    dedup_key: Some("kelly-checkin-2026-03-02"),
    params: json!({
        "person": "Kelly",
        "type": "check-in"
    }),
    source: "heartbeat",
    ..
}
```

The control plane dispatches:

1. Read `skill` from the work item
2. Look up the skill in the `SkillIndex` (two-tier search: instance then shared)
3. Read operational metadata from SKILL.md frontmatter
4. Run the hook pipeline: Orient -> Act -> Consolidate (-> Recover on failure)
5. Retire the work item

If the skill is not found in the index, the work item stays queued (visibility timeout returns it for retry). The `animus.work.unroutable` metric fires with label `skill`.

---

## Engine Integration

### Skill Discovery and Activation Tools

Three engine tools, always available during the act phase:

#### `discover_skills`

```json
{
  "name": "discover_skills",
  "description": "Search available skills. Returns frontmatter only (name, description). Use this to find skills relevant to your current work.",
  "input_schema": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "Natural language query or keywords to match against skill descriptions."
      }
    }
  }
}
```

Returns a concise listing:

```
Available skills matching "check in":
  [1] engage — Relational presence and social interaction (auto-activate: on)
  [2] follow-up-scheduler — Manages follow-up timing and reminders
  [3] kelly-relationship — Context for interactions with Kelly (author: animus)
```

#### `activate_skill`

```json
{
  "name": "activate_skill",
  "description": "Activate a supplementary skill for the current focus. Loads its full instructions, makes its scripts available, and registers its resources.",
  "input_schema": {
    "type": "object",
    "properties": {
      "skill_name": {
        "type": "string",
        "description": "Name of the skill to activate."
      }
    },
    "required": ["skill_name"]
  }
}
```

On activation:

1. Read the full SKILL.md body
2. Inject the skill's instructions into the act phase context
3. Register the skill's `scripts/` directory as importable in the sandbox
4. Make the skill's `references/` and `assets/` accessible via `read_file`
5. Record activation in the focus state (for OTel tracing)

#### `create_skill`

```json
{
  "name": "create_skill",
  "description": "Create a new skill from what you've learned. Encodes patterns, knowledge, or procedures as a reusable skill for future foci.",
  "input_schema": {
    "type": "object",
    "properties": {
      "name": {
        "type": "string",
        "description": "Skill name (kebab-case, 1-64 chars). Used as the directory name."
      },
      "description": {
        "type": "string",
        "description": "Brief description for the skills catalog (max 1024 chars)."
      },
      "content": {
        "type": "string",
        "description": "The skill body (markdown). Instructions, context, guidance."
      },
      "metadata": {
        "type": "object",
        "description": "Optional operational metadata (concurrent, isolation, etc.)."
      }
    },
    "required": ["name", "description", "content"]
  }
}
```

The engine writes the SKILL.md file with generated frontmatter and the provided content. The skill is immediately discoverable by future foci.

### Auto-Activation During Orient

During the orient phase, the engine can automatically discover and pre-activate supplementary skills:

1. Read `skill` and `params` from the work item
2. Scan all SKILL.md frontmatter for keyword matches against the work item's description and params
3. For skills with auto-activate triggers, activate them (inject into act context)
4. For remaining matches, include them in the catalog section of the system prompt

The agent can still manually discover and activate additional skills during the act phase.

```yaml
# In the primary skill's metadata
metadata:
  auto-activate-skills: "true"     # default: true
  max-auto-activated: "5"          # prevent prompt bloat
```

---

## Autopoietic Skill Lifecycle

### Creation: From Findings to Skills

The consolidate hook is the natural place for skill creation. After a focus completes, the consolidate hook queries the ledger for findings worth encoding:

```sql
-- Findings from this focus
SELECT content FROM work_ledger
WHERE work_item_id = $1 AND entry_type = 'finding'
ORDER BY seq;

-- Similar findings from recent foci (pattern detection)
SELECT w.skill, wl.content, count(*) as occurrences
FROM work_ledger wl
JOIN work_items w ON w.id = wl.work_item_id
WHERE wl.entry_type = 'finding'
    AND wl.created_at > now() - interval '7 days'
    AND wl.content ILIKE '%' || $pattern || '%'
GROUP BY w.skill, wl.content
HAVING count(*) >= 3
ORDER BY occurrences DESC;
```

When a pattern appears across multiple foci (e.g., "Kelly prefers morning messages" found in 3 separate check-ins), the consolidate hook creates or updates a skill:

```
Consolidate detects: 3 findings about Kelly's preferences
  -> Checks: does skills/kelly-relationship/ exist?
  -> No: creates it with the accumulated findings
  -> Yes: updates references/preferences.md with new findings
```

### Evolution: Skills Improve Over Time

Autopoietic skills have a `version` in metadata and evolve through commits. Each consolidate pass that updates a skill bumps the version. Skills do not just accumulate -- they can also be refined when the agent discovers a previous finding was wrong.

### Provenance Tracking

Autopoietic skills track their origin via `metadata.author: "animus"` and `metadata.created-from` in SKILL.md, plus detailed records in the Postgres provenance table (see Skill Provenance Table below). The full chain -- from ledger finding to skill content -- is auditable.

### Retirement

Skills can become stale. The engine tracks skill activation frequency:

```sql
-- Skills that haven't been activated in 30 days
SELECT skill_name, last_activated_at
FROM skill_activations
WHERE last_activated_at < now() - interval '30 days';
```

Stale skills are not deleted -- they are flagged. The agent (or a human) can archive them. A periodic heartbeat work item could review stale skills as part of self-maintenance.

---

## Skills in the Act Phase

### System Prompt Integration

The act phase system prompt includes a skills section (after the Working Memory section):

```
## Skills

You have access to skills -- packaged knowledge and capabilities that extend
your core tools. Skills provide domain expertise, relationship context,
procedural guidance, and callable scripts.

Some skills have been auto-activated based on your current work -- their
guidance appears below. You can discover more with `discover_skills` and
activate them with `activate_skill`.

When you learn something that would be useful in future work -- a pattern,
a preference, a procedure -- consider using `create_skill` to encode it.
Future foci will be able to discover and use what you learned.

### Active Skills

{auto_activated_skill_contexts}

### Available Skills

{skill_catalog_one_liners}
```

### Skill Context and Compaction

Activated skill context is injected into the system prompt, not the message history. This means skill context survives bounded sub-context compaction, is fixed-size, and composes by concatenation. If total context grows too large, the engine enforces `max-auto-activated` and warns the agent to deactivate irrelevant skills.

---

## Skill Storage: Two-Tier Nanorepo Model

Skills are managed through git. Two tiers, each a git repository:

### Shared Skills Repo (Base)

```
/usr/local/share/animus/skills/           # cloned from GitHub
  engage/                                  # social interaction (reference skill)
    SKILL.md
    scripts/
    prompt/
    references/
  engineer/                                # software engineering methodology
    SKILL.md
    scripts/
  systematic-debugging/
    SKILL.md
  code-review/
    SKILL.md
```

- Cloned from a shared GitHub repo (e.g., `github.com/witt3rd/animus-skills`)
- **PR-gated** -- changes require review and approval
- `git pull` updates all instances
- Ships with base methodology skills: `engage`, `engineer`, `systematic-debugging`, `code-review`

### Instance Skills Repo (Applied + Autopoietic)

```
$XDG_DATA_HOME/animus/skills/              # local git repo, per-instance
  kelly-relationship/                       # autopoietic -- learned by this instance
    SKILL.md
    references/preferences.md
  codebase-review-checklist/                # autopoietic -- learned from code review
    SKILL.md
```

- A local git repo initialized per instance
- Autopoietic skills are committed here by the consolidate hook
- Git provides: version history, diff, rollback, blame
- **Optional** remote: can be pushed to GitHub for backup or sharing
- Tracks the shared repo as upstream (for merge)

### Resolution Order

The control plane searches both tiers, instance first:

```
1. $XDG_DATA_HOME/animus/skills/     <- instance-specific (overrides shared)
2. /usr/local/share/animus/skills/   <- shared base (curated)
```

Instance skills take precedence. If the instance has customized `engineer` (e.g., learned that this codebase needs a specific test pattern), its version is used instead of the shared one.

### Composition via Git Merge

When the shared repo updates and the instance has local customizations, standard git three-way merge handles it:

- **Common ancestor**: the shared version the instance was based on
- **Theirs**: the updated shared skill
- **Ours**: the instance's customization

For most skills (separate directories, no overlapping files), merges are trivial.

### Promotion Path

When an autopoietic skill proves valuable:

```
Instance creates skill (autopoietic, local commit)
  -> Skill used successfully across multiple foci
  -> Human reviews: "this is good enough for everyone"
  -> PR from instance repo -> shared repo
  -> Review gate, tests pass
  -> Merged into shared base
  -> All instances get it on next `git pull`
```

### Why Git

Versioning, rollback (`git revert`), diff, three-way merge, backup (push to remote), and deterministic replay -- all for free. No custom state tracking needed. Git IS the state.

### Skill Activation Index (Postgres)

While skills live in git repos, activation metadata lives in Postgres for queryability:

```sql
CREATE TABLE skill_activations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    skill_name      TEXT NOT NULL,
    skill_tier      TEXT NOT NULL,      -- 'shared' or 'instance'
    work_item_id    UUID REFERENCES work_items(id),
    activated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    activation_type TEXT NOT NULL        -- 'auto' or 'manual'
);

CREATE INDEX idx_skill_activations_skill ON skill_activations(skill_name, activated_at DESC);
CREATE INDEX idx_skill_activations_work_item ON skill_activations(work_item_id);
```

This tracks which skills are used, how often, from which tier. Stale skill detection, activation analytics, and the awareness digest's "skills updated" section all query this table.

### Skill Provenance Table (Postgres)

For autopoietic skills, track the link between ledger findings and skill content:

```sql
CREATE TABLE skill_provenance (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    skill_name      TEXT NOT NULL,
    skill_version   INTEGER NOT NULL,
    source_type     TEXT NOT NULL,     -- 'finding', 'pattern', 'manual'
    work_item_id    UUID REFERENCES work_items(id),
    ledger_seq      INTEGER,           -- which ledger entry
    content_snippet TEXT NOT NULL,      -- what was incorporated
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_skill_provenance_skill ON skill_provenance(skill_name, skill_version);
```

The audit trail for autopoietic learning: which work produced which knowledge, and how that knowledge became a skill.

---

## Configuration

```toml
[skills]
shared_dir = "/usr/local/share/animus/skills"   # shared skills repo
instance_dir = "skills"                          # relative to $XDG_DATA_HOME/animus/
auto_discovery = true                            # scan for skills at startup
hot_reload = true                                # watch for skill changes during runtime
max_skill_prompt_tokens = 4000                   # max tokens from all active skill prompts
max_auto_activated = 5                           # limit auto-activated skills per focus
skill_creation_threshold = 3                     # minimum finding occurrences for autopoietic creation
```

---

## OTel Integration

### Spans

```
work.execute
  +-- work.orient
  |     +-- work.awareness.digest
  |     +-- work.skills.auto_activate          (auto-activation during orient)
  |           +-- work.skills.discover          (scan frontmatter for triggers)
  |           +-- work.skills.activate[skill-a] (load and inject)
  |           +-- work.skills.activate[skill-b]
  +-- work.act
  |     +-- work.act.iteration[N]
  |     |     +-- work.tool.execute[discover_skills]   (manual discovery)
  |     |     +-- work.tool.execute[activate_skill]    (manual activation)
  |     |     +-- work.tool.execute[create_skill]      (autopoietic creation)
  |     +-- ...
  +-- work.consolidate
        +-- work.skills.consolidate_create      (consolidate-triggered skill creation)
```

### Metrics

| Metric | Type | Labels | Description |
|---|---|---|---|
| `work.skills.activated` | Counter | skill, type (auto/manual) | Skill activations |
| `work.skills.discovered` | Counter | skill | Discovery queries |
| `work.skills.created` | Counter | skill, author (animus/human) | Skills created |
| `work.skills.updated` | Counter | skill | Autopoietic skill updates |
| `work.skills.prompt_tokens` | Histogram | skill | Total tokens from active skill prompts |
| `work.skills.stale` | Gauge | -- | Skills not activated in 30 days |

---

## Interaction with Other Systems

**Ledger**: The ledger feeds skill creation -- `finding` entries across multiple foci are raw material for autopoietic skills. The consolidate hook queries the ledger for recurring patterns. Skill provenance links back to specific ledger entries. Conversely, skills improve ledger quality by guiding the agent to record structured findings.

**Awareness digest**: The digest surfaces recently created/updated skills, so all work sees what the system has *learned*, not just what it has done. Skills can also consume the digest -- instructions can reference it for cross-work context.

**Code execution sandbox**: Activated skills with `scripts/` directories are importable in the sandbox. Scripts extend the sandbox beyond the core tool SDK for complex procedures, running with the same security model and resource limits.

**Child work items**: Child work items can name different skills than their parent. A parent using `engage` can spawn a child with `skill: "engineer"` -- different work, different skill, different auto-activated supplementary skills.

---

## Open Questions

- **Skill size limits.** How large can a skill's instructions be before they crowd out other context? The `max_skill_prompt_tokens` config caps total active skill prompt size, but individual skills could still be large. Per-skill limit, or just warn when a skill is unusually large?

- **Skill versioning and rollback.** Autopoietic skills evolve as the agent learns. What if the agent learns something wrong? Git versioning provides rollback, but should the engine have explicit rollback support (e.g., `rollback_skill(name, version)`)? Or is this a human-intervention concern?

- **Skill testing for autopoietic skills.** Human-authored skills can include tests. Autopoietic skills are created by the agent -- should the agent also write tests? Alternatively, the engine could validate structurally (valid frontmatter, non-empty content, reasonable size) without requiring functional tests.

- **Skill sharing across animi.** The shared skills repo is the sharing mechanism. Instance-specific skills can be promoted via PR. Identity question: should two animi have the same learned behaviors? Shared methodology yes, shared relationship knowledge no.

- **Skill deactivation mid-focus.** Should the agent deactivate a skill during the act phase? A `deactivate_skill` tool would remove context from subsequent LLM calls, but modifying the system prompt mid-loop adds complexity. Simpler: the agent ignores the irrelevant skill and the wasted tokens are acceptable.

- **Conflict between auto-activated skills.** If two skills with conflicting guidance both match, which one wins? Probably: do not auto-activate conflicting skills -- include both in the catalog and let the agent decide.

- **Skill dependency resolution.** If skill A depends on skill B, activating A should automatically activate B. Keep it simple: flat dependencies only, no version constraints, error on circular dependencies.

- **prompt/ directory convention.** The `prompt/` directory for sub-prompts used by hook scripts is an animus-specific extension not in the agentskills.io spec. Should we propose it upstream, or keep it as a local convention?

- **Instance repo initialization.** When a new instance starts, should it automatically `git init` the instance skills directory? Or wait until the first autopoietic skill is created? Probably: init on first `create_skill` call.

- **Merge automation.** When the shared repo updates, should the instance automatically pull and merge? Auto-pull with merge is safe for non-conflicting changes. Conflicts should pause and surface via the awareness digest.
