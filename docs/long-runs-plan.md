# Durable long runs

Build this after the harness foundations and team/session release. Ordinary coding turns keep their current behavior; supervised runs are explicit.

- Persist an objective, concrete acceptance criteria, bounded model-request/token/wall-time budgets, progress notes, completion evidence, session ID and recovery-run ID. An exclusive task lock prevents two supervisors from owning the same run.
- Reuse the existing coding agent, provider transports, permission checks, tool loop, checkpoints, compaction and cancellation. Do not create a second agent framework or automatically delegate work.
- Provide start/resume/status/list/stop controls. Restart requires explicit review of possible side effects; use the existing uncertain-tool recovery instead of replaying unfinished tools. Resume keeps consumed budgets and does not retain approval grants.
- Add a task-progress tool only inside supervised runs. Keep the durable objective/criteria in context across compaction; require evidence for every criterion before accepting a completion claim. Distinguish model-reported completion from independent verification. Do not force tests for tasks that do not need them.
- Account for each provider attempt, including retries and compaction. Reserve a conservative token estimate before sending requests. Exact billed input can differ; report that limit honestly. Wall-time cancellation must also interrupt waiting for permissions and running commands.
- Persist tool results before stopping at stalled repeated failures; stop repeated premature final answers instead of looping forever. Preserve the last useful checkpoint and give a concise stop reason.
- Verify original behavior and the real process path with deterministic long sequences, compaction, provider interruption, deadline cancellation, stop requests, restart recovery, budget retention and task ownership. A fast fixture is not a multi-hour endurance or model-quality guarantee.

Implemented and installed: see [long-runs.md](long-runs.md) and [replayable validation metadata](long-runs-validation.json). Cross-platform CI is checked after publication. Multi-hour wall-clock endurance remains unmeasured.
