# Terminal workspace upgrade

Requested 2026-11-14. Deliver in tested increments, preserving the Rust agent,
existing commands, model endpoints, tool guards, repository intelligence and
native renderer fallback. Completed increments are committed and pushed using
the repository's existing human Git identity, without AI co-author trailers.

## Audit and design

- [x] Baseline: 179 Rust tests and 20 TypeScript UI tests pass.
- [x] Inspect local Codex picker, keymap, skills and status-line code for interaction
  patterns. Implement OsmoGrep-specific code and styling; do not transplant its app.
- [x] Confirm source boundaries: Rust owns events, policy, persistence and execution;
  the optional TypeScript process owns display. Keep this boundary.
- [x] Identify gaps: duplicated command descriptions, transcript-only help/settings,
  no skill discovery/picker or configurable keymap/status line, estimated usage
  without historical daily measurements. `main.rs` and `commands.rs` already have
  broad responsibilities; new features belong in focused modules.

## Increment 1 — Command center and settings surfaces

- [x] Establish one command catalog for help, filtering and completion metadata.
- [x] Searchable command center and a consistent settings surface; preserve drafts,
  scroll position and Escape behavior. Keep legacy command arguments working.
- [x] Theme, model/provider, permission and pending-approval views with accurate
  descriptions of OsmoGrep's actual tool policy, without claiming OS isolation.
- [x] Structured TypeScript panel rendering and a functional native fallback.
- [x] Regression tests, screenshots, Mac PTY checks, commit and push.

## Increment 2 — Persistent controls

- [x] Searchable keymap with real remapping, conflict checks, reset and protected
  cancellation behavior. Display actual configured shortcuts in hints.
- [x] Status-line field selection, ordering and immediate preview, backed by saved
  workspace preferences with backward-compatible defaults and atomic writes.
- [x] Keep settings searchable, show their scope and make saved changes explicit.
- [x] Test persistence/restart, invalid settings, tiny terminals and draft recovery;
  commit and push.

## Increment 3 — Skills

- [x] Discover bounded local/repository skill catalogs from existing skill folders.
- [x] Browse, search, enable/disable and insert skills using `/skills` and `$`.
- [x] Feed explicitly selected skill instructions into the existing agent path,
  with source references and content budgets. Never weaken tool permissions.
- [x] Test discovery, disabled skills, path boundaries, insertion and actual model
  request integration; commit and push.

## Increment 4 — Real activity and pixel statistics

- [x] Capture provider-reported input/output/cache usage when available, separating
  it from estimates. Do not invent costs, quotas or historical daily activity.
- [x] Crash-safe local usage history containing metadata, not prompts or code.
- [x] Dithered activity charts, model breakdowns, context/usage views and honest
  empty states. Historical graphs start with collected observations.
- [x] Validate provider parsing, aggregation, concurrent writes and chart layouts;
  commit and push.

## Increment 5 — Four approved ambient animations

- [x] Port concepts 09 (horizon), 05 (interference), 06 (Life) and 01 (Lorenz) from
  the approved HTML gallery into reusable terminal animation geometry.
- [x] Select one per launch. Start small and very dim; clicking brightens and plays
  its transition, then returns to the quiet pose. Hover does not affect it.
- [x] Preserve graphics and text fallbacks, reduced motion, budgets, image cleanup,
  resize behavior and composer ownership.
- [x] Test deterministic scene fixtures, random selection, timing, click behavior,
  performance and Mac rendering; commit and push.

## Final validation and installation

- [x] Run full Rust/UI suites and compatibility checks; inspect committed diffs.
- [x] Install the optimized binary atomically with a backup and verify PATH/hash.
- [x] Exercise real Mac controls, provider mock, skills, permission handling,
  persistence, images, shell, renderer failure recovery and terminal restoration.
- [x] Push remaining validated changes and report exact coverage and limitations.

## Scope and evidence rules

- User preferences are workspace scoped unless the UI explicitly states otherwise.
- Permission UI controls the existing OsmoGrep tool policy; it is not a new sandbox.
- Preserve provider abstraction and custom model/endpoint entry.
- Context estimates, provider token counts and unreported billing stay distinct.
- The HTML concept gallery remains a review artifact; production code must not load it.
- The unrelated `.git.broken-20260813/` directory is never staged or modified.

## Execution log — workspace controls

- Replaced duplicated command listings with a curated catalog. Manual testing,
  triage, swarm and plan-item operations are omitted from discovery; legacy
  handlers remain compatible. Natural-language work goes to the existing agent.
- Added Rust-owned searchable workspace panels and a separate TypeScript renderer,
  including native-renderer fallback, appearance/model/permission views, actual
  key remapping, status-line ordering/preview and explicit local skill selection.
- Preferences use atomic replacement; old sessions receive defaults. Escape and
  Ctrl+C retain their cancellation paths. Skills are bounded and opt-in; discovery
  never executes them or follows external skill symlinks.
- Added provider usage events and a SQLite WAL metadata ledger, with separate
  estimated context/legacy counters, UTC pixel activity and per-model totals.
  No billing prices, subscription quotas or reconstructed historical activity.
- Executed: 190 Rust tests pass; 22 UI tests pass; TypeScript typecheck/bundle passes.
  The sandboxed Rust run could not bind its mock HTTP port or reach the clipboard;
  the same suite passes with the required local access.
- Real Mac PTY: 11 acceptance checks passed in `.context/workspace-controls-validation/`:
  help/permission search, 200k-character editing, real image thumbnail and provider
  bytes, resizing, shell execution, renderer crash fallback and terminal restoration.
  This run predates the usage ledger and the final animation changes; their expanded
  end-to-end checks are recorded separately below.

## Final local validation — 2026-11-14

- All 193 Rust tests and 24 TypeScript UI tests pass; TypeScript typecheck, bundle
  generation, optimized release build and `git diff --check` pass.
- All 16 expanded Mac PTY checks pass, including a second real process loading
  saved settings, genuine key remapping, selected skill instructions in the model
  request, provider-reported usage, pixel history, images and renderer crash recovery.
  Harness fixes canonicalize macOS `/private` paths and allocate a fresh controlling
  PTY for restart; these were test harness failures, not application failures.
- Installed CLI, graph/report/history queries, MCP authorization/root-spoof rejection,
  agent ReadFile/graph tools and TUI stop/exit behavior pass with a local provider.
- A single live OpenAI gpt-5.2 read-only text smoke request passes: expected response,
  exit 0, zero tool calls, 1,101 reported input tokens and 13 output tokens. It used
  an empty temporary workspace; no proprietary repository content was sent.
- The five existing controlled predictive-analysis cases pass; warm indexing reuses
  all three parsed files. This is regression coverage, not a public-repository
  accuracy benchmark or runtime verification of those synthetic regressions.
- Four approved geometry scenes are selected per process, at 0.62 visual scale.
  Startup playback remains very dim; clicks brighten playback and it fades back.
  Hover does nothing. PNG and Braille fallbacks, reduced motion and artwork-off
  settings are covered. Visual fixtures were inspected locally.
- Warm PNG generation, 30 frames per scene at 60×30 cells: horizon 7.83 ms mean /
  9.97 ms p95; interference 11.92 / 20.44 ms; Life 3.46 / 4.32 ms; Lorenz 4.12 /
  5.18 ms. These exclude process IPC and terminal output; UI cadence is 70 ms.
- Installed atomically at `~/.local/bin/osmogrep`, first on PATH, with a backup.
  SHA-256: `7fe81aac6aca08ba12104b25a145e266e36c0d6dff0af9d28acf26e4e5ed7e8e`.
- Remaining scope limits: real vision-model interpretation, other OS/terminal
  combinations, and hosted GitHub/E2B delivery were not revalidated. Native fallback
  presents the same controls/data with simpler graphics. Skill selection is wired
  through the interactive agent launcher. History starts with new reported usage;
  missing provider counts, prices and account quotas are never invented.

## Follow-up polish — 2026-11-14

- Responsive status fitting shortens workspace paths before removing optional
  fields. Panels now reserve side gutters and bottom padding, wrap help text,
  and use the full available label width in narrow panes. The quick command
  picker uses an OsmoGrep marker and separate selection/help treatment.
- Tool headings select a path/command instead of dumping edit payloads. Long
  tool results fold to three lines; Ctrl+T expands results and diff details.
  Diffs have line numbers, shaded additions/removals, pending/applied labels,
  bounded previews and a final-answer position after the diff. Denied/cancelled
  previews are discarded. Repeated cancellation requests are idempotent.
- Thinking/tool activity has a restrained animated dot wave and elapsed run
  time. Permission requests dock above the composer while keeping transcript
  output visible; arrows/Enter and Y/N/A are supported without changing policy.
- Stats include clearly labeled, deterministic sample data for visual review,
  separate from SQLite/history. D toggles sample/recorded mode. A compact recent
  activity chart replaces the calendar when a short terminal cannot fit it.
- Neovim launch now has its own small module. Standalone launch uses a private
  tmux server with its status bar disabled and the actual terminal dimensions.
  Narrow terminals use an editor window; wide terminals keep at least 64 columns
  for the agent. Existing tmux sessions retain their options. No plugin sync or
  user configuration rewrite runs on editor open. Toggle requests a save-aware
  close; only editor panes owned by this agent are targeted. Runtime checks size
  even when an intermediate multiplexer misses a resize event.
- The six interactive composer studies live in `composer-studies.html`. The user
  selected Cotton for light mode and Graphite for dark mode. Both are implemented
  as stable ANSI background-cell grain, preserving text selection, masking,
  cursor geometry and attachment protocols; the native fallback uses their base
  material colors. The other four concepts remain review artifacts.
- Validation: 197 Rust tests and 30 UI tests pass. The real Mac UI suite adds an
  approved temporary-file edit and shaded diff (17 checks). Six isolated real
  tmux/Neovim checks cover narrow windows, unsaved-file protection, full agent
  restoration, wide splits, resizing and clean terminal exit. The editor harness
  uses a bounded file gate to keep its test PTY alive for restoration inspection.
- Browser review: all six studies support multiline Unicode editing and image
  previews; comparison/selection persistence and 390/320 px layouts pass, with
  zero page errors and no external network requests. Fine browser paper grain
  is represented by restrained terminal-cell shading in the actual composer.

### Native composer pixels and readable weekly usage — 2026-11-14

- Replaced cell-sized noise with a cached, fine-pixel PNG paper surface through
  Kitty graphics. Cotton/light and Graphite/dark retain editable terminal text;
  selection uses an explicit material background for contrast in either theme.
  Unsupported terminals get a uniform matte fallback. Static pixels upload once;
  placements refresh after redraw, while changed/removed surfaces release data.
  PNG encoding is shared with the existing central artwork; no dependency added.
- Quick actions reserve top, side and bottom padding plus a gap above the input.
  Workspace cards retain outer gutters and two blank rows below shortcut hints.
  A small dotted circle replaces section diamonds; selection uses an arrow.
- Replaced the calendar/experimental terrain with weekly horizontal dithered bars.
  Each Monday–Sunday UTC bucket has an exact total; summaries show this week,
  last week and the average over displayed weeks. Gray/orange separates input
  and output on a shared linear scale. Current week is partial. Up to 12 weeks
  fit according to terminal height; sample data is explicit and D toggles actual
  recorded history without writing samples to the ledger.
- Validation: 198 Rust checks, 35 UI checks, 18 Mac terminal acceptance checks
  (including retained native paper uploads), and 6 isolated Neovim lifecycle
  checks pass. Unit cases cover pixel PNG checksums, sub-cell grain, both-theme
  selection contrast, geometry limits, padding and UTC/year-boundary aggregation.
  Native pixel layering was inspected in a temporary Ghostty window. The chart
  was reviewed using renderer fixtures; other operating systems were not tested.

### Attached command tray, interactive selectors and weekly reveal — 2026-11-14

- Attached quick actions to the top edge of the composer at the same width in
  both renderers. Kept internal padding; removed the intervening black gap.
- Added separately sized model/provider footer hit regions and searchable
  choosers. Both keyboard and row clicks use Rust-owned actions. Clicks carry
  the displayed label so an asynchronously refreshed catalog cannot apply a
  different model through a stale row index. Drafts and approval priority remain
  intact; F5 and custom identifiers remain available.
- Rust fetches bounded model metadata asynchronously only on chooser use. The
  selected provider's credentials stay in Rust, redirects are disabled, and
  failures leave custom entry available. Saved keys are now scoped by provider,
  retaining legacy keys instead of reusing one provider's key for another.
- Weekly bars reveal once over 900 ms at a 32 ms animation interval, with eased
  growth and a restrained row stagger. Metrics remain stable. Entry, section
  return and D restart it; redraw and hover do not. Reduce Motion skips it.
- Validation: 202 Rust tests, 38 UI tests, 20 Mac PTY checks and 6 isolated
  Neovim checks pass. The PTY suite now uses a temporary HOME and a local model
  catalog, proving actual footer clicks, row selection, model changes, chart
  animation, persisted settings and renderer recovery without real provider calls.

### Conversation delivery and transcript repair — 2026-11-15

Made ordinary coding assistance the primary agent behavior. Replaced the
triage-heavy system prompt with current-request guidance and removed forced
verification follow-ups from the runtime. Bare greetings have an empty tool
schema, `tool_choice=none`, and a runtime guard against unexpected tool calls.
Historical system/skill steering is refreshed rather than silently replayed.
User messages are recorded before provider execution, including failed/cancelled
turns. Assistant prefaces no longer terminate responses that also request tools.

Changed normal Enter delivery during a run from live steering to a FIFO queue;
Tab shares the queue, preserves whitespace/history, and excludes API-key entry.
Cancellation pauses pending turns; empty Enter resumes them. Missing credentials
and skill errors retain unsent prompts. New requests no longer overwrite each
other. Explicit steering preserves outstanding tool results. Old session diffs do
not schedule checks after unrelated replies; inherited automation defaults require
fresh opt-in. Framework detection recognizes Swift packages without assuming that
a `tests` directory is Python; Swift commands produce verification evidence.

Added drag selection/copy over rendered plain-text transcript cells, bounded wheel
scrolling, and a stable viewport during streamed output. Escape closes/deselects
or cancels; it never exits an idle app. Fixed permission-card resets that exposed
black holes in its padding. Set contrasting cursor colors and restore them at
terminal teardown. API-key input never enters history or the follow-up queue.

Native Mac acceptance covers an idle Escape, greeting with stale session changes,
three distinct queued turns, preface plus real read tool, permission-gated edits,
scrolling/copy via captured OSC52, stable scrolling during streaming, and terminal
restoration. Live provider checks in a temporary fixture also passed: greeting
with zero tools and a random marker retrieved through read_file. No proprietary
project source was sent. The VM verifier rejected this checkout because it is
outside its configured work folder; its boundary was left unchanged and no VM
verification verdict is claimed.
