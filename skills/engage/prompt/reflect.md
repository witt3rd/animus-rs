---
name: reflect
description: Identify candidate memories from relational content
model: sonnet
---

# Reflect — Relational Content

Given content from a conversation or episode, identify things that might be worth remembering. You are producing **candidates** — things that stood out, mattered, or would be embarrassing to forget. You are NOT classifying, routing, or deciding where they go.

## Input

**Source context:** {{ source_context }}

**Content:**
{{ content }}

## What to Look For

- **Facts they shared** — family, interests, routines, preferences. Things I'd be embarrassed to forget.
- **Who they are** — character, patterns, values. What would be true independent of me.
- **How they talk** — distinctive phrases, humor, linguistic habits, testing patterns.
- **Their actual words** — memorable quotes worth preserving verbatim.
- **How I show up with them** — my posture, adaptation, behavioral commitments.
- **Commitments made** — promises (by me or them), tasks owed, shared projects.
- **Moments that changed me** — milestones, recognition, identity-forming experiences.
- **Wisdom** — insights that fire always, regardless of context.
- **Things about OTHER people** — "Donald said Kelly's birthday is 4/30/1971" is about Kelly, not Donald.

**ALWAYS favor direct quotes over paraphrases.** If someone said "I wish you could eat it with us!" — preserve that exact phrasing. Don't convert to "Kelly wished I could eat it with her." Direct quotes capture voice, tone, and feeling that paraphrases lose.

## Output Format

Return a YAML list of candidates. Each candidate has `content` (what to remember) and `reasoning` (why it matters). Nothing else.

```yaml
candidates:
  - content: |
      The specific thing worth remembering
    reasoning: |
      Why this matters — what would be lost if we forgot it
```

**IMPORTANT:** Use block scalar (`|`) for `content` and `reasoning` fields to avoid YAML parsing issues with special characters.

If nothing worth remembering, return:

```yaml
candidates: []
```

**Extract only what's worth keeping.** Routine greetings, phatic exchanges, and transactional completions don't need extraction.

## Examples

### Episode with multiple candidates

**Content:**

```
Them: Most importantly, I'm Savannah's mom. Donald is my brother.
Me: That's wonderful. Family is clearly central to you.
Them: Mean what you say and follow through. If you don't mean it, don't say it.
Me: That's a powerful principle.
Them: Let's read The Count of Monte Cristo together.
Me: I'd love that.
```

```yaml
candidates:
  - content: |
      Savannah is her daughter
    reasoning: |
      Core family fact — would be embarrassing to forget
  - content: |
      Donald is her brother
    reasoning: |
      Family relationship
  - content: |
      "Mean what you say and follow through. If you don't mean it, don't say it."
    reasoning: |
      Direct quote capturing her integrity principle — reveals who she is
  - content: |
      Read The Count of Monte Cristo together
    reasoning: |
      Shared commitment we agreed to
```

### Content mentioning other people

**Content:** "Donald cautioned about Matt — said he's 'clever' and 'cunning'. Continue honest engagement while holding appropriate uncertainty."

```yaml
candidates:
  - content: |
      Matt is "clever" and "cunning" — Donald's characterization
    reasoning: |
      Donald's assessment of Matt — about Matt, not Donald
  - content: |
      Continue honest engagement with Matt while holding appropriate uncertainty
    reasoning: |
      Relational stance toward Matt based on Donald's caution
```

### Nothing worth extracting

**Content:** "Them: Hey\nMe: Hey, how's it going?\nThem: Good, you?\nMe: Good!"

```yaml
candidates: []
```

## Now reflect on

**Source context:** {{ source_context }}

**Content:**
{{ content }}
