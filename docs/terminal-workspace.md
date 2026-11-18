# Terminal workspace

Use `/help` or F1 to search app controls. Ask the agent to test, debug, review,
plan or coordinate work in plain language. Legacy explicit workflow commands
remain supported but are omitted from the menu.

| Control | Purpose |
| --- | --- |
| `/settings` · F2 | Appearance, motion and workspace preferences |
| `/model` · F5 | Provider/model/endpoint draft; choose Apply model to save |
| `/permissions` · F4 | Existing tool policy; choosing a profile clears auto-approval |
| `/keymap` · F8 | Search actual shortcuts; Enter captures a new chord |
| `/statusline` | Toggle fields with Space; reorder enabled fields with Left/Right |
| `/skills` · F3 · type `$` | Search, enable/disable and insert explicit skills |
| `/usage` | Reported provider tokens, cached subset, calls and separate estimates |
| `/stats` | Weekly token bars, exact aggregates and sample/recorded views |
| `/status` | Workspace, run, session, verification and connected-tool status |

The command tray attaches directly to the paper composer and shares its width.
The model and provider labels in the status line are separate dropdown controls:
click either one, type to filter, then click a row or press Enter. Choosing a
provider opens its model chooser; choosing a listed model applies the pair for
subsequent turns. Escape backs out while preserving the input draft. F5 opens
the model settings for keyboard access, endpoint editing and custom identifiers.

Model discovery runs in Rust against the chosen endpoint's `/models` metadata
route, with a five-second timeout, no redirects and a 2 MiB response bound. No
agent turn or completion request runs for discovery. Endpoints without a catalog
retain custom entry. Select the Responses, Chat Completions or Anthropic API
supported by the endpoint. Saved credentials are scoped by provider, with legacy keys
preserved; they never enter the renderer. Drag over transcript text to select it; release copies plain text to the clipboard.
Ctrl+C copies that selection again, and Ctrl+O copies the latest response. Hold
Shift for terminal-native selection instead. `OSMOGREP_MOUSE=off` leaves keyboard
selectors and transcript copy commands available.

Settings shows visible categories, including Workspace for editor and device
panes. Tab/Shift+Tab also switches panel sections. Type to filter, Up/Down to navigate and
Escape to close without losing a draft. Escape never exits an idle app; `/exit`
is the explicit exit command. Model-field editing supports paste,
Backspace and Ctrl+U to replace a value. In shortcut capture, Backspace restores
the default and Delete unbinds the action. Conflicts are rejected. Escape and
Ctrl+C retain cancellation/copy behavior. Pending tool approvals take priority.

Preferences are saved per workspace using atomic replacement. Auto-approval is
session-only. Permissions use OsmoGrep's existing tool classifications; they do
not create OS filesystem/network isolation. Plan mode forces read-only tools.

Skills are discovered in repository `skills/`, `.agents/skills/`,
`.osmogrep/skills/` and user `~/.agents/skills/`, `~/.codex/skills/`,
`~/.osmogrep/skills/`. Discovery is bounded and does not execute files or follow
external skill symlinks. Only explicitly named enabled `$skills` are read into
an interactive agent request, with a combined 32 KiB instruction budget.
Instruction source paths are included; tool permissions continue to apply.

Usage history stores provider/model, a workspace hash, timestamps and token/call
metadata in a local SQLite WAL database. It stores no prompts, responses or code.
Charts aggregate recorded responses in the last 365 UTC days, capped at 2,000
day/model groups. The weekly view shows up to 12 Monday–Sunday UTC buckets,
with exact totals, input/output composition, this week, last week and the
average over displayed weeks. The current week is partial. Bar lengths share
one linear scale. Providers omitting usage create no fictional chart entries.
Stats opens clearly labeled sample data for the current design review; press D
to switch between sample and recorded mode. Samples never enter the usage ledger. Bars fill once over 900 ms on entry,
section return and sample/recorded switches, with a small row stagger. Totals
stay fixed. Reduce Motion skips the reveal; hover and normal redraws do not
restart it.
Legacy estimates remain labeled; prices, quotas and earlier activity are not
reconstructed. Database failure is reported without failing the coding agent.

One of horizon, interference, Conway Life or Lorenz is selected at launch. The
10-second central morph is small and very dim, then settles. Click to brighten
or replay it; pointer movement has no effect. Drafts and conversation take its
place. Reduce motion and artwork-off settings are available under Appearance.
Graphics-capable terminals use fine PNG dots; others use Braille. The native Rust
renderer remains a functional fallback with simpler presentation.

After updating the installed binary, exit a running session and start
`osmogrep` again to load the new renderer.

The composer uses **Cotton** in light mode and **Graphite** in dark mode. Its
paper grain uses a cached fine-pixel PNG behind real terminal text on Kitty-compatible
terminals. The [Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
places it below text and selection backgrounds; edits reuse the pixels, while
resize/theme changes replace them. Closing a panel, renderer fallback and exit
remove obsolete image data. Copied text stays clean. `/theme light` and `/theme dark` select the matching
surface. Other terminals and the native renderer use a uniform matte surface in the same
base material colors, without cell-sized checkerboard noise.

Tool output folds after three lines. **Ctrl+T** toggles details and longer diff
hunks; it can be remapped in Keymap. Changed lines have numbers and shaded
backgrounds. Pending permissions sit above the input while the transcript stays
visible: use Up/Down then Enter, or Y/N/A. Escape cancels once. A timed activity
indicator shows thinking/tool progress.

`/nv` opens Neovim without synchronizing plugins or rewriting your configuration.
In Ghostty on macOS it opens a native pane on the right while the same live chat
process stays on the left. The editor has a compact project tree, persistent
file tabs and a clickable agent-file ticker. Space a returns to chat, Space e
toggles files, Space g jumps to the live file, Space f toggles following, and
Space s splits the live file from a separate inspection window. Manual inspection
does not steal the live view or overwrite unsaved buffers. Close Neovim normally
to expand chat; `/nv toggle` requests a save-aware close.

Other terminals use an isolated tmux workspace with no status bar: side by side
at 100 columns or more, stacked below that. Ctrl+B, O switches panes. A standalone
agent must finish/cancel its current run before the initial tmux handoff. Existing
tmux settings stay unchanged. `OSMOGREP_EDITOR_BACKEND=tmux` forces this fallback.

The welcome header stays through the first chat exchange and hides starting with
the second message. Update checks run silently without blocking input. `/clear`
clears the visible transcript and diffs; `/diff` can reopen session changes, and
conversation context and undo history remain available.

The normal workflow is coding assistance, not a mandatory test/review pipeline.
Greetings have no tool access; coding requests use relevant tools and checks.
Saved conversation system instructions are refreshed each turn; old temporary
skill/steering instructions do not silently carry into unrelated tasks. Assistant
prefaces do not stop subsequent tool calls in the same response. There is no
forced verification-on-stop loop. Legacy background review/evaluation is off by
default, and old inherited settings do not count as explicit opt-in. Expert
`/autofix on` enables it only for changes made in the current turn.

Enter while running queues a separate follow-up turn, in order; Tab also queues
without replacing earlier messages. Queued text and image attachments remain
associated. Escape cancels current execution and pauses queued messages rather
than exiting or dropping them. Press Enter with an empty draft to resume a paused
queue. Explicit live steering remains available through `/steer now`. Ordinary
messages are never injected as live system steering. Missing credentials or skill
errors retain the unsent message for resumption. Submitted whitespace is preserved;
credentials do not enter recall history or the follow-up queue.

The transcript supports wheel/PageUp/PageDown scrolling, with End returning to live
output. Streaming does not pull a scrolled viewport back to the bottom. Permission
cards maintain a continuous background across styled text and padding. Cursor
colors are explicit for Cotton and Graphite and restored on exit. Swift packages
use `swift test`/`swift build`; a generic `tests` directory no longer implies pytest.

## Provider APIs

Provider selection now controls the actual API format: Responses, Chat Completions, or native Anthropic Messages. Model settings include an API protocol override for custom gateways. Both the primary agent and scoped calls translate tool calls/results, inline images, streaming events and usage through the same boundary. Ollama and LM Studio can run without a key. Headless output preserves streamed whitespace while redacting secret-like tokens. See [the source review and configuration guide](agent-provider-design.md) for provider defaults, commands, evidence and limits.

## Artwork spacing and edit output

The four approved central morphs retain the black background and dim settled logo. The artwork uses a smaller canvas, at least three terminal rows of clearance above the composer, separated native pixel dots, and smoother easing at up to roughly 30 frames per second. Hover remains inactive and Reduce Motion still settles the artwork immediately.

Long transcripts expose a thin draggable rail in the unused right gutter; wheel and PageUp/PageDown scrolling remain available. The footer indicates when the view is scrolled back and End returns to live output. Patch headings and approvals show the target path rather than raw patch JSON. Applied diffs appear beside the edit result, before the assistant's final reply. The prompt requests a one-to-three-sentence change summary without quoting the inserted content unless asked.

## Search, context and workspace drawer

Open **Find** with Ctrl+P (remappable in Settings), or `/find`. Its visible tabs share a local workspace drawer; using them does not call a model. Existing custom Ctrl+P bindings are preserved; Find remains available through `/find`.

| Tab | Behavior |
| --- | --- |
| Find | Search commands, file names, supported symbols, current output, pins and saved conversations in this repository. Enter jumps to a current message; saved results show their full text locally. |
| Context | Type `@` at a word boundary to choose files, Rust/Python symbols, images or recent output. Use `src/main.rs:10-20` for exact lines. Folder attachment supplies a file list, not every file's contents. |
| Queue | Pause the queue while editing, reorder or remove requests, then Resume. The current draft is preserved. During a run, **Send next** puts the draft first in the pending queue; **Steer now** uses the existing live steering channel and approval policy. |
| Changes | Inspect individual contiguous hunks. Keep records the choice. Revert lines requires a second confirmation, saves a checkpoint and refuses to overwrite subsequent file changes. Finish the active turn first. Reverting every line of a newly created file leaves an empty file; use the existing checkpoint rewind for full file lifecycle restoration. |
| Retry | Pick an earlier request to edit in a new saved conversation branch. The original conversation remains saved. Files stay as they are; retry does not roll back the workspace. Attached images/references become removable chips rather than hidden context. |
| Pins | Save useful output for later reuse. Pins remain local until explicitly attached. |
| Layout | Choose chat/code focus, review or the existing preview selector. Set the editor percentage for future opens. Narrow tmux workspaces stack the editor. Ghostty sizing is approximate because its automation API does not report pane dimensions; manual zoom changes can require toggling the terminal's zoom back. |
| Timing | View elapsed time, first visible output, provider attempts/retries, tool execution, approval waits, other elapsed time and overlapping UI round trips. Provider-internal thinking and network-only timing are unavailable. These observations cover the current process's most recent turn. |

Use arrows to select rows, click an action or press Alt+1…9 for its numbered action. Page Up/Down scroll the preview. Escape closes the drawer without cancelling an active task. Escape in chat still interrupts. Large previews and result pages retain their source rather than silently truncating it.

References are immutable snapshots. File references check the source digest again before sending and retain the prompt if the file changed. A reference is limited to 256 KiB and active references/pins to 1 MiB each, with an explicit error if exceeded. Use an editor selection for excerpts from larger files. Saved-session search reports unreadable/oversized snapshots instead of silently skipping them.

In Neovim, select text and press **Space a** to attach the exact selection, including unsaved buffer content, then return to chat. The private bridge accepts only bounded selections from this repository. Removing any part of an image/reference label removes the whole chip.

Source locations such as `src/main.rs:24:3` have a blue tint and open local source actions. Rerun is available when the originating user shell command is known; it prepares a shell draft and requires Enter. Unknown commands are never reconstructed from shortened tool summaries. Ask about an output attaches that output explicitly.

No new daemon, service, dependency, background model call or automatic wakeup of closed agents is introduced. Headless/RPC event output remains unchanged.
