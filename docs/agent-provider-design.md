# Agent and provider source review

Reviewed the local Codex, pi, and Hermes source snapshots on 2026-11-15 to repair OsmoGrep's conversation and API boundaries. This records the parts inspected, their consequences, and the implemented scope. It is not a claim to have audited every file in those repositories.

## Source evidence

All three reference directories are siblings of this repository in the local workspace.

| Source snapshot | Files inspected | Useful behavior |
| --- | --- | --- |
| pi, `@earendil-works/pi-ai` 0.80.6; local snapshot has no Git metadata | `packages/agent/src/agent-loop.ts`; `packages/ai/src/{compat.ts,types.ts,auth/resolve.ts}`; `packages/ai/src/api/{transform-messages,openai-completions,anthropic-messages,openai-responses}.ts` | Agent messages are separate from wire messages. A provider has its own API and auth resolution. Streaming constructs a complete assistant message; tool calls and their results remain paired. Explicit steering and follow-ups enter the loop at different boundaries. Truncated tool arguments must not execute. |
| Hermes 0.18.2; local snapshot has no Git metadata | `run_agent.py`; `agent/{conversation_loop,auxiliary_client}.py`; `agent/transports/{base,types,chat_completions,anthropic,__init__}.py` | The transport converts messages/tools and normalizes responses; it does not own tool execution. Strict providers reject internal metadata. DeepSeek-style reasoning fields and Gemini tool signatures need replay. Anthropic thinking signatures require exact block ordering, including thinking interleaved with tools. |
| Codex `2dae757b8` | `codex-rs/core/src/client.rs`; `codex-rs/model-provider-info/src/lib.rs`; `codex-rs/codex-api/src/sse/responses.rs` | Request configuration and connection state have explicit lifetimes. A closed stream without its completion event is an error. Cancellation participates while awaiting stream events. Auth and retry policy are transport concerns. This snapshot supports Responses wire format and explicitly rejects the old Chat setting; it is not a general Anthropic/Chat adapter to copy wholesale. |

## What was broken here

OsmoGrep's provider setting changed the endpoint and key name, but the main loop and scoped calls always posted Responses payloads to `/responses`. Groq, Mistral and local Chat-compatible servers therefore received the wrong protocol. Native Anthropic headers, system messages, image blocks, tool definitions, tool results and usage were absent. The non-streaming path also put credentials and the full payload in `curl` arguments.

The preceding repair removed implicit test/review scheduling, stale system instructions and forced verification follow-ups; separated queued turns from steering; preserved conversation on errors; and kept Escape from exiting the application. Those fixes are retained. The provider work does not add another planner or another agent loop.

## Implemented boundary

`src/provider.rs` translates at the existing Rust model boundary. The existing permission checks, tool registry, read-only parallelism, transcript events and cancellation token remain the authorities. No reference agent runtime is embedded. The native MCP client uses the official rmcp SDK; provider requests use cancellable async reqwest.

* OpenAI defaults to Responses; Anthropic to native Messages; Groq, Mistral, OpenRouter, DeepSeek, xAI, Ollama and LM Studio to Chat Completions. A custom provider requires an explicit base URL and uses `OSMOGREP_API_KEY` unless `api_key_env` is configured.
* Both the main loop and scoped calls use this boundary. Model discovery uses protocol-appropriate authentication. The model settings retain protocol selection alongside endpoint/provider/model.
* Text, inline images, schemas, tool IDs and results are translated to each wire format. Within a tool run the original native assistant message is replayed, preserving ordered signed thinking blocks, reasoning content and per-call metadata. The presentation receives only normalized text/tools.
* SSE handles UTF-8 across network reads, CRLF, optional spaces after `data:`, multiline data, indexed argument fragments, final usage and explicit completion/error events. Limits bound response/event memory. Incomplete output, duplicate call IDs and malformed arguments are rejected before any call in that response executes.
* Keys stay in HTTP headers in Rust, outside the renderer and process command line. Redirects are disabled, embedded endpoint credentials are rejected, saved config permissions are restricted on Unix, and known keys are redacted from provider error messages.
* Anthropic billed input includes uncached input plus cache reads and writes. Cached input remains a subset, so the existing charts do not count it twice. Chat token fields remain compatible with the existing usage ledger.
* Only connection failures, HTTP 429 and server errors may retry, before any output is displayed. Authentication/validation failures and interrupted partial streams are surfaced directly. Cancellation drops the async request/socket even when the provider is silent.

## System prompt review

Also inspected pi's `packages/coding-agent/src/core/{system-prompt,agent-session}.ts`, Hermes' `agent/{system_prompt,prompt_builder}.py`, and Codex's `core/{gpt_5_2_prompt,gpt-5.2-codex_prompt}.md` plus `core/src/context/base_instructions.rs`.

Pi builds a compact identity, tool-aware guidelines, optional custom instructions, project files, skills, and finally date/cwd. Hermes separates stable identity/tool guidance from project context and volatile memory/session context, caching the assembled prompt. It also conditionally injects stronger tool-use enforcement for some model families. Codex separates base/developer instructions and repository context, with explicit differences between discussion, questions and implementation requests. These are source snapshot observations, not promises about every version or model.

OsmoGrep's base prompt is now reviewable in `src/prompts/agent.md`. Its sections cover current intent, conditional tool use, scope/evidence, and communication. Ordinary conversation, explanation, rewriting and brainstorming may finish without tools even when schemas are available. Workspace-specific questions use relevant reads; implementation requests get actual action. The small exact-greeting fast path remains an optimization, not a classifier for every other message. Other requests retain tool availability and the model chooses based on their meaning. There is no extra routing-model call or growing keyword blacklist.

Project facts are explicitly reference data rather than a task list. Repository instructions and selected skill/steer content are assembled separately, freshly per user turn so old task instructions cannot become permanent system instructions. Applicable nested directory instructions are added before a mutation; newly discovered rules require the model to review and reissue that call. This deliberately differs from Hermes' whole-session caching: correctness when the selected skills or repository instructions change takes priority over retaining stale context. Tool execution still passes through the existing permissions regardless of what a prompt asks the model to do.

## Configuration

Existing OpenAI configuration remains valid. Provider/model selectors infer the normal API. Model settings include **API protocol**, which cycles through auto, Responses, Chat Completions and Anthropic Messages; choose Apply model to save it.

For a custom gateway:

```text
/model custom model-id https://gateway.example/v1 --api=chat-completions
```

`--api=auto` restores provider inference. Alternatively, set `OSMOGREP_API` or the optional `api` field in the `[model]` configuration. Values are `responses`, `chat-completions`, and `anthropic-messages`. `base_url` is the API root, without the final `/responses`, `/messages`, or `/chat/completions` suffix.

Anthropic uses `ANTHROPIC_API_KEY`, OpenRouter `OPENROUTER_API_KEY`, DeepSeek `DEEPSEEK_API_KEY`, xAI `XAI_API_KEY`; the existing provider key names remain unchanged. Ollama and LM Studio do not require a key. A provider switch never borrows another provider's saved key.

## Verification and deliberate limits

`src/provider_tests.rs` checks conversion, reasoning/signature replay across parallel calls, cache accounting, invalid/truncated output, real HTTP headers/paths, fragmented UTF-8 SSE, errors and silent-server cancellation. `terminal-ui/scripts/provider-acceptance.py` runs the actual agent with real file tools against Chat and Anthropic fixtures in streaming and JSON modes. The existing Mac acceptance suite covers Responses and the UI path. The opt-in paid-provider script `terminal-ui/scripts/provider-live-acceptance.py` checks a greeting, arithmetic, supplied-code explanation, rewriting, and an actual temporary file read. The first four must use zero tools; the last must use read_file. Live-provider checks are recorded separately from mocks.

New saved API keys and OAuth tokens use OS-protected storage. Configured, user-registered OAuth device flows support pending/slow-down, expiry, refresh locking, and token rotation. This does not import another agent's credentials or provide consumer subscription login. Native Gemini/Vertex, Bedrock signing, WebSockets, exhaustive model capability catalogs, and automatic paid-provider fallback remain outside the supported transport set. Per-model context/output limits, tool/vision declarations, and reasoning options are explicit configuration; choosing a model does not manufacture capabilities.

Canonical assistant/tool history now persists across turns and sessions. Compaction occurs at completed tool boundaries, archives originals, and produces a model-written handoff; context overflow gets one compaction/retry before surfacing an error. Provider cancellation closes the active socket, verified with a real local listener. See [Harness foundations](harness-foundations.md) for configuration, recovery, integration limits, and reproducible acceptance commands. Independent VM regression verification remains unavailable because the configured service rejects this repository's location.
