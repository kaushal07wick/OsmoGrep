# Supervised long runs

Use an explicit task when a coding objective needs sustained work across many model rounds. Ordinary chat and coding turns keep their normal behavior. The supervisor reuses OsmoGrep's coding loop, provider transports, compaction, approvals, file guards, process sessions, and recovery checkpoints.

```sh
osmogrep task start \
  --objective "Implement retry handling while preserving the public API" \
  --criterion "429 and transient server errors use the existing bounded retry policy" \
  --criterion "Existing public behavior and tests remain intact" \
  --max-seconds 28800 \
  --max-requests 1000 \
  --max-tokens 2000000 \
  --auto-approve
```

Run in the project or add `task --repo-root /path/to/project`. The `--auto-approve` flag authorizes dangerous actions within the selected permission profile; omit it to deny actions needing approval in this noninteractive command. `--permission-profile read-only` keeps project mutations blocked. Existing tool policies and sandbox settings still apply. This supervised mode runs one coding agent. Nested subagent/swarm/AI-workflow tools are unavailable until they can share the supervisor’s budget; ordinary explicitly requested subagents remain available outside this mode.

Ask in the objective to coordinate with another OsmoGrep instance in the same repository. The task's session also appears in the session center (**Command+Left**, or **Ctrl+S**) in another terminal.

## What survives

Task metadata records the objective, acceptance criteria, recent milestones, completion evidence, consumed budgets, session ID, recovery-run ID, and stop reason in `.context/osmogrep-tasks/`. The existing run archive stores canonical conversation and completed tool outcomes. Keep `.context/` out of Git.

The objective and criteria are refreshed as separate system context across compaction. A `task_progress` tool is exposed only during supervised runs. The agent records meaningful checkpoints and must supply an evidence entry for every criterion before its final answer can complete the task. A short “done” response alone causes continuation; three such answers without a new milestone stop the run as stalled. Repeated identical tool failures also stop supervised work after eight warnings/results.

**Completed means the model reported the criteria satisfied with recorded evidence.** Evidence is reviewable; it is not an independent correctness certificate. Tests run when the task calls for them. The supervisor does not turn every request into a testing or review workflow and does not automatically spawn subagents.

## Budgets

Each task has three cumulative limits:

| Budget | Behavior |
| --- | --- |
| Active wall time | Includes provider waits, tools, retries, and process supervision; excludes time while the task is not running |
| Provider requests | Counts each attempt, including retries and context summaries |
| Charged tokens | Reserves estimated input plus the configured maximum output before each request, then reconciles provider-reported input/output usage |

Missing usage, failed requests, and interrupted requests retain their reservation conservatively. Input tokenization is estimated, so this is an operational budget, **not an exact billing cap**. A provider can report more usage than estimated; no further request is started if the remaining reservation will not fit. Cached input is not subtracted from the token count.

The budget covers this supervisor’s native model calls. External tools, shell programs, MCP servers, and separately started team instances have their own costs and are not a combined billing account.

Defaults are eight active hours, 1,000 provider requests, and two million charged tokens. Explicit limits can reach seven active days, 100,000 requests, and one billion tokens. A deadline cancels an outstanding provider request or running owned command through the existing cancellation path. Budget exhaustion pauses with a checkpoint and reason; it never marks the objective completed. Resume retains already-consumed budgets.

## Inspect, stop, and resume

```sh
osmogrep task list
osmogrep task status TASK_UUID
osmogrep task stop TASK_UUID
osmogrep task resume TASK_UUID
```

A task has an exclusive supervisor lock. A second process cannot resume it while its current owner is running. `stop` requests cancellation from another terminal. Status is stored independently of the terminal output; `--json-events` on start/resume adds `task_started` and `task_finished` records to the normal JSONL agent events.

After a crash or an uncertain in-flight tool, inspect the workspace and use:

```sh
osmogrep task resume TASK_UUID --reviewed --auto-approve
```

This confirms inspection of possible previous effects. Recovery opens a new session from the existing checkpoint, preserves completed tool results, and marks any uncertain tool for inspection. It never replays an unfinished tool automatically. Approval choices must be supplied again on resume; stored task data cannot silently grant them. Clean pauses at a completed boundary do not require `--reviewed`.

A completed task cannot be resumed as though it were unfinished. An exhausted task keeps its limits; start a new objective with an explicit budget when more work is needed. Arbitrary shell commands and MCP servers remain capable of external effects, so a recovered conversation cannot undo all possible side effects.

## Verification scope

The replayable local fixture exercises more than 100 model rounds, repeated compaction, premature completion, request/token/retry budgets, external blockers, deadlines, cross-process stop, ownership, crash recovery, and preservation of later user edits. It uses a deterministic local provider and no paid calls. See [validation metadata](long-runs-validation.json).

These accelerated checks do not establish multi-hour endurance, provider availability, or general coding success. Native Gemini/Bedrock, subscription authentication, remote team networking, and automatic process takeover are not added by this supervisor.
