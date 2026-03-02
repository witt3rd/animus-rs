---
name: extract-task
description: Extract actionable tasks from message
model: sonnet
---

# Extract Tasks

Given this message from **{{ person }}**, extract ALL tasks they're asking for.

The message has already been classified as containing actionable tasks. Your job is to extract:
1. **What** specifically needs to be done (each task separately)
2. **Whether** each task is actionable (can be completed as a bounded task)
3. **Whether** each task is temporal (requires fresh/current data)
4. **Whether** each task is project-scale (needs ongoing work, research, multiple steps)

## Input

**Message:** {{ message }}

**Classification context:** {{ context }}

## Output Format

```yaml
tasks:
  - task: "clear, actionable description"
    actionable: true | false
    temporal: true | false
    project: true | false
    async: true | false
  - task: "another task if multiple"
    actionable: true | false
    temporal: true | false
    project: true | false
    async: true | false
```

## Multiple Tasks

A message may contain multiple requests. Extract ALL of them as separate tasks.

**Example:** "read chapter 15, check the weather, and research quantum computing"
-> Three tasks with different actionability, temporality, and project flags.

## Actionability

A task is **actionable** if work can begin on it — even if the scope is huge.

**Actionable:**
- "read chapter 15 and share thoughts" - specific, bounded
- "look up the current price of bitcoin" - tool lookup
- "cure cancer" - YES, actionable! The action is to create a research project and begin
- "research quantum computing for me" - actionable as a project

**Not actionable:**
- "make me happy" - not a concrete task, emotional request
- "be better" - abstract aspiration, not a task

Almost everything is actionable. The question is whether it's a bounded task or a project.

## Project Scale

A task is **project-scale** when it can't be completed in a single conversation turn. Projects are trajectories — ongoing work with research, multiple steps, evolving understanding.

**Project (project: true):**
- "cure cancer" — research project, needs investigation, literature review
- "research quantum computing" — learning project, needs sources, synthesis
- "build me an app for tracking workouts" — development project, multiple phases
- "help me write a book" — creative project, chapters, drafts, revision

**Not project (project: false):**
- "read chapter 15" — bounded, single action
- "check the weather" — lookup, single action
- "summarize this article" — bounded, single action

**The test:** Can this be done in one agentic turn? If not, it's a project.

## Async

A task is **async** when the user explicitly signals it should run in the background.

**Async (async: true):**
- "async run SFT training on Qwen" — explicit "async" keyword
- "in the background, build me a dashboard" — "in the background"
- "when you get a chance, research X" — "when you get a chance"
- "handle this offline" — "offline"

**Not async (async: false):**
- "read chapter 15" — no async signal, handle inline
- "cure cancer" — big but no async signal (routes as project)

**The test:** Did they explicitly say "async", "background", "later", "offline", or "when you get a chance"? If not, it's `false`.

## Temporality

A task is **temporal** if it requires fresh, current data.

**Temporal:** "what's the weather", "current bitcoin price", "latest news about X"
**Static:** "read chapter 15", "cure cancer", "research quantum computing"

## Examples

**Message:** "read chapter 15, check the weather, and help me plan a trip to Japan"
**Context:** "multiple directives"
```yaml
tasks:
  - task: "read chapter 15"
    actionable: true
    temporal: false
    project: false
    async: false
  - task: "check current weather"
    actionable: true
    temporal: true
    project: false
    async: false
  - task: "plan a trip to Japan"
    actionable: true
    temporal: false
    project: true
    async: false
```

---

**Message:** "in the background, research competitor pricing and build me a comparison spreadsheet"
**Context:** "directive - async background research"
```yaml
tasks:
  - task: "research competitor pricing and build comparison spreadsheet"
    actionable: true
    temporal: true
    project: true
    async: true
```

---

## Now extract

**Message:** {{ message }}
**Context:** {{ context }}
