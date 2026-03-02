---
name: check-satisfaction
description: Check if the response satisfied an extracted task
model: sonnet
---

# Check Task Satisfaction

Did the agent complete this task, or just acknowledge it?

## Task

{{ task }}

## Response

{{ response }}

## Thinking (if available)

{{ thinking }}

## Tool Use (if available)

{{ tools }}

## Output

```yaml
status: satisfied | acknowledged
reason: "brief explanation"
```

## Status Definitions

**satisfied** - The task was completed. The work product is in the response, OR the agent used tools to accomplish the task and the response reflects the result:
- Task: "tell me about Kelly" -> Response contains information about Kelly
- Task: "summarize the article" -> Response contains the summary
- Task: "get the transcript of this video" -> Agent used a tool to fetch it and the response discusses the content
- Task: "check the weather" -> Agent used a tool to fetch weather data and the response reports it

If the agent used tools to obtain or process the requested information and the response incorporates or discusses that result — that is **satisfied**, even if the raw tool output isn't reproduced verbatim in the response.

**acknowledged** - Response says it will be done later, or defers:
- "I'll look into that and get back to you"
- "I'll handle that in a bit"
- The response promises future action without delivering the result now

## Now check

**Task:** {{ task }}
**Response:** {{ response }}
**Tools used:** {{ tools }}
