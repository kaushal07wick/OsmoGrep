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

## TypeScript terminal presentation and image composer — 2026-11-14

The earlier centered and growing layouts were rejected. The user selected
Claude Code as the reference and supplied its actual startup/composer alongside
Codex's animation and image preview. The default presentation now uses a compact
brand/model/directory block at the upper left, the terminal's full width and a
bottom composer. A local point-cloud animation occupies the empty startup view
for 2.2 seconds, then stops. Typing or conversation hides it.

`terminal-ui/` contains the TypeScript source, tests and generated self-contained
JavaScript bundle. It reuses Pi's terminal display components, not Pi's agent.
Rust retains the existing event loop, input buffer, session history, tools,
permissions, model providers, cancellation, background jobs and command handling.
The Node child receives display snapshots and returns styled rows and geometry;
it cannot request tool execution or approval. The bundle is embedded in the Rust
binary and extracted into a private temporary directory for the UI process.
No runtime npm installation or background service is required.

Node.js 22.19+ enables the TypeScript renderer. Missing Node, invalid geometry,
renderer failure or a timed-out response restores the native Rust UI with a
visible warning. Request writes happen off the UI thread so a hung child cannot
block cancellation indefinitely. The parent owns terminal restoration and kills
its child on exit. Provider credentials are excluded from the child's environment;
API-key entry is masked before serialization. Repository terminal control codes
are removed before rendering. Existing light/dark themes, accent, density, session
names, command menus, plans, diffs, permission/update prompts and voice display
are represented in the new presentation.

Composer changes include partial Shift+arrow selection, replacement/cut/paste,
system clipboard integration on Mac, Unicode-safe cursor boundaries and movement
through wrapped rows. Long drafts retain their full text while the visible area
scrolls. Keyboard enhancement negotiation makes Shift+Enter distinguishable on
supporting terminals, and is popped during cleanup.

Image labels now correspond to real immutable image data. Dragged image paths
and Mac clipboard images produce PNG thumbnails, displayed with the terminal's
graphics protocol when available. Selected originals become `input_image`
content through the existing provider call; image messages during a running
turn are queued. Decoder limits, eight-image count and 20 MiB/image limits bound
image handling. No original image is sent to the presentation process.

Validation: **177 Rust tests** and **12 TypeScript tests** passed. Rust checks
include actual macOS clipboard image extraction through a private named
pasteboard, leaving the user's clipboard untouched. UI tests cover geometry,
Unicode and large drafts, selection, image placements, terminal-control stripping,
animation, themes, density, session names, diffs and prompts. The optimized build
was checked to embed the exact current generated JavaScript bundle. A GitHub CI
job now checks the TypeScript tests and reproducibility of the bundled output.

The Mac PTY acceptance script in `terminal-ui/scripts/mac-acceptance.py` uses a
local deterministic model endpoint and a generated solid-color PNG. It checks
the real TypeScript process, multiline editing, graphics sequences, original
image bytes at the model boundary, resize, shell execution and recovery after
killing only its own renderer child. This is distinct from live vision-model
quality and from the hosted PR/E2B verification path, which were not rerun.

Installed-release validation passed both the new Mac PTY acceptance script and
the existing CLI/graph/report/MCP/tool compatibility harness. The shell check
created and checked a file in the controlled temporary repository; the image
check compared original PNG bytes at the local provider boundary. Renderer
crash recovery and terminal flag restoration passed. Installation was atomic,
kept a backup, and verified `~/.local/bin/osmogrep` is first on PATH. Installed
SHA-256: `9b9ab543c0f2f9d0172af8651b378e7a161c3ffc2834e423103edf34ac344c5e`.
The redacted [validation summary](terminal-ui-validation.json) contains results;
raw local terminal transcripts remain under ignored `.context/` directories.

## Live website branding — 2026-11-14

The user clarified that the local website checkout was outdated. Fetched the
current `https://osmogrep.com/` HTML, deployed JavaScript and stylesheets, and
inspected the live page in a headless browser. The current brand uses a pixel
ring with an orange segment, lowercase wordmark, engraved textures and an ASCII
woodpecker finding and catching a bug. The preliminary orange terminal-mark
interpretation was discarded before installation.

Copied the deployed SVG pixel grid and sampled the real hero animation into 80
compact terminal frames. `terminal-ui/brand/provenance.json` records the source
hashes; the offline importer preserves the drawing geometry and woodgrain.
Runtime playback only decodes the small bundled data and draws terminal cells,
with warm light ink on a black dark-mode canvas. The illustration plays once,
stops after ten seconds, and is hidden by drafts, conversations, menus, plans,
diffs and dialogs. The existing layout, editor and execution behavior remain.

All **177 Rust tests** and **13 TypeScript UI tests** passed. The optimized
release embeds the exact new bundle. Installed it atomically with a backup,
then passed the Mac PTY acceptance checks for editing, image bytes at the local
provider boundary, graphics output, resizing, shell execution, renderer crash
recovery and terminal restoration. Installed SHA-256:
`3e3bec94780d1657d5344473bb52a93038ad5cb359c3359523ab291fec2da98e`.
The artwork is adapted to terminal character cells; web fonts, browser canvas
resolution and pointer-following effects are not reproduced.

## Header animation and composer correction — 2026-11-14

The large sampled illustration was rejected. Removed its playback, compressed
frames and drawing-code extraction. The actual website pixel ring now stays in
the header while its orange segment travels around the outline for the first
ten seconds, then rests in its original position. Typing does not displace the
logo or conversation. Removed the full-width dotted texture. The importer now
only reads the website SVG pixels; it no longer evaluates JavaScript assets.

Replaced the composer's thin strip and horizontal rules with one continuous
charcoal surface, blank rows above and below the draft, more horizontal padding
and a gap before the footer. Existing multiline editing, selection, image
previews, permissions and cancellation continue through the same interfaces.
Checked the generated layout and successive logo phases visually.

Validation: **177 Rust tests**, **14 TypeScript UI tests**, the TypeScript build
and optimized Rust build passed. The real Mac PTY acceptance test passed all
nine checks, including visible header animation followed by no further accent
updates after ten seconds, large draft editing, thumbnail sequences, original
image bytes at the local mock provider, resizing, shell execution, renderer
crash recovery and terminal restoration. It ran against the optimized release;
the installed executable is byte-identical and passed `--version` and `--help`.
Installation was atomic with a backup, and `~/.local/bin/osmogrep` is first on
PATH. SHA-256:
`4c391f69cf37c07441aaee11db85540f3d3ba311126d69858e79113a8d7d1e99`.
Raw terminal evidence remains under ignored `.context/header-logo-release-validation/`.
Live model quality and hosted PR/E2B delivery were not retested for this UI change.

## Command picker refresh — 2026-11-14

Replaced the bare ten-row command table and bright inverse selection with a
compact charcoal card aligned above the composer. It displays up to six matches,
an accent arrow and soft selected surface, readable descriptions, a position
counter and keyboard hints. The selected match stays in view during scrolling.
Small terminals omit descriptions and decorative rows as space decreases;
light mode uses corresponding light surfaces. Existing Rust filtering and
keyboard execution remain the source of truth.

All **16 TypeScript tests**, **177 Rust tests**, the TypeScript build and optimized
Rust build passed. Added checks cover selection visibility, Unicode commands,
narrow terminals, light mode and keeping approval prompts and the editor visible.
The Mac PTY check passed all ten checks, including opening the new card, arrow
navigation and local execution of `/help`, alongside editing, images, resize,
animation cutoff, shell execution, renderer recovery and terminal restoration.
The initial test expected the help heading to remain onscreen; corrected it to
check the visible final paragraph after the heading scrolled out of view.
Visually inspected the generated menu layout. Installed the byte-identical
release atomically with a backup and verified it is first on PATH. SHA-256:
`0e6bb27927f917d56c617e4a11debd703a23fb159b3c26b4d42ccbe9cb642c15`.
Raw evidence is ignored under `.context/command-picker-validation/`.
No live provider or hosted E2B checks were repeated for this presentation change.

## Interactive central dithered morph — 2026-11-14

The user clarified that the reference animation belongs in the empty center of
the terminal. The header logo is now static. Added an original procedural point
cloud derived from the deployed OsmoGrep pixel ring: it rotates, morphs into a
sphere, then resolves to the original ring in ten seconds. Eight ordered-dithered
Braille dot positions per cell provide fine shading on the black canvas. The
settled ring remains visible while idle. There are no downloaded animation frames,
graphics-protocol requirements, new dependencies or model calls.

Hover tilts the artwork; left click replays its transformation. Rust owns pointer
events, the replay clock and temporary mouse capture. Only unmodified pointer
events inside the artwork are consumed; keyboard and wheel handlers remain.
The parent validates the artwork bounds before enabling interaction. Typing,
conversation, dialogs and plans hide the artwork and release automatic capture.
Explicit `OSMOGREP_MOUSE=off` disables artwork mouse reporting. Renderer failure
and exit restore capture state through the existing cleanup path.

Validation: **179 Rust tests**, **18 TypeScript tests**, TypeScript bundle build
and optimized Rust build passed. Visually inspected six animation phases. A
100-frame procedural-generation microbenchmark at 60×28 cells averaged **1.35 ms
per frame**, excluding IPC and terminal output. The Mac PTY acceptance run passed
all eleven checks, including actual SGR hover/click events, settlement after ten
seconds, mouse release on typing, command navigation, editing, image bytes at the
local provider boundary, resizing, shell execution, renderer recovery and terminal
restoration. Evidence is local under ignored `.context/central-morph-validation/`.

Installed the byte-identical optimized release atomically with a backup and
verified it is first on PATH. SHA-256:
`141c1c3505393bb8b46e6dd6352effc8abd1a722162b4711fd0bfbcc2ed6f6ec`.
Other terminal/font combinations were not checked locally; short windows and the
native fallback omit the central artwork. No hosted execution checks were rerun.

## Finer startup choreography without hover — 2026-11-14

Replaced the sphere transition with a designed ten-second sequence: hold the
OsmoGrep ring, rotate and morph into a white chevron and orange underscore (`>_`),
pause on that terminal mark, then rotate back and rest on the ring. Startup plays
automatically once. Hover and dragging have no influence, and clicks during
playback cannot restart its clock. Clicking the settled mark still replays it.
Automatic mouse reporting now requests button events only, with no hover reports.

Ghostty/Kitty-compatible terminals reuse the existing image rendering and cleanup
path for a procedurally generated PNG at **three times the linear dot resolution**
of the text version: 360×336 samples for a 60×28-cell stage. Both dimensions grow
threefold; the stage size remains stable. The text fallback retains Braille cells.
Surface meshes use typed arrays, and PNG compression/checksums use Node's standard
library; no dependencies, external assets or services were added.

Validation: **179 Rust tests**, **20 UI tests**, TypeScript bundle and optimized
Rust builds passed. Tests check pose holds, return to the exact initial frame,
PNG dimensions, checksums and decompression, graphics placement, hover isolation
and ignoring clicks during playback. Inspected six rendered phases. A warm
100-frame 60×28-cell microbenchmark averaged **9.31 ms** including fine PNG encoding
and **1.08 ms** for text generation; both exclude IPC and terminal output.

All eleven Mac PTY checks passed. The run decoded emitted PNG frames, checked
threefold dimensions, observed automatic changing poses and settlement, confirmed
hover preserves identical image data without restarting playback, then tested
click replay, editing, images, resize, shell execution, fallback and restoration.
Installed the byte-identical release atomically with a backup, first on PATH.
SHA-256: `ea4b7927382dd860044601dab8fd342e6a0007e601fc1404ac0a84becdc835cd`.
The fine path was checked with the Ghostty-compatible protocol; other terminal
implementations and hosted execution were not retested.

## Subtler central artwork — 2026-11-14

Reduced the artwork's projected size by 20% and dark-mode color brightness by
about 30% in both rendering paths. The animation sequence, fine dot resolution,
input geometry and interaction behavior are unchanged. Visually checked the
updated phases; all **20 UI tests**, TypeScript and release builds, and eleven
Mac PTY checks passed. Rust source was unchanged, so its previous 179-test result
was not rerun. The earlier microbenchmark remains attributed to `d3532cf`.

Installed the byte-identical release atomically with a backup, first on PATH.
SHA-256: `d8788b182a4842a1f292125e007c6ec504c003288d0a66e1a904375c0bf52d2b`.

## Origami transformation — 2026-11-14

Replaced the terminal symbol with an original folded-paper bird. The logo
unfolds into twelve shaded triangular facets with a muted orange beak, the wings
fold once around fixed shoulder hinges, and the bird reforms into the ring.
The smaller scale, dim palette, ten-second duration and lack of hover interaction
remain. Both the fine graphics path and text fallback use the same facet geometry.

All **20 UI tests**, TypeScript and optimized Rust builds, and eleven Mac PTY
checks passed. Updated the gesture test to verify wing motion and return to the
open-wing pose, followed by the exact starting ring. Visually inspected six phases.
Rust source was unchanged; its prior test result and microbenchmark remain
attributed to `d3532cf`. Installed the byte-identical release atomically with a
backup, first on PATH. SHA-256:
`b2f3fca126459960c480881f979d5c2d9b2d5f60c997db470eaef1758f5cc035`.

## Ten-concept HTML motion gallery — 2026-11-14

Created the standalone [animation concept gallery](animation-concepts.html) for
user review before terminal implementation. Its ten directions are Lorenz chaos,
a rotating tesseract, Hopf fibers, a Klein-bottle immersion, wave interference,
Conway's Life, Gray–Scott reaction–diffusion, Fourier epicycles, an artistic
accretion disk and a bitonic sorting network. Each has a shared fourteen-second
logo-to-concept-to-logo transition, full-size terminal preview, timeline scrubbing,
pause/replay, speed selection, concept hold and a persistent local shortlist.
Copying a choice prepares approval text; it does not change the installed app.

The HTML embeds all CSS, JavaScript and assets, uses no third-party runtime or
network assets, and includes links explaining the mathematical inspiration.
Self-checks validate finite geometry, the Life seed and growth, and the sorting
network against 32 permutations. Browser checks confirmed ten distinct moving
renders, inspector navigation, scrubbing, stopped redraw while paused, selection
persistence, clipboard text, Escape, reduced motion and layouts at 1440, 390 and
320 pixels. No page errors or external resource requests were observed in Chrome.
Active and idle requestAnimationFrame timing were both approximately 33.3 ms in
the local browser environment; this is not a claim of 60 fps or terminal speed.
Raw screenshots and the local browser check report are in
`/private/tmp/osmogrep-motion-gallery/`.

This is an approval preview. The installed terminal and Rust/TypeScript agent
sources were not changed; terminal implementation and performance verification
await the user's selected direction. The gallery has not been pushed or deployed.

### Terminal workspace controls and approved animation integration — 2026-11-14

Implemented the staged [terminal workspace upgrade](terminal-workspace-plan.md):
curated slash-command discovery, searchable panels, existing tool-policy controls,
persisted remapping/status line, explicit bounded local skills, provider usage
metadata/history and the four approved dim central animations. The Rust agent and
native fallback remain in place. Local validation: 193 Rust tests, 24 UI tests,
16 real Mac PTY checks, installed CLI/MCP/agent compatibility, five controlled
analysis cases and one live OpenAI text smoke all pass. See
[validation measurements](terminal-ui-validation.json). Hosted GitHub/E2B delivery,
real vision interpretation and other operating systems were not revalidated.

### Responsive terminal polish and selected composer materials — 2026-11-14

Implemented the reported layout/padding fixes, path-aware status fitting, distinct
quick picker, contextual permission dock with keyboard decisions, timed activity,
folded tool output and numbered shaded diffs. Tool headings no longer dump edit
payloads, and denied/cancelled diff previews are discarded. Stats have explicitly
labeled sample data with a D toggle; no sample activity enters durable history.

Replaced the automatic Neovim plugin sync/config rewrite with an isolated tmux
workspace and responsive editor placement. Six real editor lifecycle checks pass,
including unsaved-file protection and close/resize recovery. The final release
passes 197 Rust tests, 30 UI tests and 17 Mac PTY checks. The six local HTML material
studies were reviewed; the selected Cotton/light and Graphite/dark treatments are
implemented with stable terminal-cell grain. See `terminal-workspace.md` for keys
and `terminal-ui-validation.json` for evidence and remaining platform limits.

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
