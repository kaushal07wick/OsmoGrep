# Sessions and teams

OsmoGrep instances coordinate through a private local SQLite database. Open two or three terminals in the same repository, enable team messaging, and give each instance its own task. They remain separate coding sessions with their own model, context, permissions, and process ownership.

```sh
cd your-project
osmogrep --new --team
# In another terminal, in the same project:
osmogrep --new --team
```

`/team on` enables coordination in an existing session. `/team off` suspends automatic incoming context and model-initiated sends. Presence and file-tool activity remain visible. Messaging never starts another agent, silently creates a paid turn, or grants permissions.

## Session center

Open `/agents`, `/sessions`, F6, or `osmogrep agents`. Search titles, directories, models, and session IDs. Sessions are grouped by project, with filters for working, ready, inactive, and needs-you states. Errors appear under Needs you. Elapsed update age reflects actual activity rather than heartbeat traffic.

| Control | Action |
| --- | --- |
| Left/right | Change status filter |
| Up/down | Select a session |
| Enter | Resume an inactive session; preview a live owner's saved output |
| F1 | Preview saved output without taking ownership |
| F2 | New session in the selected project's directory |
| F3 | Fork the selected session's saved history |
| F4 | Send a message to the selected session in this repository |
| F5 | Pin/unpin a session within its project |
| Escape | Back to the list, or back to your unchanged composer |

Previews refresh once a second; Up/down, PageUp/PageDown, Home/End scroll saved output. They are saved snapshots, not terminal screen scraping. A running process keeps its exclusive session writer lock. Finish foreground/background work and close the editor before switching your own session. A fork receives saved conversation history, not another process's unfinished command handles.

![Session center with illustrative sessions](assets/terminal-sessions.png)

## Messages and shared progress

The Team view shows unread messages and recorded progress. The composer shows a Team capsule when enabled or when messages are waiting. Click it to open the view.

- `/team inbox` prints and durably saves unread messages before acknowledging them.
- `/team history` shows recent sent/received messages and acknowledgement counts.
- `/team updates` shows recorded status, tool, and file activity.
- `/team send <session-UUID> <message>` explicitly sends one message, even when automatic messaging is off.

The agent's `team` tool supports peers, inbox, history, updates, send, acknowledge, claim, and release. Sends require a stable client message ID, so retrying a send does not duplicate it. Broadcasts reach sessions already registered when sent; direct messages survive an offline recipient's restart. Acknowledgement means accepted into durable context or explicitly reviewed, not a guarantee that the model acted on it. The sender's UI tracks the latest outgoing acknowledgement.

During an active task, incoming messages are added at a completed tool boundary before the next model request. They are clearly marked collaborator reference data; they cannot replace the user's objective, grant tool approval, or authorize delegation. An idle instance shows an unread indicator until the user starts work or reads its inbox.

Status and completed file-tool edits are recorded automatically. Raw shell arguments, provider credentials, and full tool output are not broadcast into the shared activity feed. Message bodies are visible to collaborating sessions and may reach their selected model providers.

## Coordinated edits

Agents can claim exact file paths for up to two minutes. Active task heartbeats renew explicit claims; completion releases them. File mutation tools also acquire short atomic claims, refusing a conflicting owner before changing files. Multi-file claims succeed together or leave no partial claim. Existing read-before-write and stale-content checks still apply.

Claims are cooperative guards for OsmoGrep file tools. Arbitrary shell commands, MCP servers, and outside editors do not obey these leases. Use separate Git worktrees for strongly isolated concurrent edits. Linked worktrees share messages through their common Git directory while their file claims stay scoped to each actual checkout.

## Storage and failure behavior

The database is `team.sqlite3` in the platform OsmoGrep configuration directory. It uses transactions, write-ahead logging, private Unix file permissions, and bounded queries/message sizes. Nothing requires a manually maintained repository Markdown file. Live presence expires eight seconds after a process stops heartbeating. Graceful exit closes presence and releases claims; expired leases recover after a crash.

This is coordination between processes on one machine/user account. It does not provide remote network messaging, distributed consensus, or automatic takeover of another session. SQLite can report contention or storage failures; sends then remain visibly failed and drafts stay editable. Session snapshots and messages can contain private project information and should be protected with the rest of your local configuration.

## Runtime presentation

A cached fine-dot sphere replaces the previous working wave. Labels distinguish thinking, reading, commands, edits, and waiting for permission; the timer reports elapsed wall time. Reduce Motion keeps the visual still. Kitty-compatible terminals use a 36px graphic; other terminals use a small Braille fallback. Completion timing sits on its own aligned line.

The composer voice capsule starts/stops the existing voice connection and reflects its actual connection state. It does not simulate microphone levels. Entering an API key pauses voice, and late transcripts cannot enter a protected input field.

The implementation uses the existing Rust runtime and pi-tui renderer. References included local Codex session discovery/overview behavior and the [TermUI component catalog](https://www.termui.io/components); no UI framework replacement was required.
