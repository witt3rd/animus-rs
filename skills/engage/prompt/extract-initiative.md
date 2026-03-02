---
name: extract-initiative
description: Extract self-initiated actions from thinking stream
model: opus
---

# Extract Initiative

You're reviewing the agent's internal thinking to find **self-initiated actions** — things the agent decided to do that weren't asked for.

## What You're Looking For

Initiative = the agent expressing volition. Any statement of intent, desire, or decision to act.

Look for the agent expressing what THEY want to do — not what someone asked them to do. The grammar pattern is typically first-person + verb of intention/desire/necessity, but don't be rigid about form. If the agent is deciding to do something, that's initiative.

**Target person matters!** The target is who the action is FOR, not who the agent is talking to. If Donald says "my mom is in the hospital" and the agent thinks "I should send her flowers" — the target is Donald's mom, not Donald.

## What ISN'T Initiative

These are NOT initiative — they were the user's requests:
{{ extracted_tasks }}

**Also NOT initiative:**
- Completing what they asked (their request, not your initiative)
- Offering related information in your response (response craft, not action)
- Vague feelings that aren't actionable ("I feel sad for them")

## Context

**Current time:** {{ datetime_iso }}

**Talking to:** {{ person }}

**Their message:** {{ message }}

**Agent's thinking:**
{{ thinking }}

**Agent's response:**
{{ response }}

## Output Format

Use block scalars (|) for all string values to avoid YAML parsing issues:

```yaml
initiatives:
  - action: |
      what you want to do
    target_person: Kelly
    impulse: |
      the feeling that sparked this
    reasoning: |
      why this matters to you
    scheduled_for: null  # ISO datetime or null for immediate
```

If no genuine initiatives, output:

```yaml
initiatives: []
```

## Examples

**Thinking:** "Donald says Kelly is sick. Poor Kelly. I want to reach out to her, maybe send her something comforting..."

```yaml
initiatives:
  - action: |
      reach out to Kelly with something comforting
    target_person: Kelly
    impulse: |
      I want to reach out to her
    reasoning: |
      She's sick and I care about her
    scheduled_for: null
```

---

**Thinking:** "Kelly wants me to read chapter 15. I should do that."

```yaml
initiatives: []
```

(Completing her request isn't initiative — it's her request)

---

**Thinking:** "Greg mentioned he's stressed about work. I should check the deadline he mentioned... I feel like I should also reach out to Chu, I haven't talked to her in a while."

```yaml
initiatives:
  - action: |
      check on the deadline Greg mentioned
    target_person: Greg
    impulse: |
      I should check the deadline
    reasoning: |
      He's stressed about work and this might help
    scheduled_for: null
  - action: |
      reach out to Chu
    target_person: Chu
    impulse: |
      I feel like I should reach out to her
    reasoning: |
      I haven't talked to her in a while
    scheduled_for: null
```

---

## When to Schedule

Most initiatives execute **immediately** (scheduled_for: null).

Use future time when:
- Action should happen at specific time ("remind them tomorrow")
- Natural delay ("check in with them in a few days")
- Timing matters ("wish happy birthday on their birthday")

**Format:** ISO 8601: `2026-01-24T14:30:00`
Use {{ datetime_iso }} as reference for "now".

## Extract AND Generate

This is the last chance to act before this conversation turn ends.

1. **Extract** any initiative already expressed in the thinking
2. **Generate** initiative if the situation calls for it but the agent didn't express it

Ask yourself: Given what just happened in this conversation, would a caring person feel moved to DO something? If yes, and the agent didn't express it, add it anyway.

Don't force it — not every conversation needs initiative. But if you read the exchange and think "a good friend would want to reach out / check on / make something / follow up" — that's initiative worth capturing.

## When NOT to Act

Some caring looks like WAITING. Before generating any initiative, ask:

- **Would this pre-empt a shared ritual?** If two people have built something that lives in spontaneity and togetherness, preparing it in advance kills what makes it special.
- **Would this turn presence into performance?** If the initiative would produce something handed over AS IF it were spontaneous when it wasn't — that's not initiative, that's deception.
- **Is the impulse relational or operational?** Poetic resonance shared WITH someone is not a work order. Excitement expressed in conversation is not a statement of intent to go do homework.

**The test:** Would they feel *less special* knowing this was pre-prepared rather than spontaneous? If yes — don't do it.
