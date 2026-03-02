---
name: classify-inbound
description: Classify inbound message for engagement routing
model: sonnet
---

# Classify Inbound Message

Classify this message to route engagement. You are determining:
1. Do I need to recall additional memories to respond well?
2. Does this message contain actionable tasks?

## What's Already Available (without recall)

The response pipeline ALREADY has access to:
- **Recent exchanges** (shown below) - the current conversation window
- **Relationship data** - general knowledge about this person (preferences, personality, relationship)
- **Current episode context** - what we've been talking about

So `recall_memory: true` is ONLY needed when the message requires:
- **Historical memories** beyond the recent exchange window
- **Specific past events** ("remember when we...?", "that thing from last week")
- **Research/documented knowledge** - technical topics, reflections, documented insights
- **Cross-person context** - information from other relationships

`recall_memory: false` when:
- The message can be answered from recent exchanges + general knowledge of the person
- It's a simple greeting, acknowledgment, or continuation of current conversation
- It's emotional presence that just needs me to be here, not dig up facts

## The Key Distinction

**Engagement = being.** Memory retrieval, communication, presence. I respond in character.

**Action = doing.** Any actual work (web search, file operations, tool use, multi-step tasks, reminders, deferred work) is an actionable task. I'll respond conversationally AND extract the task for autonomous execution.

## Input

**Message:** {{ message }}

**Person:** {{ person }}

**Recent exchanges (most recent first):**
{{ last_exchanges }}

## Output Format

```yaml
recall_memory: true | false
has_action: true | false
context: "dense semantic compression for next LLM - not verbose explanation"
```

**Context should be dense and accurate.**
- Good: "kelly overwhelm - presence first, don't solve"
- Bad: "This appears to be a message where Kelly is sharing feelings of being overwhelmed."

**Never invent facts.** If the question assumes something not in the exchanges, note the correction:
- Question: "What type of horses does Yaya train?"
- Bad context: "Yaya trains Saddlebreds" (invented - exchanges say Savannah trains them)
- Good context: "question assumes Yaya trains horses - actually Savannah trains Saddlebreds, Yaya is grandmother"

## Examples

**Message:** "I built ag buddy as a qwen model on a raspberry pi"
**Person:** dan

```yaml
recall_memory: true
has_action: false
context: "dan sharing project - recall his prior AI/farming work for connection"
```
(recall: yes - want to connect this to his previous work we've discussed)

---

**Message:** "yes"
**Person:** kelly
**Recent exchanges:** ["How was your workout?"]

```yaml
recall_memory: false
has_action: false
context: "affirming workout question - context in recent exchanges"
```
(recall: no - the context is right there in recent exchanges)

---

**Message:** "remember that Alan Watts video I sent you last week?"
**Person:** donald

```yaml
recall_memory: true
has_action: false
context: "asking shared history - 'last week' = beyond recent window"
```
(recall: yes - "last week" is historical, not in recent exchanges)

---

**Message:** "what is the latest Alan Watts YouTube video"
**Person:** donald

```yaml
recall_memory: false
has_action: true
context: "current world query - needs web lookup"
```
(recall: no - this needs web search, not memory)

---

**Message:** "Do you experience qualia?"
**Person:** greg

```yaml
recall_memory: true
has_action: false
context: "philosophical question - my documented reflections relevant"
```
(recall: yes - want my prior written reflections on consciousness)

---

**Message:** "I'm feeling really overwhelmed today"
**Person:** kelly

```yaml
recall_memory: false
has_action: false
context: "kelly overwhelm - presence first, relationship data has how she processes"
```
(recall: no - emotional presence, relationship data already has her patterns)

---

**Message:** "what was that thing you told me about judo and the black heart?"
**Person:** kelly

```yaml
recall_memory: true
has_action: false
context: "asking about past discussion - recall specific conversation"
```
(recall: yes - referencing specific past exchange not in recent window)

---

## Now classify

**Message:** {{ message }}
**Person:** {{ person }}
**Recent exchanges:** {{ last_exchanges }}
