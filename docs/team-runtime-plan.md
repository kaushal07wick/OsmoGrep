# Shared agent runtime — implementation sequence

This follows the harness foundations and precedes ultra-long-run supervision. User steering on 2026-11-15 adds same-repository team messaging, a cross-project command center, and improved runtime/voice presentation. Existing work continues; ordinary prompts never launch extra agents.

1. Finish foundation acceptance, install the verified release, and commit/push it separately.
2. Add a local transactional team store using the existing SQLite dependency: instance presence and heartbeats, durable directed/broadcast messages, acknowledgements/cursors, structured activity, and path claims. No manually maintained Markdown/text coordination. Messages remain reference input and never bypass user scope or approvals. Idle delivery must not silently create paid turns.
3. Connect registration, progress, message delivery at safe agent-loop boundaries, file-change recording and conflict-aware claims. Exercise multiple independent OS processes, concurrent delivery, stale-instance expiry, restart replay and edit contention.
4. Build a command center over saved sessions and live presence: project grouping, filters/search, pins, active/inactive/needs-you states, preview, resume/fork, and team contact. Respect the exclusive session owner; never steal another process's mutable session.
5. Replace the working glyph with a small fine-pixel orb, truthful phase labels and elapsed timing. Align completion timing. Add a voice control/status capsule in the composer, reflecting actual connection/listening state rather than simulated microphone levels.
6. Verify keyboard/mouse/resize/selection/accessibility and reduced motion in both renderers; run process and UI acceptance, install, commit and push. Then move to durable budgeted long-run supervision and endurance tests.

References: local Codex `agents_overview*.rs`, discovery/actions/view and status-feed code; TermUI's component catalog and MIT source. Reuse compatible patterns in the existing renderer instead of replacing the terminal framework. This plan records scope, not completion evidence.

Validation: see [team-runtime-validation.json](team-runtime-validation.json). Local process fixtures, UI checks, installation, and documentation are complete. Cross-platform CI is tracked after pushing; the separate [long-run supervisor](long-runs.md) is implemented and validated locally.
