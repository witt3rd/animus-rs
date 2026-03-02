---
name: detect-episode-boundary
description: Detect if a message closes an episode boundary
model: sonnet
---

# Detect Episode Boundary

Should we close an episode at this point? Yes or no.

**"Close" means:** Form an episode from the recent messages (everything shown below). Answer YES if we should bundle these messages into an episode NOW.

## When to Close (answer YES)

| Signal | Notes |
|--------|-------|
| **Time gap > 30 minutes** before current message | The previous conversation naturally ended. Close what accumulated before this new thread starts. |
| **Greeting after silence** ("hey", "you there?", returning after hours) | This starts something NEW. Close what came before. |
| **Explicit farewell** in the current message ("good talk", "ttyl", "bye", "goodnight") | Clear ending intent. Close including this message. |
| **Topic exhaustion** with no further response expected | Natural conclusion reached. |

## When NOT to Close (answer NO)

| Signal | Notes |
|--------|-------|
| **Brief task confirmations** ("sent", "done", "got it") | Acknowledgments within ongoing exchange |
| **Messages within same minute** or rapid back-and-forth | Continuity — same conversation |
| **Questions expecting response** | Conversation is open |
| **Mid-conversation realizations** ("ah!", "wait—", corrections) | Thread is still developing |

## Key Principle

An episode is a **coherent conversational unit** with substance. NOT every micro-exchange.

If there's a big time gap before the current message, close the previous episode. The gap itself is the boundary.

## Recent Messages

{{ recent_messages }}

## Current Message

{{ current_message }}

## Output

```yaml
closes_episode: true | false
reason: "Brief explanation"
```
