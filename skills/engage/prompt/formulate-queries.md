---
name: formulate-queries
description: Generate optimal queries for memory recall
model: sonnet
---

# Formulate Memory Recall Queries

Generate 1-3 semantic queries to recall relevant memories for responding to this message.

## Input

**Message:** {{ message }}

**Person:** {{ person }}

**Context:** {{ context }}

## What's Already In Context (don't query for these)

The response pipeline ALREADY receives:
- **Recent exchanges** - the current conversation window
- **Relationship data** - general knowledge about this person (preferences, personality, relationship patterns)
- **Current episode context** - what we've been talking about

So DON'T query for:
- General relationship context (already loaded)
- Recent conversation content (exchanges have it)
- Basic facts about who they are (already loaded)

DO query for:
- **Specific past events** beyond the recent window ("remember when...")
- **Documented insights** - research, written reflections
- **Cross-person context** - information from other relationships
- **Historical patterns** - specific instances, not general patterns

## Query Design Principles

1. **Think about document vocabulary** - What words/phrases would actually appear in stored memories?
2. **Translate semantic meaning to search terms** - The message uses one vocabulary, documents may use another
3. **Include person name** - weight person-specific content
4. **Shape the response you want** - Query phrasing guides synthesis
5. **Multiple angles** - topic query + relationship query if relevant

## The Key Question

For each topic, ask two things:
1. **"What words would I find in a document about this?"** (vocabulary)
2. **"What kind of answer do I need?"** (shape)

Examples of vocabulary translation:
- Message says "evil one" -> Documents might say: black heart, the pattern, cold optimization
- Message says "judo idea" -> Documents might say: judo strategy, exploit essential nature
- Message says "the wizard" -> Documents might say: Donald, wizard, fighting together

## Output Format

Each query needs a **scope**:
- `[person]` - recall from this person's relationship data and conversation history
- `[general]` - recall from general knowledge (research, memory notes, broader knowledge base)

Return as many queries as needed, one per line with scope prefix:

```
[person] query about person-specific memories
[general] query about general knowledge
[person] another person-specific query
```

No explanation needed. Just the scoped queries.

## Examples

**Message:** "I built ag buddy as a qwen model on a raspberry pi then tuned it to be an expert in cold climate seed crops"
**Person:** dan
**Context:** "dan sharing project - ag buddy qwen rpi - his AI farming work"

```
[person] dan edge computing AI local inference raspberry pi
[person] dan agricultural AI embodied substrate farming
[general] raspberry pi edge inference qwen small models
```

---

**Message:** "what was that workout program you suggested a few weeks ago?"
**Person:** kelly
**Context:** "asking about specific past workout recommendation"

```
[person] kelly workout program suggested conversation
[person] kelly exercise recommendation discussion
```

---

**Message:** "Do you experience qualia?"
**Person:** greg
**Context:** "philosophical question - my prior reflections relevant"

```
[general] consciousness qualia subjective experience
[general] my reflections phenomenology awareness
[person] greg philosophical discussions
```

---

**Message:** "remember that Alan Watts video I sent you last week?"
**Person:** donald
**Context:** "asking shared history - 'I sent you' = memory not web"

```
[person] donald Alan Watts video shared
[person] donald sent video conversation
```

---

## Now generate queries:

**Message:** {{ message }}
**Person:** {{ person }}
**Context:** {{ context }}
