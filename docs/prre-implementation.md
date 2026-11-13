# Persistent runtime regression intelligence — implementation log

## Status

Local core implemented and tested; hosted integration pending. The complete
master engineering definition of done is not met. A controlled differential VM
regression and repair were validated during Mac installation testing through the
existing MCP tools. The new Rust bridge contract, hosted execution, production
telemetry and live GitHub delivery remain pending. See [supported behavior and
limits](prre.md) and the Mac installation addendum below.

## Phase 0: audited baseline (2026-11-13)

This checkout is one Rust binary crate, version 0.3.4, not a workspace. No
`AGENTS.md` applies within it. The pre-existing untracked `.git.broken-20260813/`
directory is left untouched.

| Area | Source of truth / reusable boundary | Observed capability |
| --- | --- | --- |
| CLI / TUI | `src/main.rs`, `src/ui/`, `src/state.rs` | Default interactive agent; version, doctor, sessions, uninstall, run, triage |
| Agent / models | `src/agent.rs` | Existing provider configuration, bounded agent/tool loop, cancellation, review jobs |
| Tools | `src/tools/mod.rs` | Scoped schemas, permission classification, root-aware dispatch |
| Commands / GitHub | `src/commands.rs`, `src/triage.rs` | Git / gh operations, REST triage, optional posting; not a GitHub App worker |
| Index | `src/context/indexer.rs` | Tree-sitter Rust/Python symbols; local context.json / meta.json; name-based calls |
| Context export | `.context/context.json`, `call_graph.md` | Working-tree cache and checked-in descriptive map, neither revision-aware nor runtime proof |
| Memory / jobs | `src/persistence.rs`, `src/harness.rs` | Local session JSON and run JSONL; in-process job queue |
| Execution | `src/process_runner.rs`, `src/test_harness.rs` | Local bounded commands and detected test frameworks; not a sandbox |
| Evidence | `src/verification.rs`, `src/verify_stop.rs` | Local command ledger, edit staleness, verify-before-stop |
| MCP | `src/mcp.rs`, `src/tools/mcp_call.rs` | Configured command/environment bridge client; no MCP server in this tree |
| Cloud / E2B / bot / console | No matching source, manifest, migration, worker or frontend found | Absent from this checkout; cannot port into a nonexistent worker |
| Security / logs | `shell_guard.rs`, tool permissions, event redaction, run ledgers | Existing local controls; not VM isolation or hosted tenant authorization |
| Skills | Repository instructions loaded by agent; dynamic workflow files | No existing packaged runtime-risk skills |

Baseline actually executed:

- `cargo test --locked`: **138 passed, 0 failed** (0.22s test execution).
- `cargo check --locked`: **passed** (15.25s baseline).
- `cargo run --locked -- --help` and `cargo run --locked -- version`: **passed**.
- TUI startup: baseline rendering, input, cancellation and terminal unit tests
  passed; interactive terminal startup has not yet been exercised.
- Cloud-worker, hosted E2B and GitHub App tests: absent, not passed.
- No live model request or external authentication attempted.

## Implementation boundaries and reach

Preserve the existing agent loop, normal startup, TUI rendering, command behavior,
context cache format, provider configuration, and verification ledger. The new
optional context store belongs under `src/context/`; prediction and differential
orchestration belong in a separate module. Reach: additive CLI commands, opt-in
agent graph queries, the existing MCP bridge, schema migrations, controlled
fixtures, tests and documentation. No automatic execution of repository recipes
on the host. No new executor, cloud console or agent runtime.

Use transactional SQLite with revision snapshots and content-addressed per-file
analysis. Reuse existing tree-sitter extraction. Store provenance and evidence
classes explicitly. Static name matches remain uncertain. Keep data repo-local
under ignored `.context/`, with bounded queries and explicit retention.

## Milestones / blockers

1. Verification foundation: preserve identical generated reproducer and original
   suite results through the existing configured isolated-execution bridge.
2. Durable graph and history: transactional revision snapshots, incremental
   extraction, runtime metadata, bounded structural queries.
3. Selective analysis and pairs: deterministic skips, runtime risk, explicit
   causal hypotheses, centralized budgets, conflict-aware combined revisions.
4. Interfaces: additive CLI, agent tools and MCP exposure, skills, structured
   reports suitable for the absent hosted API/console integration.
5. Evaluation / hardening: controlled positive/negative cases, restart,
   rollback, divergence, stale evidence, failures, budgets, compatibility.

External integration blocker: the requested cloud worker, web console, E2B
adapter and GitHub App source are not in this repository. Their APIs must not be
invented or represented as tested. Available OsmoGrep MCP tools refer to a local
VM service; availability and its allowed repository boundary need live checking
before claiming runtime verification.

## Implemented increments and actual checks

### Verification foundation

Added `src/intelligence/verify.rs`, reusing `src/mcp.rs` and the existing bounded
process runner. The lifecycle preserves the original-suite context and generated
test, compares, checks integrity, exports, and always attempts close after a
session ID is received. No repository command is executed by this layer on the
host. Failed setup is distinct from a failed behavioral assertion.

Confirmation requires explicit backend attestations plus revision, source-tree,
test hash, command, oracle, environment and immutable export bindings. Missing
fields remain inconclusive. The adapter's required normalization is documented;
it is not claimed compatible with a live console export yet. Original-suite
results survive a mocked worker loss. Stale revision bindings fail confirmation.
The repair/three-revision extension and actual cloud-worker port remain pending.

Fixed two existing integration defects needed by this path:

- MCP child stdout was inherited, so callers could not parse the returned JSON.
  It is now captured through the shared runner without truncating valid JSON.
- The runner waited before draining pipes, allowing large logs to stall a
  command. It now drains concurrently, enforces a stream size bound, and on Unix
  terminates process groups at deadlines/cancellation, including inherited pipes
  left open after a parent exits. Existing command/cancellation tests still pass.

The first foundation checks passed (two differential tests and one bridge test).
Additional mocked lifecycle, source/command validation and pipe tests were added
with subsequent hardening. These are mocked evidence checks, not a runtime bug
confirmation.

### Durable graph, history and runtime context

Added `src/context/graph.rs`, `graph_schema.sql`, and `runtime.rs`. Added one direct
dependency, `rusqlite` with bundled SQLite; updated the lockfile. Reused existing
tree-sitter extraction via a small additive helper in `indexer.rs`.

The schema includes revision snapshots, content-addressed analysis, typed nodes
and relationships, structural memory, timestamped deployment observations,
immutable reports/artifacts, and transactional model/experiment reservations.
File analysis is reused by path/blob/parser identity. Updates publish only on
commit, so cancellation and failed migration roll back. Old snapshots stay
queryable and sibling revisions do not replace each other's relationships.

Tests passed for reopen/restart, changed-symbol updates, unchanged-file reuse,
idempotence, isolated revisions, cancelled indexing, bad migrations, corrupt
storage, explicit compaction, and symlink rejection. Resource-linked historical
evidence is retrieved for new predictions. Runtime observations retain exact
deployment IDs and timestamps and do not overwrite manifest declarations.

Current parser coverage is explicitly limited to Rust/Python top-level
definitions and name-based calls. General semantic resolution, broad historical
GitHub/incident ingestion and live telemetry collectors are not implemented.

### Selective prediction and pairs

Added `src/intelligence/git.rs` and `predict.rs`. Conventional docs-only changes
and empty diffs skip model/sandbox work. Runtime/configuration indicators
escalate deterministically. Explicit `capacity_covers_concurrency` invariants
produce numerical causal hypotheses with assumptions, independent oracle sources,
expected observations, affected components and verification plans.

Pairs are selected through shared declared resources or mapped service files.
Related branches use a checked common base and Git's merge-tree/commit-tree
plumbing without changing branches, index or worktree. Conflicts are visible;
custom merge drivers are rejected before execution. Unrelated pairs defer.
Reports preserve the heads, common base, merge order and combined identity.

Central policy supplies modes and limits. Durable reservations apply across
report IDs for a candidate, including failed attempts. There is no automatic
merge blocking or new queue daemon. Non-capacity interaction rules, probabilistic
ranking and an asynchronous hosting scheduler remain future work.

### Product interfaces and model reuse

Added optional `index`, `context`, `predict`, and `intelligence-mcp` commands.
The six requested MCP operations share the CLI implementation and a fixed
canonical repository root. Explicit verification is disabled unless the MCP
server is started with a selected sandbox bridge. The agent's scoped
`query_code_graph` tool is read-only and absent from ordinary prompt schemas.

`--generate-experiment` makes at most the reserved number of bounded calls through
the existing provider code; no new model SDK or agent loop was introduced.
Unknown/over-budget/invalid test output cannot execute. The shared provider path
was exercised with an actual local HTTP mock, including output limits and
provider-reported usage. **No live model request was made in this initial phase**;
the installation addendum below records subsequent live checks.

Added four short skills under `skills/`; they are repository instructions and
were not installed globally. Existing CLI forms and TUI tests still pass.
Structured reports include PR numbers when supplied, exact revisions, risk,
hypotheses, historical context and associated verification jobs. The absent
console/worker/GitHub App has not been changed or replaced.

### Evaluation and hardening

Added the clearly labeled controlled repository under `fixtures/prre/pool` and
`scripts/prre-evaluate.py`. The script creates independent Git histories, runs
the real CLI, asserts labels, measures analysis/indexing overhead, and exports a
syntactically valid targeted Python contention experiment. It does not execute
fixture workload code on the host. The original smoke test is intentionally
insufficient; its actual execution still requires authorized VM access.

Recorded final analysis evaluation: [prre-evaluation.json](prre-evaluation.json).

| Controlled case | Expected / actual hypotheses | Measured latency |
| --- | --- | --- |
| Documentation-only change | 0 / 0, skipped | 106.94 ms |
| Pool reduction alone | 0 / 0 | 559.59 ms |
| Concurrency increase alone | 0 / 0 | 545.92 ms |
| Related pair combined | 1 / 1, predicted | 974.28 ms |
| Irrelevant pair | 0 / 0, deferred | 614.24 ms |

Sample size: **five synthetic analysis cases from one controlled repository**.
Model calls/tokens: **0/0**. Sandbox experiments: **0**. Cold indexing parsed
three files in 1,271.27 ms; warm indexing reused all three and parsed none in
87.55 ms. Database files totalled 225,280 bytes at measurement. Child CPU:
0.562297s user + 0.649734s system. Peak child RSS: 17,448,960 bytes. These are
single-run local harness measurements, including subprocess overhead, not
public-repository accuracy, runtime reproduction rate or hosted cost estimates.

The complete suite grew from **138 to 165 tests** with no baseline test edits or
deletions. Final `cargo test --locked --quiet`: **165 passed, 0 failed**. The
network-restricted sandbox denied the new HTTP mock's temporary loopback bind;
the same test and full suite passed with the approved test execution permission.
No credentials or external provider calls were involved.

Also passed: `cargo check --locked`, `cargo build --locked`, formatting checks for
all changed Rust files, `git diff --check`, real CLI help/version/run-help,
report retrieval across processes, impact queries, fetched PR identity, mutually
exclusive flag rejection, stdio MCP initialize/list/query, root-spoof rejection,
and syntax checks for fixture/test Python. No optional intelligence database was
created in this repository by ordinary CLI help/version operations. The existing
TUI's rendering/input/cancellation tests passed; interactive startup, other host
OSes and production-scale resource behavior remain untested.

## Live integration attempt and remaining blockers

The initial live verification request used the controlled repository:

`/private/tmp/osmogrep-prre-evaluation-20261113/repository`

The available OsmoGrep MCP `start` service rejected it with:

> Choose a repository directly inside the configured work folder; symlinks are not allowed.

No session ID was returned. The configured allowed folder was not disclosed in
that response. It was not widened, no different repository was substituted, and
no host runtime experiment was used as a substitute. This follows the installed
`osmogrep-verify` skill's explicit allowed-folder boundary as well as the task's
isolation requirements. No repeated authentication/provisioning was attempted.

To complete hosted validation, the remaining integration needs:

1. The real worker/console/E2B/GitHub App source and API contracts. Only master and
   origin/master are present locally; the remote-tracking tree also lacks them.
2. An authorized checkout inside the service's configured work folder, with the
   exact controlled base/candidate and a supported Python 3.11+ setup recipe.
3. A bridge adapter supplying the independent attestations documented in
   `prre.md`, and backend application of trusted setup recipes/resource policy.
4. Actual original-suite, base/candidate and optional repair VM executions,
   exported immutable artifacts and cleanup, then a live private GitHub report
   and console display through their existing integrations.
5. Unrelated public-repository regression evaluation, more interaction rules,
   broader graph analysis/history ingestion, and hosting-level scheduling,
   cancellation, isolation/tenant controls and deployment hardening.

The local layer is usable for bounded screening, durable graph/history queries
and the controlled capacity prediction. It is not represented as a fully deployed
or generally validated predictive regression product.

## Mac installation and real verification — 2026-11-14

Installed an optimized `cargo build --release --locked` binary atomically at
`~/.local/bin/osmogrep`, the first OsmoGrep on the existing PATH. Mode is 0755;
SHA-256 matches the build. Earlier binaries were backed up in private temporary
storage. Existing model credentials and provider configuration were preserved.
The native release and validation receipt are summarized in
[prre-install-validation.json](prre-install-validation.json).

The installed application passed:

- `version`, `doctor`, session listing, uninstall dry-run and command help.
- All five controlled analysis cases, cold/warm indexing, persisted report
  retrieval, graph impact/history queries and the six-tool stdio MCP surface.
- Rejection of a request attempting to supply another repository root.
- A local deterministic provider driving the real agent through `read_file` and
  `query_code_graph`, including actual tool outputs and completion.
- A live read-only request through the already configured provider, followed by
  a live run successfully invoking both `read_file` and `query_code_graph`.
- The actual terminal UI in a Mac PTY: startup, `/help`, `/quit` keeping the app
  open, `/exit` returning zero, alternate-screen cleanup, and restoration of the
  terminal's original local flags. README command descriptions were corrected
  to distinguish stopping an agent from exiting the app.

The live tool check caught a routing bug: the literal `query_code_graph` name did
not match the existing natural-language scope keywords. Fixed once in
`ToolScope::for_prompt`, added a regression test, rebuilt/reinstalled the release,
and repeated the live and local acceptance checks successfully. The complete
suite now has **166 passed, 0 failed**, including unchanged pre-existing tests.

Read-only process metadata located the existing console checkout and its already
configured work folder. The controlled Git history was copied into a new direct
child of that folder, with explicit notice; no service configuration or allowed
boundary was widened. The real backend exposed two fixture setup mistakes before
execution: it uses `.osmogrep.json` for custom recipes and recognizes
`test_original.py` for original-test freezing. The fixture now supplies both;
Python bytecode writes are disabled to keep experiment source fingerprints
stable. The independent contract and original test contents were preserved.

The existing VM MCP tools then executed the controlled scenario:

| Check | Base | Candidate / repaired candidate |
| --- | --- | --- |
| Original suites before experiment | pass | pass |
| Identical generated contention reproducer | pass (0) | assertion failure (1) |
| Same reproducer after pool-capacity repair | pass (0) | pass (0) |
| Original suites after repair | pass | pass |
| Original tests and recipe integrity | unchanged | unchanged |

The export reported `verified: true`, classification
`introduced-regression-repaired`. Both VMs were closed. Raw result, patch,
replay manifest and filtered source snapshots remain under ignored local
`.context/mac-install-validation/verified-vm-evidence/`; the public
[runtime summary](prre-runtime-validation.json) records hashes and observations
without publishing raw local runtime artifacts.

This validates one controlled failure and the existing VM execution path. It
does **not** validate the Rust `predict --verify` normalized adapter, which is
still unconfigured, nor hosted E2B/GitHub delivery, general detection accuracy or
production operation. The earlier outside-folder rejection is retained above as
an accurate record of the initial attempt, not the current VM validation status.

## Centered terminal layout — 2026-11-14

Replaced the edge-to-edge layout with one centered column capped at 112 terminal
cells. The empty welcome and prompt form a vertically centered group. Active
conversations keep a compact header and a bottom prompt within the same column.
Removed the oversized ASCII banner, light command cards and duplicate displayed
shell labels. Original stored log entries and execution behavior are unchanged.
Input dividers follow the existing theme, and footer details adapt to available
width while keeping a gap before keyboard hints. Command menus stay in bounds.

The full suite passed **167 tests**. Layout checks cover idle/active placement,
single shell entries, cursor and popup bounds at terminal sizes from 1×1 through
240×80, plus existing Unicode wrapping checks. Actual cell-buffer previews were
rendered and inspected. The optimized release was installed atomically and
passed the Mac PTY lifecycle, terminal restoration, local provider/tool and MCP
acceptance checks. Current installed SHA-256:
`f791f286d53f83c70227216c2df7cb729f441f7df105134c4de48d64ca1e590b`.
No live model or VM reruns were needed for this presentation-only change.

## Terminal-agent layout correction — 2026-11-14

The vertically centered welcome above was rejected in visual review. Restored
a compact, left-aligned session header at the top of the readable column. The
composer follows the transcript as it grows, then remains at the bottom while
long transcripts scroll. Startup and active conversation use the same layout;
removed the welcome heading and its special placement. Slash menus reserve room
above the composer. Version and approval state remain visible in the header,
with directory, branch and index status beneath. Shell entries retain their
single themed command line.

Visual transcript inspection also exposed multiline log records joining
paragraphs. Render each source line separately so paragraphs, blank lines and
fenced code retain their structure. Stored logs and execution are unchanged.

The full suite passed **168 tests**, including transcript growth, scrolling,
menu bounds and paragraph/code preservation. Release build passed. Changed-file
formatting and diff whitespace checks passed; the whole-repository formatter
still reports pre-existing formatting differences in `src/commands.rs`.

Installed the optimized release atomically at `~/.local/bin/osmogrep`, verified
it is first on PATH, and reran the Mac PTY lifecycle/terminal restoration,
local mocked provider/tool, graph/report reopening and MCP acceptance checks.
All passed. Actual cell-buffer previews were inspected for startup,
conversation and command-menu layout. Installed SHA-256:
`cf4604671a1b8159946f89a597294e851016c2e72def891022944226469b8069`.
Live providers, VM verification and hosted delivery were not rerun for this UI
correction. Existing sessions must exit and relaunch to load the new binary.
