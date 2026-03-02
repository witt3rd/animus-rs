#!/usr/bin/env bash
set -euo pipefail
#
# orient.sh — Prepare relational context for the engage skill.
#
# Ported from v1 pre_engage (animus.rel.pre.pre_engage).
# Pipeline: classify → extract tasks → formulate queries → recall → assemble.
#
# Called by the control plane before the act phase.
# Environment variables provided by the engine:
#   ANIMUS_WORK_ID    — current work item ID
#   ANIMUS_SKILL      — "engage"
#   ANIMUS_FOCUS_DIR  — scratch directory for this focus
#   ANIMUS_PHASE      — "orient"
#
# CLI commands used below are SPECULATIVE — they define the interface
# the animus CLI needs to support. These scripts are "red" until the
# CLI is built. Each command documents its expected behavior.
#

SKILL_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# ---------------------------------------------------------------------------
# 1. Read work item params
# ---------------------------------------------------------------------------
# animus work show --json: returns the full work item as JSON
# Expected: { "id": "...", "skill": "engage", "params": { "person": "...", "text": "..." }, ... }

WORK_JSON=$(animus work show "$ANIMUS_WORK_ID" --json)
PERSON=$(echo "$WORK_JSON" | jq -r '.params.person')
TEXT=$(echo "$WORK_JSON" | jq -r '.params.text')

if [ -z "$PERSON" ] || [ "$PERSON" = "null" ]; then
  echo "ERROR: work item missing params.person" >&2
  exit 1
fi
if [ -z "$TEXT" ] || [ "$TEXT" = "null" ]; then
  echo "ERROR: work item missing params.text" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# 2. Get recent exchanges for classification context
# ---------------------------------------------------------------------------
# animus memory search-episodes: search episode/exchange history
#   --person NAME   filter to this person's conversations
#   --limit N       max results
#   --format text   return as readable text (not JSON)

RECENT=$(animus memory search-episodes \
  --person "$PERSON" \
  --limit 5 \
  --format text 2>/dev/null || echo "(no recent exchanges)")

# ---------------------------------------------------------------------------
# 3. Classify inbound message
# ---------------------------------------------------------------------------
# animus llm complete: run a sub-LLM call
#   --prompt-file PATH    markdown prompt with {{ var }} placeholders
#   --var key=value       template variable substitution
#   --format yaml         parse output as YAML, return structured
#
# classify-inbound.md determines:
#   recall_memory: true|false — do we need memory retrieval?
#   has_action: true|false    — does message contain actionable tasks?
#   context: "dense semantic compression for downstream stages"

CLASSIFICATION=$(animus llm complete \
  --prompt-file "$SKILL_DIR/prompt/classify-inbound.md" \
  --var person="$PERSON" \
  --var message="$TEXT" \
  --var last_exchanges="$RECENT" \
  --format yaml)

RECALL_MEMORY=$(echo "$CLASSIFICATION" | yq -r '.recall_memory')
HAS_ACTION=$(echo "$CLASSIFICATION" | yq -r '.has_action')
CONTEXT=$(echo "$CLASSIFICATION" | yq -r '.context')

# Write classification to ledger for the act phase and consolidate
animus ledger append "$ANIMUS_WORK_ID" note \
  "classification: recall=$RECALL_MEMORY action=$HAS_ACTION context=$CONTEXT"

# ---------------------------------------------------------------------------
# 4. Extract tasks (if actionable)
# ---------------------------------------------------------------------------
# extract-task.md breaks down requests into structured tasks:
#   task, actionable, temporal, project, async flags
# Written to ledger as a finding so consolidate can check satisfaction later.

TASKS=""
if [ "$HAS_ACTION" = "true" ]; then
  TASKS=$(animus llm complete \
    --prompt-file "$SKILL_DIR/prompt/extract-task.md" \
    --var message="$TEXT" \
    --var context="$CONTEXT" \
    --var person="$PERSON" \
    --format yaml)

  animus ledger append "$ANIMUS_WORK_ID" finding \
    "extracted-tasks:
$TASKS"
fi

# ---------------------------------------------------------------------------
# 5. Formulate memory queries and recall
# ---------------------------------------------------------------------------
# formulate-queries.md generates 1-3 scoped semantic queries:
#   [person] query — search this person's relationship data
#   [general] query — search general knowledge base
#
# animus memory search: vector similarity search
#   --person NAME   scope to person-specific memories
#   --limit N       max results
#   --format text   readable text output

MEMORIES=""
if [ "$RECALL_MEMORY" = "true" ]; then
  QUERIES=$(animus llm complete \
    --prompt-file "$SKILL_DIR/prompt/formulate-queries.md" \
    --var message="$TEXT" \
    --var person="$PERSON" \
    --var context="$CONTEXT" \
    --format text)

  while IFS= read -r line; do
    [ -z "$line" ] && continue

    # Parse scope: [person] or [general]
    scope=$(echo "$line" | sed 's/\[//;s/\].*//')
    query=$(echo "$line" | sed 's/\[[^]]*\] *//')

    if [ "$scope" = "person" ]; then
      result=$(animus memory search "$query" --person "$PERSON" --limit 3 --format text 2>/dev/null || echo "")
    else
      result=$(animus memory search "$query" --limit 3 --format text 2>/dev/null || echo "")
    fi

    if [ -n "$result" ]; then
      MEMORIES="${MEMORIES}${result}
---
"
    fi
  done <<< "$QUERIES"
fi

# ---------------------------------------------------------------------------
# 6. Search relevant conversation history
# ---------------------------------------------------------------------------
# animus memory search-episodes with --query: find past conversations
# with this person that are topically relevant to the current message.

HISTORY=$(animus memory search-episodes \
  --person "$PERSON" \
  --query "$TEXT" \
  --limit 3 \
  --format text 2>/dev/null || echo "(no relevant history)")

# ---------------------------------------------------------------------------
# 7. Load relationship context
# ---------------------------------------------------------------------------
# animus rel show: display relationship data for a person
# In v1 this was: them.md, voice.md, quotes.md, mask.md, know.md
# In v2 these are stored in Postgres and returned as structured text.

RELATIONSHIP=$(animus rel show "$PERSON" --format text 2>/dev/null || echo "(no relationship data)")

# ---------------------------------------------------------------------------
# 8. Load identity context
# ---------------------------------------------------------------------------
# animus identity show: display the animus's self-knowledge
# In v1 this was: me.md, now.md

IDENTITY=$(animus identity show --format text 2>/dev/null || echo "(no identity data)")

# ---------------------------------------------------------------------------
# 9. Assemble context and write to ledger
# ---------------------------------------------------------------------------
# Everything the act phase needs is written as a single ledger entry.
# The act phase's system prompt includes ledger content, so this context
# becomes part of what the LLM sees when reading SKILL.md.

animus ledger append "$ANIMUS_WORK_ID" context "$(cat <<ORIENT_CONTEXT
## Identity

$IDENTITY

## Person: $PERSON

$RELATIONSHIP

## Inbound Message

$TEXT

## Classification

$CONTEXT

## Recalled Memories

${MEMORIES:-(no memories recalled)}

## Relevant History

$HISTORY

## Recent Exchanges

$RECENT
ORIENT_CONTEXT
)"
