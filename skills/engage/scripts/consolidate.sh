#!/usr/bin/env bash
set -euo pipefail
#
# consolidate.sh — Process results after the act phase for the engage skill.
#
# Ported from v1 post_engage (animus.rel.post.post_engage).
# Pipeline: check satisfaction → extract initiatives → detect episode
#           boundary → form episode → log exchange.
#
# Called by the control plane after the act phase completes.
# Environment variables provided by the engine:
#   ANIMUS_WORK_ID    — current work item ID
#   ANIMUS_SKILL      — "engage"
#   ANIMUS_FOCUS_DIR  — scratch directory for this focus
#   ANIMUS_PHASE      — "consolidate"
#
# CLI commands used below are SPECULATIVE — they define the interface
# the animus CLI needs to support. These scripts are "red" until the
# CLI is built.
#

SKILL_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# ---------------------------------------------------------------------------
# 0. Read work item params and act phase output
# ---------------------------------------------------------------------------

WORK_JSON=$(animus work show "$ANIMUS_WORK_ID" --json)
PERSON=$(echo "$WORK_JSON" | jq -r '.params.person')
TEXT=$(echo "$WORK_JSON" | jq -r '.params.text')

# The act phase's response — the message that was (or will be) sent.
# animus ledger read --type response: reads the agent's final text output
RESPONSE=$(animus ledger read "$ANIMUS_WORK_ID" --type response --last 1 --format text)

# The act phase's internal thinking (if captured by the engine).
# animus ledger read --type thinking: reads the agent's thinking trace
THINKING=$(animus ledger read "$ANIMUS_WORK_ID" --type thinking --format text 2>/dev/null || echo "")

# Tool usage summary from the act phase
TOOLS_USED=$(animus ledger read "$ANIMUS_WORK_ID" --type note --grep "tool:" --format text 2>/dev/null || echo "")

# ---------------------------------------------------------------------------
# 1. Check task satisfaction
# ---------------------------------------------------------------------------
# For each task extracted during orient, determine if the response
# actually completed it or just acknowledged it.
#
# check-satisfaction.md returns:
#   status: satisfied | acknowledged
#   reason: "brief explanation"
#
# Unsatisfied tasks get queued as follow-up work items.

TASKS=$(animus ledger read "$ANIMUS_WORK_ID" --type finding --grep "extracted-tasks" --format text 2>/dev/null || echo "")

if [ -n "$TASKS" ] && [ "$TASKS" != "(no entries)" ]; then
  # Parse each task and check satisfaction
  # In production this would iterate over structured YAML; simplified here
  SATISFACTION=$(animus llm complete \
    --prompt-file "$SKILL_DIR/prompt/check-satisfaction.md" \
    --var task="$TASKS" \
    --var response="$RESPONSE" \
    --var thinking="$THINKING" \
    --var tools="$TOOLS_USED" \
    --format yaml)

  STATUS=$(echo "$SATISFACTION" | yq -r '.status')

  if [ "$STATUS" = "acknowledged" ]; then
    # Task was acknowledged but not completed — queue as follow-up work.
    # The skill for follow-up depends on the task type.
    # For now, submit as a generic work item the human/control plane can route.
    animus work submit initiative "$PERSON" \
      --params "$(jq -n \
        --arg person "$PERSON" \
        --arg task "$TASKS" \
        --arg context "Acknowledged during engage but not satisfied" \
        '{person: $person, task: $task, context: $context}')" \
      --trigger "engage:unsatisfied-task"

    animus ledger append "$ANIMUS_WORK_ID" finding \
      "task-satisfaction: acknowledged — queued as follow-up"
  else
    animus ledger append "$ANIMUS_WORK_ID" finding \
      "task-satisfaction: satisfied"
  fi
fi

# ---------------------------------------------------------------------------
# 2. Extract self-generated initiatives
# ---------------------------------------------------------------------------
# extract-initiative.md analyzes the agent's thinking to find actions
# the agent decided to do unprompted — volition, not instruction.
#
# Returns: initiatives[] with action, target_person, impulse, reasoning,
#          scheduled_for (null = immediate, ISO datetime = future)
#
# Uses a more capable model (opus) for subtle intent extraction.

INITIATIVES=$(animus llm complete \
  --prompt-file "$SKILL_DIR/prompt/extract-initiative.md" \
  --var thinking="$THINKING" \
  --var response="$RESPONSE" \
  --var message="$TEXT" \
  --var person="$PERSON" \
  --var datetime_iso="$(date -u +%Y-%m-%dT%H:%M:%S)" \
  --model opus \
  --format yaml)

# Queue each initiative as a new work item
INITIATIVE_COUNT=$(echo "$INITIATIVES" | yq -r '.initiatives | length')
for i in $(seq 0 $((INITIATIVE_COUNT - 1))); do
  ACTION=$(echo "$INITIATIVES" | yq -r ".initiatives[$i].action")
  TARGET=$(echo "$INITIATIVES" | yq -r ".initiatives[$i].target_person")
  IMPULSE=$(echo "$INITIATIVES" | yq -r ".initiatives[$i].impulse")
  SCHEDULED=$(echo "$INITIATIVES" | yq -r ".initiatives[$i].scheduled_for")

  [ "$ACTION" = "null" ] || [ -z "$ACTION" ] && continue

  animus work submit initiative "$PERSON" \
    --params "$(jq -n \
      --arg action "$ACTION" \
      --arg target "$TARGET" \
      --arg impulse "$IMPULSE" \
      --arg scheduled "$SCHEDULED" \
      '{action: $action, target_person: $target, impulse: $impulse, scheduled_for: $scheduled}')" \
    --trigger "engage:self-initiative"

  animus ledger append "$ANIMUS_WORK_ID" finding \
    "initiative: $ACTION (target: $TARGET)"
done

# ---------------------------------------------------------------------------
# 3. Detect episode boundary
# ---------------------------------------------------------------------------
# detect-episode-boundary.md determines if the current message closes
# a conversational episode (time gap, farewell, topic exhaustion).
#
# If an episode closes, we form it and queue for memory extraction.

RECENT_MESSAGES=$(animus memory search-episodes \
  --person "$PERSON" \
  --limit 10 \
  --format text 2>/dev/null || echo "")

BOUNDARY=$(animus llm complete \
  --prompt-file "$SKILL_DIR/prompt/detect-episode-boundary.md" \
  --var current_message="$TEXT" \
  --var person="$PERSON" \
  --var recent_messages="$RECENT_MESSAGES" \
  --format yaml)

CLOSES_EPISODE=$(echo "$BOUNDARY" | yq -r '.closes_episode')
BOUNDARY_REASON=$(echo "$BOUNDARY" | yq -r '.reason')

if [ "$CLOSES_EPISODE" = "true" ]; then
  # Form the episode: bundle messages, generate summary, store as indexed memory
  # animus memory store-episode: creates an episode record from the conversation
  animus memory store-episode \
    --person "$PERSON" \
    --work-id "$ANIMUS_WORK_ID" \
    --reason "$BOUNDARY_REASON"

  # Queue for radiation (knowledge extraction and embedding)
  # In v1 this was the radiate DC — here it's a work item with the radiate skill
  animus work submit radiate "$PERSON" \
    --params "$(jq -n \
      --arg person "$PERSON" \
      --arg work_id "$ANIMUS_WORK_ID" \
      --arg reason "$BOUNDARY_REASON" \
      '{person: $person, source_work_id: $work_id, episode_reason: $reason}')" \
    --trigger "engage:episode-closed"

  animus ledger append "$ANIMUS_WORK_ID" finding \
    "episode-closed: $BOUNDARY_REASON — queued for radiation"
fi

# ---------------------------------------------------------------------------
# 4. Log the exchange
# ---------------------------------------------------------------------------
# Always log the message pair for conversation continuity.
# animus memory log-exchange: stores the inbound/outbound pair
# with timestamps, person, and work item reference.

animus memory log-exchange \
  --person "$PERSON" \
  --inbound "$TEXT" \
  --response "$RESPONSE" \
  --work-id "$ANIMUS_WORK_ID"

# ---------------------------------------------------------------------------
# 5. Write consolidate outcome
# ---------------------------------------------------------------------------
# Summary of what consolidate did, written to the focus directory
# for the control plane to read as the work item outcome.

cat > "$ANIMUS_FOCUS_DIR/consolidate-out.json" <<OUTCOME
{
  "verdict": "pass",
  "person": "$PERSON",
  "episode_closed": $CLOSES_EPISODE,
  "initiatives_queued": $INITIATIVE_COUNT,
  "tasks_checked": $([ -n "$TASKS" ] && echo "true" || echo "false")
}
OUTCOME
