#!/usr/bin/env bash
set -euo pipefail
#
# recover.sh — Handle failures during the engage skill lifecycle.
#
# Called by the control plane when orient or act fails.
# Decides whether to retry or dead-letter the work item.
#
# Environment variables provided by the engine:
#   ANIMUS_WORK_ID    — current work item ID
#   ANIMUS_SKILL      — "engage"
#   ANIMUS_FOCUS_DIR  — scratch directory for this focus
#   ANIMUS_PHASE      — "recover"
#
# The engine also provides (via focus context):
#   ANIMUS_FAILED_PHASE  — which phase failed ("orient" or "act")
#   ANIMUS_ATTEMPT       — current attempt number
#   ANIMUS_MAX_ATTEMPTS  — max attempts from skill metadata
#

WORK_JSON=$(animus work show "$ANIMUS_WORK_ID" --json)
PERSON=$(echo "$WORK_JSON" | jq -r '.params.person')
ATTEMPT=${ANIMUS_ATTEMPT:-1}
MAX_ATTEMPTS=${ANIMUS_MAX_ATTEMPTS:-2}
FAILED_PHASE=${ANIMUS_FAILED_PHASE:-"unknown"}

# Log the failure
animus ledger append "$ANIMUS_WORK_ID" error \
  "Recovery attempt $ATTEMPT/$MAX_ATTEMPTS after $FAILED_PHASE failure"

if [ "$ATTEMPT" -ge "$MAX_ATTEMPTS" ]; then
  # Exhausted retries — dead-letter
  animus ledger append "$ANIMUS_WORK_ID" error \
    "Max attempts exhausted. Dead-lettering engage for $PERSON."

  # Write outcome indicating failure
  cat > "$ANIMUS_FOCUS_DIR/consolidate-out.json" <<OUTCOME
{
  "verdict": "fail",
  "person": "$PERSON",
  "reason": "max_attempts_exhausted",
  "failed_phase": "$FAILED_PHASE",
  "attempts": $ATTEMPT
}
OUTCOME

  exit 1  # signal to control plane: dead-letter this work item
fi

# Still have retries — signal to control plane: requeue
animus ledger append "$ANIMUS_WORK_ID" note \
  "Retrying ($ATTEMPT/$MAX_ATTEMPTS) after $FAILED_PHASE failure"

exit 0  # signal to control plane: requeue for retry
