# Terminal workspace

Use `/help` or F1 to search app controls. Ask the agent to test, debug, review,
plan or coordinate work in plain language. Legacy explicit workflow commands
remain supported but are omitted from the menu.

| Control | Purpose |
| --- | --- |
| `/settings` · F2 | Appearance, motion and workspace preferences |
| `/model` · F5 | Provider/model/endpoint draft; choose Apply model to save |
| `/permissions` · F4 | Existing tool policy; choosing a profile clears auto-approval |
| `/keymap` · F8 | Search 19 actual shortcuts; Enter captures a new chord |
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
retain custom entry. Listed models still need the Responses capability used by
the existing agent. Saved credentials are scoped by provider, with legacy keys
preserved; they never enter the renderer. Drag over transcript text to select it; release copies plain text to the clipboard.
Ctrl+C copies that selection again, and Ctrl+O copies the latest response. Hold
Shift for terminal-native selection instead. `OSMOGREP_MOUSE=off` leaves keyboard
selectors and transcript copy commands available.

Tab/Shift+Tab switches panel sections. Type to filter, Up/Down to navigate and
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
On standalone terminals it creates an isolated tmux workspace with no status bar.
Below 144 columns the editor gets a full-width window; use Ctrl+B, P to switch
windows. Wider terminals use a balanced split; Ctrl+B, O switches panes. Close
Neovim normally to restore the full agent view. `/nv toggle` requests a save-aware
close and preserves unsaved buffers until you decide. A standalone agent must
finish/cancel its current run before the initial tmux handoff. When already in
your own tmux session, OsmoGrep leaves that session's status bar/options alone.

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
