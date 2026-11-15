# Harness foundations

OsmoGrep keeps the coding loop in Rust. The renderer presents events; it cannot grant tool permissions. The supported provider protocols are Responses, Chat Completions, and Anthropic Messages. Explicit sustained work uses the [long-run supervisor](long-runs.md); multi-hour endurance remains a separate validation claim.

## Sessions and recovery

Each conversation has an independent UUID, complete canonical tool history, and an exclusive writer lock. Existing repository sessions are imported without deleting their original files. A second writer must fork rather than overwrite an open session.

- `/sessions` opens the session center; `/sessions <search>` prints matching saved conversations. See [sessions and teams](team-runtime.md).
- `/resume <UUID>` opens one; `/fork <UUID>` creates an independent copy.
- A normal `osmogrep` launch starts a fresh session, retaining workspace appearance and shortcuts. Older conversations open only when selected in the session center or explicitly through `--resume <UUID>` / `--fork <UUID>`. `--new` remains an alias for the default.
- `/recover <run-UUID>` opens an interrupted run in a new conversation. Completed tool results survive. An uncertain in-flight tool is marked for inspection, **never automatically replayed**.
- `/compact` requests a model-written handoff. Original messages are archived locally before compaction; the latest user request and completed tool-call pairs are preserved.
- Queued prompts restored through explicit resume remain paused until explicitly resumed.

Local state contains source/tool output and can contain sensitive project data. Keep `.context/` out of Git. Session/config replacements are atomic; Unix files are private. Recovery checkpoints live in `.context/osmogrep-runs/`, code snapshots in `.context/osmogrep-checkpoints/`, and large tool output in local artifacts.

## Context and model controls

Configure the selected model's actual limits in the platform `osmogrep/config.toml`. Unknown models use a conservative configurable 128k context/16k output default, not an inferred capability guarantee. Input estimates include structured tools and images; they are estimates rather than an exact provider tokenizer.

```toml
[model_options."openai/my-model"]
context_window = 128000
max_output_tokens = 16000
tools = true
vision = true
reasoning = "medium"

# Native Anthropic models can additionally use thinking_budget = 4096.
# The provider must support the configured option.
```

`/reasoning auto|none|minimal|low|medium|high|xhigh` and model settings save per-model controls. Unsupported efforts are reported by the provider. `tools=false` removes tool access; `vision=false` rejects image requests before sending them. `[context] window/output` and `[context.models."model-id"]` remain fallback configuration.

The tool loop checks the budget before each model request, summarizes only at completed tool boundaries, and handles a provider context overflow with one compaction/retry. It refuses an oversized current request rather than silently clipping it. Summaries use a bounded extract; the full source remains in the archive for retrieval. This does not promise lossless semantic recall.

## Commands that can keep running

The `run_shell` tool starts a process and returns `session_id`, incremental stdout/stderr, byte cursors, status, elapsed time, and original output-file paths. A subsequent call with that ID polls, sends stdin, closes stdin, or cancels the process tree. The agent can do other work between polls. Final polled verification commands record evidence once.

```json
{"cmd":"python3 interactive_job.py","yield_ms":500,"timeout_secs":120}
{"session_id":"returned-id","input":"answer\n","stdout_cursor":120,"yield_ms":1000}
{"session_id":"returned-id","cancel":true}
```

PTY mode uses the native `script` utility on macOS/Linux. Pipe mode remains available elsewhere. Sessions have explicit deadlines, 16KiB poll chunks, 64MiB spools per stream, and a 64-session ceiling. Graceful application exit stops owned processes. Arbitrary shell processes are not promised to survive or be reattached after a parent crash; inspect for external side effects before retrying. Closing a session preserves its output artifacts.

## Permissions and OS enforcement

Read-only tool policy is separate from OS sandbox enforcement. Approvals authorize an action; they do not themselves confine a process. Configure literal command prefixes, workspace write paths, or explicitly trusted tools:

```toml
[permissions]
allow_commands = [["cargo", "test"], ["cargo", "check"]]
write_paths = ["src", "tests"]
# allow_tools = ["a_tool_you_explicitly_trust"]

[sandbox]
mode = "off" # workspace or read-only
network = false
```

Prefix rules apply to literal simple argv only. Shell operators, redirection, substitutions, quotes, and chained commands ask again. A prefix grant trusts the selected command's own behavior. `/sandbox off|workspace|read-only` changes OS enforcement. macOS uses `sandbox-exec`; Linux requires bubblewrap. Enabled sandboxing fails explicitly on unsupported systems. Network is denied by default inside sandboxed processes; temporary runtime files are permitted. It is not a VM or protection against every side effect of a remote MCP service. Global configuration is user-controlled, not repository-controlled.

File tools reject workspace escapes and symlink write paths, protect `.git`, and use staged atomic replacements with stale-content checks. Whole-file replacement requires a prior read of an existing file. Begin/end patches validate every hunk before writing and report every changed file. Ordinary unified text patches report all changed files; binary/mode-only/rename/quoted-path changes require explicit file/Git actions. Optimistic checks reduce concurrent-edit races; they are not a filesystem transaction against arbitrary external writers.

## Checkpoints and rewind

`/checkpoints` lists turns. `/rewind <UUID> code|conversation|both` restores captured file-tool edits, forks conversation history, or does both. File restoration checks every captured file before starting and refuses later user edits. Created/deleted files are included. An IO failure reports partial progress; a retry can continue safely after inspection.

Snapshots cover text file tools, up to 512 files/50MiB per turn. **Shell, remote MCP, external services, and Git side effects are not reversed.** `/undo` retains the smaller last-file-edit control and also checks for later edits.

## Credentials and authentication

New saved keys never fall back to plaintext: macOS Keychain, Linux Secret Service via `secret-tool`, and Windows DPAPI protect them. Unlock/provision the OS backend or use environment variables. Credential namespaces include the configuration-directory identity, so alternate profiles do not overwrite each other's keys. `/key migrate` moves legacy keys and removes plaintext config values only after successful secure saves.

For a provider supporting standard device authorization, register your own client and configure:

```toml
[auth.my-provider]
client_id = "your-registered-client"
device_url = "https://auth.example/device"
token_url = "https://auth.example/token"
scopes = ["offline_access"]
```

`osmogrep auth login my-provider` prints the verification URL/code and waits. `status` checks for a stored login; `logout` removes it. Expiry, pending/slow-down, refresh locking, and rotated refresh tokens are handled. HTTPS is required except loopback fixtures. This is **not consumer ChatGPT/Claude subscription login** and does not borrow another app's OAuth client or credentials. Native Gemini/Vertex, AWS Bedrock signing, and WebSocket model transport are not implemented.

## MCP, language servers, and project context

The native MCP client initializes connections, discovers tools/resources/prompts, reuses stdio or Streamable HTTP transports, enforces deadlines, and cancels failed requests. Remote servers require HTTPS except loopback; bearer secrets come from an environment variable. Legacy `transport="bridge"` configurations remain available.

```toml
[mcp]
enabled = true
default_server = "local"
[mcp.servers.local]
command = "your-mcp-server"
args = ["--stdio"]
timeout_ms = 30000
[mcp.servers.remote]
url = "https://service.example/mcp"
bearer_env = "MY_MCP_TOKEN"

[lsp.servers.rust]
command = "rust-analyzer"
args = []
extensions = ["rs"]
language_id = "rust"
timeout_ms = 10000
```

The `language_server` tool supports hover, definitions, references, symbols, and diagnostics through initialized persistent servers. It synchronizes opened documents, invalidates changed diagnostics, bounds framing, and rejects server-initiated edits/commands. Diagnostics expose whether a reported version was confirmed. Install/configure the language server yourself; an absent server is an explicit error, not fabricated context.

Root instructions and applicable nested `.osmogrep.md`, `AGENTS.md`, `CLAUDE.md`, and `.cursorrules` guide coding work. New nested rules must be reviewed before the pending edit executes. Nested instructions are capped at 32KiB. Optional `.osmogrep/MEMORY.md` stores at most 16KiB of editable reference facts; the memory tool writes only through normal approvals and is prompted to save only explicitly requested facts. Memory is not an autonomous task queue. `tool_search` discovers additional tool schemas without granting permission or enabling delegation implicitly. Existing symbol/repository retrieval remains available.

## Explicit subagents

Subagents start only when the user asks to delegate. Each receives a specific task and scope. Read-only workers share current files; writable workers receive an isolated Git worktree containing the parent's current tracked/untracked nonignored files, including uncommitted edits. Workers cannot recursively delegate or approve their own permissions.

The `subagent` tool exposes spawn/list/status/send/steer/permission/cancel/changes/close. Status includes progress and explicit permission requests. Each worker has a turn-iteration limit (default 20), a lifetime timeout (default 15min), and a maximum of eight concurrent open workers. Changes are reviewed as a patch relative to the snapshot, then applied by the parent through ordinary approved tools. No automatic merge, commit, or push occurs. Closing preserves the worktree; binary/empty-file changes may require direct inspection. Snapshot limits are 10,000 files/50MiB. Worker control is process-local; durable worker conversation/files remain after interruption, but automatic reattachment is not claimed.

## Integration contract and acceptance

`osmogrep rpc --repo-root /project [--session-id UUID]` speaks newline-delimited JSON. `ready` announces protocol version 1 and supported methods. Requests have `id`, `method`, and optional `params`; replies carry the same ID. Events retain stream `run_id`, sequence, and timestamp fields. Recovery/checkpoint UUIDs identify durable agent turns separately; use `/checkpoints` or `.context/osmogrep-runs/` to obtain those IDs. Supported methods: `prompt`, `steer`, `cancel`, `permission`, `session`, `compact`, `recover`, `checkpoints`, `rewind`, `shutdown`. Prompt queues are bounded to 64 and incoming records to 1MiB. Permission decisions use the emitted request ID. EOF cancels owned work. RPC prompts currently carry text; interactive UI image attachments remain supported separately.

```json
{"id":1,"method":"prompt","params":{"prompt":"Explain src/main.rs"}}
{"id":2,"method":"session"}
{"id":3,"method":"shutdown"}
```

MCP supplies the external tool-extension boundary; RPC supplies the control/integration boundary. There is no arbitrary in-process plugin ABI.

Reproduce local checks with:

```sh
cargo test --locked
npm --prefix terminal-ui test
npm --prefix terminal-ui run build
cargo build --release --locked
python3 terminal-ui/scripts/provider-acceptance.py --binary target/release/osmogrep --output /tmp/providers.json
python3 terminal-ui/scripts/harness-acceptance.py --binary target/release/osmogrep --output /tmp/harness.json
# macOS, outside an enclosing sandbox:
cargo test --locked -- --ignored --nocapture
python3 terminal-ui/scripts/mac-acceptance.py --binary target/release/osmogrep --output /tmp/terminal-check
```

Fixture acceptance exercises real executable/model-transport/tool/filesystem/session boundaries, including overflow, compaction, cancellation, recovery, and isolated workers. It is deterministic protocol/behavior evidence, **not a coding-success benchmark or multi-day endurance claim**. Live-provider checks are opt-in and recorded separately. Platform code outside the tested host still needs CI/runtime verification; independent VM evidence is unavailable for the current repository location.

Live coding smoke checks are a separate paid opt-in:

```sh
python3 terminal-ui/scripts/coding-live-acceptance.py --binary target/release/osmogrep --output /tmp/live-coding.json --allow-paid
```

The three cases cover a documentation edit preserving user text, a Python bug fix preserving original tests, and a multi-file rename verified independently. They are small smoke checks, not a broad success-rate benchmark.
