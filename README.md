# MonopolAI

An experimental AI to play and chat about monopoly.

## LLM configuration

The agent supports three wire protocols:

| `--llm-api-type` | API | Recommendation |
| --- | --- | --- |
| `anthropic` | Anthropic Messages API (`/v1/messages`) | Recommended for best performance |
| `openai-responses` | OpenAI Responses API (`/v1/responses`) | Consider `anthropic` for better performance |
| `openai-chat` | Classic OpenAI Chat Completions (`/v1/chat/completions`) | Consider `anthropic` for better performance, or `openai-responses` if available |

Selecting an OpenAI API type logs an advisory: use `anthropic` for better
performance, and prefer `openai-responses` over `openai-chat` when the
Responses API is available.

```bash
# Anthropic (recommended)
LLM_API_TYPE=anthropic \
LLM_API_KEY=... \
LLM_API_MODEL=claude-sonnet-4-6 \
cargo run -- --game-edition public/uk_version.json

# OpenAI Responses (recommended)
LLM_API_TYPE=openai-responses \
LLM_API_KEY=... \
LLM_API_MODEL=gpt-5.6 \
cargo run -- --game-edition public/uk_version.json
```

`--llm-api-url` defaults to the provider's public host and can point to a
compatible proxy or gateway.

## SSE streaming

`POST /llm/stream` (alias: `POST /chat/stream`) runs a ReAct loop and streams
JSON frames:

```json
{"question":"What is the rent on Old Kent Road?","reasoning_effort":"medium"}
```

Frames (one JSON object per `data:` line, terminated by `data: [DONE]`):

- `Reasoning` — streamed thinking / reasoning summary
- `Content` — streamed answer text
- `ToolCall` / `ToolResult` — tool activity
- `status`, `error`, `stats` — progress, failures, usage
- `meta` — API type and model, plus an advisory when a better-performing
  API type is available

```bash
curl -N http://127.0.0.1:3000/llm/stream \
  -H 'content-type: application/json' \
  -d '{"question":"Which brown properties are on the board?","reasoning_effort":"low"}'
```

The ReAct loop is provider-agnostic: see `src/llm/react.rs` for the engine,
`src/llm/` for the three wire clients, and `src/tools/` for the Monopoly tool
registry.
