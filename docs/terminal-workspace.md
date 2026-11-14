# Terminal workspace

Use `/help` or F1 to search app controls. Ask the agent to test, debug, review,
plan or coordinate work in plain language. Legacy explicit workflow commands
remain supported but are omitted from the menu.

| Control | Purpose |
| --- | --- |
| `/settings` · F2 | Appearance, motion and workspace preferences |
| `/model` · F5 | Provider/model/endpoint draft; choose Apply model to save |
| `/permissions` · F4 | Existing tool policy; choosing a profile clears auto-approval |
| `/keymap` · F8 | Search 18 actual shortcuts; Enter captures a new chord |
| `/statusline` | Toggle fields with Space; reorder enabled fields with Left/Right |
| `/skills` · F3 · type `$` | Search, enable/disable and insert explicit skills |
| `/usage` | Reported provider tokens, cached subset, calls and separate estimates |
| `/stats` | Dithered UTC activity and model totals from recorded usage |
| `/status` | Workspace, run, session, verification and connected-tool status |

Tab/Shift+Tab switches panel sections. Type to filter, Up/Down to navigate and
Escape to close without losing a draft. Model-field editing supports paste,
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
Charts cover recorded responses in the last 365 UTC days, capped at 2,000
day/model groups. Providers omitting usage create no fictional chart entries.
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
