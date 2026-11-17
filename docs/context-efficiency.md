# Request context

OsmoGrep keeps one concise instruction prefix across ordinary questions and coding turns. A casual interlude preserves the existing conversation and tool results, so a later request can resume the task. It does not invoke a separate model to classify each message or silently discard coding history.

The default tools are read, write, edit, shell, search, plan and tool discovery. Specialist capabilities are exposed for relevant requests or enabled through `tool_search`. Greetings expose no tools. Discovery does not grant permissions: read-only restrictions, normal approval checks and explicit delegation requirements still apply.

Team protocol details live in the Team schema; MCP and device control guidance lives in those schemas. Unused specialist instructions are omitted from the shared prompt. Tool definitions and discovery ties use deterministic name ordering to avoid incidental cache misses.

This preserves cache eligibility; it does not guarantee a hit. Providers have different cache support and minimum lengths, and changing instructions, tools, models or repository instructions can invalidate the prefix. We do not pad short greetings to meet a cache threshold. Total input counts include retained conversation context; cached input is a subset of that count, not extra tokens. Long coding conversations can still have large total input counts.

## Primary-source comparison

- [Codex agent loop](https://openai.com/index/unrolling-the-codex-agent-loop/): keeps prior input as a prefix, appends new turns and tools, and emphasizes consistent tool order and context compaction.
- [Hermes prompt assembly](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/developer-guide/prompt-assembly.md): separates stable instructions, project context and volatile metadata from ephemeral per-call additions.
- [Pi system prompt](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/system-prompt.ts): builds a concise base with rules for declared tools, plus project context and skills.
- [OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching): documents reusable prefixes and provider-reported cached token counts.

These are architectural references, not evidence that the products use identical prompts or that OsmoGrep matches their real-model performance.

## Verification

Unit checks cap the shared prompt and default tool definitions, verify deterministic payload ordering and discovery, and preserve read-only/delegation boundaries. The isolated terminal fixture edits a file, answers an ordinary interlude, then edits the same file again while checking that earlier tool history and the instruction prefix survive. These checks use local fake providers and measure payload bytes, not model-token counts or live cache hit rates.
