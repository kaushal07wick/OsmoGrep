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

- [ ] Establish one command catalog for help, filtering and completion metadata.
- [ ] Searchable command center and a consistent settings surface; preserve drafts,
  scroll position and Escape behavior. Keep legacy command arguments working.
- [ ] Theme, model/provider, permission and pending-approval views with accurate
  descriptions of OsmoGrep's actual tool policy, without claiming OS isolation.
- [ ] Structured TypeScript panel rendering and a functional native fallback.
- [ ] Regression tests, screenshots, Mac PTY checks, commit and push.

## Increment 2 — Persistent controls

- [ ] Searchable keymap with real remapping, conflict checks, reset and protected
  cancellation behavior. Display actual configured shortcuts in hints.
- [ ] Status-line field selection, ordering and immediate preview, backed by saved
  workspace preferences with backward-compatible defaults and atomic writes.
- [ ] Keep settings searchable, show their scope and make saved changes explicit.
- [ ] Test persistence/restart, invalid settings, tiny terminals and draft recovery;
  commit and push.

## Increment 3 — Skills

- [ ] Discover bounded local/repository skill catalogs from existing skill folders.
- [ ] Browse, search, enable/disable and insert skills using `/skills` and `$`.
- [ ] Feed explicitly selected skill instructions into the existing agent path,
  with source references and content budgets. Never weaken tool permissions.
- [ ] Test discovery, disabled skills, path boundaries, insertion and actual model
  request integration; commit and push.

## Increment 4 — Real activity and pixel statistics

- [ ] Capture provider-reported input/output/cache usage when available, separating
  it from estimates. Do not invent costs, quotas or historical daily activity.
- [ ] Crash-safe local usage history containing metadata, not prompts or code.
- [ ] Dithered activity charts, model breakdowns, context/usage views and honest
  empty states. Historical graphs start with collected observations.
- [ ] Validate provider parsing, aggregation, concurrent writes and chart layouts;
  commit and push.

## Increment 5 — Four approved ambient animations

- [ ] Port concepts 09 (horizon), 05 (interference), 06 (Life) and 01 (Lorenz) from
  the approved HTML gallery into reusable terminal animation geometry.
- [ ] Select one per launch. Start small and very dim; clicking brightens and plays
  its transition, then returns to the quiet pose. Hover does not affect it.
- [ ] Preserve graphics and text fallbacks, reduced motion, budgets, image cleanup,
  resize behavior and composer ownership.
- [ ] Test deterministic scene fixtures, random selection, timing, click behavior,
  performance and Mac rendering; commit and push.

## Final validation and installation

- [ ] Run full Rust/UI suites and compatibility checks; inspect committed diffs.
- [ ] Install the optimized binary atomically with a backup and verify PATH/hash.
- [ ] Exercise real Mac controls, provider mock, skills, permission handling,
  persistence, images, shell, renderer failure recovery and terminal restoration.
- [ ] Push remaining validated changes and report exact coverage and limitations.

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
