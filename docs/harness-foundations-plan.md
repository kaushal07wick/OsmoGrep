# Harness foundations

Implementation order agreed on 2026-11-15. Complete the missing foundations before ultra-long-run tuning. No automatic subagent delegation, test/review jobs, commits by the internal agent, or silent paid-provider fallback.

- [x] Model-aware context budgets, useful compaction, and overflow recovery.
- [x] Independent sessions, resume/search/fork, durable tool history, crash recovery, and concurrent-writer protection.
- [x] Interactive command sessions: streaming output, stdin, background processes, cancellation, and cleanup.
- [x] Execution/edit safety: scoped enforcement, atomic writes, and stale-file protection.
- [x] Real MCP client: discovery, stdio/HTTP lifecycle, authentication, resources, and cancellation.
- [x] Provider capabilities, secure credentials, supported authentication, reasoning controls, and transport cancellation.
- [x] Explicitly requested, individually scoped subagents with progress, follow-up, cancellation, and budgets.
- [x] Nested project instructions, language-server context, relevant retrieval, and editable optional memory.
- [x] Turn checkpoints with separate conversation/file restoration and conflict protection.
- [x] Stable extension/integration boundary and repeatable harness task evaluations.

Ultra-long runs follow these foundations; increasing iteration limits alone is not completion.

Each item requires implementation evidence and appropriate checks before it is marked complete. Unsupported platforms/providers must fail explicitly rather than suggest protection or compatibility that is absent.

## Completion evidence

| Foundation | Implementation | Checks |
| --- | --- | --- |
| Context | `compaction.rs`, model options, in-loop budget checks | tool pairing, overflow retry, archived original context, actual RPC turns |
| Sessions | `persistence.rs`, RPC and CLI/UI controls | exclusive locks, resume/fork boundaries, full history, crash recovery |
| Processes | `command_sessions.rs`, shell tool | incremental output/stdin, PTY, deadline, cancellation, completion evidence |
| Safety | `file_guard.rs`, `sandbox.rs`, `approval_rules.rs` | stale edits, escapes, literal prefixes, native macOS write/network denial |
| MCP | native rmcp client in `mcp.rs` | stdio and HTTP initialization/discovery, call/resource/reuse/cancellation |
| Providers/auth | async provider boundary, model options, OS credentials, device authorization | protocol suite, socket-close cancellation, OAuth pending/refresh fixture, synthetic Keychain round-trip |
| Explicit workers | `subagents.rs` over public RPC | isolated dirty-workspace snapshot, forwarded permissions, reviewable patch, recursion denied |
| Project context | nested instructions, LSP client, optional memory, tool search | scoped rules, framed LSP handshake/diagnostics, rejected server edits, discovery policy |
| Checkpoints | `checkpoints.rs`, multi-file patch reporting | creation/deletion/edit restore, later-user-edit refusal, multi-file unified rewind |
| Integrations/evaluation | versioned RPC, MCP extensions, fixture/live acceptance scripts, CI contracts | bounded protocol records, cancellation and queue/recovery tests; separate live coding checks |

The completion scope and explicit unsupported cases are in [the foundation guide](harness-foundations.md). These checkmarks describe supported foundations, not feature-for-feature parity with every provider or platform. Validation results are recorded in `harness-foundations-validation.json`. Ultra-long-run supervision and endurance testing remain the next phase.
