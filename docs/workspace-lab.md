# Workspace Lab

Open **Settings → Lab** for replayable bugs, candidate comparisons, code contracts,
app behavior, context evidence and failure scenarios. Each category keeps its own
saved history. Opening a record does not run it. The chat draft is preserved.

Use normal requests in chat, such as “save a replay of this bug”, “compare these two
fixes”, “work with Agent B on the shared interface”, or “why are you assuming that?”
The agent discovers the relevant guide, inspects matching saved records and live
teammates, and chains the existing actions within your task permissions. You do not
need slash commands, internal IDs or JSON. Settings remains available to inspect
history and explicitly control recording. Ambiguous targets or missing installed
builds still need clarification; ordinary fixes do not start expensive campaigns.

The CLI and discoverable `lab` agent tool use the same Rust service. Lab is absent
from the ordinary seven-tool request. It adds no idle model calls, startup jobs or
new dependencies. Context settings require one small metadata lookup per agent
turn; the Lab database is not opened for ordinary requests with capture disabled.

```console
osmogrep lab guide
osmogrep lab guide replay
osmogrep lab list
osmogrep lab list --kind behavior
osmogrep lab show RECORD_UUID
osmogrep lab create request.json
osmogrep lab execute explicit-action.json
```

`create` accepts only capture/planning, contract metadata and factual corrections.
`execute` deliberately permits execution and settings actions. Both accept `-` for
bounded JSON on stdin. `guide KIND` prints exact request examples for `replay`,
`comparison`, `contract`, `behavior`, `context` and `scenario` without opening storage.

## 1. Replayable bugs

Capture selected regular text source files, a Git base, a separate reproducer,
independent expected behavior and nonsecret environment values. Proposed file
contents can be captured without writing them into the workspace. The report
records each source hash and the workspace bytes against which application is safe.

```console
osmogrep lab export REPLAY_UUID bug.json
osmogrep lab import bug.json
osmogrep lab replay REPLAY_UUID --server vm-service --timeout-seconds 120
```

Import verifies the package checksum and schema, assigns a new local ID, and never
executes the report. A checksum establishes integrity, not trust. Inspect imported
source and reproduction commands before granting execution.

Replay freezes the captured base and selected overlays in a temporary private
checkout. Source preparation uses Git/archive plumbing; reproduction commands run
only through an existing configured isolated VM service. There is no host fallback.
The service receives `start`, `context`, `reproducer`, `compare`, `check`, `export`
and `close` calls. Cleanup is attempted on failure and cancellation, and the checkout
is removed. Failed execution still produces a saved report. If remote cleanup
fails, the result includes the session ID and cleanup error for recovery.

Initial limits: 64 selected files, 256 KiB per text file, 2 MiB of selected source;
the tracked base must contain at most 10,000 regular files / 64 MiB. Symlinks,
submodules, private-key paths and export attributes that change/omit source are
refused. Use a smaller reproducer repository when a project exceeds these limits.
Unselected working changes and untracked files are not copied into the base.

## 2. Compare two fixes

```console
osmogrep lab compare LEFT_REPLAY_UUID RIGHT_REPLAY_UUID
osmogrep lab run-comparison COMPARISON_UUID --server vm-service
osmogrep lab apply SELECTED_REPLAY_UUID
```

Both candidates must use the same Git base, exact reproducer, expected behavior
and environment. Runs and intermediate results are saved separately. A missing
duration is `null`; wall time includes preparation and VM communication. There is
no automatic winner or merge.

Application loads the selected immutable snapshot, checks original bytes, uses the
existing guarded staged writer and creates a conflict-aware restore checkpoint.
Intervening user edits cause refusal. Diffs and editor-follow events use the normal
agent path. Applying a candidate does not make it verified.

## 3. Code contracts

A contract defines an interface, source fingerprints, explicit participants and
their declared dependent paths. Participants can use session IDs or unique live
roles when proposing a contract; IDs are stored after resolution.

```console
osmogrep lab check-contract CONTRACT_UUID
osmogrep lab accept-contract CONTRACT_UUID --definition-sha256 REVIEWED_HASH
```

CLI acceptance is **local-user only**; it cannot impersonate an agent. An agent
accepts only for its actual live session. Amendments clear prior acceptances.

An agent can explicitly subscribe while accepting. Participating sessions that
accepted the current definition then send fixed, deduplicated source-drift notices
through the existing durable team mailbox to subscribed open participants. The
notice references the contract and dependent paths; it cannot grant permissions,
delegate a task, widen a recipient restriction or override a no-messaging request.
Closed recipients still have the durable contract state to inspect when reopened;
this version does not run an offline notification outbox. A source hash change
requests review; it is not proof that the API broke.

## 4. Before / after app behavior

Save an ordered flow with explicit navigation/reset steps, assertions and source
references. `guide behavior` contains the supported step format. Run it against
each installed build using an explicit `before` / `after` phase and build label,
then compare the saved run IDs and export a paired screenshot report.

Only **existing named Mac VMs and dedicated registered iPhone simulators** are
eligible. Lab resolves the existing companion and registers its MCP controls; it
does not install companions, boot/download an OS or choose a physical phone.
Phone steps use fresh observations, stable accessibility identifiers, newly bound
session/request IDs and a final release even after an uncertain input failure.
Uncertain input is never automatically retried. Configure app reset/navigation in
the saved flow so the two runs begin in a comparable state.

Each completed step saves its real observation, PNG when supplied, assertion
result and elapsed time. Comparison counts changed pixels or reports a size
change. A pixel difference is not automatically a bug. Source fingerprints are
recorded; installed build identity is user-declared, not attested. These companion
APIs do not provide console/network telemetry, so those metrics remain unavailable.
HTML exports embed retained screenshots and escaped observations without scripts
or remote dependencies. Screenshots may contain private app data: inspect before
sharing. A flow has at most 32 steps; individual PNG captures are bounded to 4 MiB
encoded / 4096×4096 pixels. Large visual exports require a smaller flow.

## 5. Context inspector

Discovery for a context-provenance question includes the current request’s
message/tool metadata and source provenance with recording disabled. The agent
can inspect again later in the turn if needed. This creates no saved request record
and does not expose private model reasoning or raw message text. Metadata describes
the canonical harness input before provider translation, not a network capture.

Enable or disable recording directly in **Lab → Context inspector**. From the next
agent turn, opt-in capture records the actual canonical harness request before
provider translation, message/tool provenance, byte sizes, fingerprints, elapsed
request time and provider-reported usage. It is not a wire capture, token counter
or view into private model reasoning. Image and reasoning payloads are excluded.

Requests remain in private application storage, limited to 100 records / 32 MiB per
project. Known credential patterns are redacted; redaction cannot recognize every
application secret. Do not enable recording around unknown sensitive material.
Disabling capture preserves evidence already saved. No authentication headers or
provider keys are intentionally recorded.

Save explicit user factual corrections with optional source references. They are
reference context, not new instructions or tool authorization, and take effect on
the next turn. Source-linked corrections are suppressed after those source hashes
change. Inspect their stale-source status or clear active corrections in the panel.
Historical correction records remain available. Corrections are workspace-scoped;
the evidence store is project-scoped across worktrees.

## 6. Failure scenarios

Plan up to 16 named cases / 32 isolated runs against a captured replay. Cases can
vary environment values, locale, timezone and explicit faults implemented by the
reproducer or its test fixture. Repetition exercises state-sensitive checks inside
the isolated reproduction. A variable called `FAULT=timeout` only injects a timeout
if the fixture actually consumes it; it does not manipulate the system network.

Every run starts from a fresh snapshot and attempts VM cleanup, so perturbations
do not touch the user's app process, host clock or host network. Each case gets a
durable replay ID and execution result immediately. Cancellation keeps completed
results. The deadline covers the campaign, with bounded preparation and cleanup;
it is not a promise to finish arbitrary guest shutdown instantly. This version
does not inject arbitrary device/network faults or automatically repair failures.

## Evidence and implementation boundaries

The normalized VM adapter must independently return integrity, equivalent
environment, validated oracle, identical reproducer, matching reproduction command,
reproducer SHA-256, oracle references, base revision, candidate tree and a nonempty
environment fingerprint. Missing or mismatched fields keep outcomes **inconclusive**,
even when an observed exit code is zero. Native MCP observations are retained;
they are not silently upgraded to attestations. Services that do not accept the
temporary checkout path must explicitly permit it or reject replay; there is no
fallback that mutates the original repository.

Production code lives in `src/lab/`, split into storage, replay/runtime, comparison,
contracts, behavior, context and scenarios. `api.rs` owns the shared action protocol;
CLI, TUI and the discoverable tool are adapters. SQLite records have optimistic
versions and project isolation. Cross-process job locks prevent duplicate execution
of a plan. Session formats and the default seven-tool set remain compatible; tool discovery
now includes a small workflow capability index.

Local protocol/unit fixtures cover these paths. Real VM/device runs and independent
hard-repository benchmark results must be reported separately; mocked execution
does not establish real-world correctness or performance parity.
