What this repo does: OsmoGrep is a terminal coding agent with model adapters,
workspace tools, persistent sessions, a TypeScript UI and optional device/MCP
integrations. This review assumes one to three instances on one Mac, large
repositories and long conversations. It prioritizes data preservation, correct
tool delivery and input responsiveness over cosmetic refactoring.

Reviewed November 16–17, 2026. Earlier resolved findings describe fixes in 0.3.12
and subsequent workspace changes. The November 17 follow-up records current
structural findings and separately identifies the messaging fixes made during
this review. Open findings are explicitly marked.

## Must fix

1. **Saving a model could erase unrelated settings — fixed** (`src/agent.rs`,
   `merge_agent_config`; `src/storage.rs`, `update_toml`).
   - **What this is:** Model and credential settings share `config.toml` with MCP,
     sandbox, reasoning and other settings.
   - **Problem:** Serializing the agent's smaller config type back over the whole
     file dropped fields it did not know. Independent saves could also overwrite
     another instance's settings changes.
   - **Fix:** Lock, re-read, merge owned fields, and atomically replace. Regression
     checks preserve unrelated sections, remove migrated plaintext keys, retain
     twelve concurrent independent updates and leave invalid TOML untouched.
   - **If we skip it:** A normal model change can disable integrations or lose
     user configuration. This fix must stay covered.

2. **Device screenshots were not delivered as model images — fixed**
   (`src/tool_budget.rs`, `model_output`; `src/provider.rs`, `payload`).
   - **What this is:** MCP tools return screenshot image blocks alongside text.
   - **Problem:** JSON-stringifying and truncating the result turns pixels into
     base64 text; a model cannot inspect the actual screen.
   - **Fix:** Validate image type, bytes and dimensions, preserve typed image
     parts, and map them to each API. Tests cover stdio image delivery and the
     placement of images after parallel Chat Completions tool results.
   - **If we skip it:** An agent can issue screen input without receiving the
     observation needed to choose and verify it.

3. **A simultaneous external save can still race an agent replacement — open**
   (`src/file_guard.rs`, `replace`; `src/tools/mod.rs`, `check_receipt`).
   - **What this is:** Read receipts and repeated byte checks detect stale edits;
     team file claims coordinate participating OsmoGrep processes.
   - **Problem:** An editor can save after the final byte check but before the
     rename. That small interval still allows its changes to be replaced. Claims
     do not lock an arbitrary external editor.
   - **Fix:** Retain a recoverable pre-replacement version at the shared write
     boundary and detect/report competing saves. Keep documented guarantees
     narrower than an atomic conditional replacement across unrelated editors.
   - **If we skip it:** Rare simultaneous saves can lose external edits. The
     current checks reduce this risk; they do not eliminate it.

4. **Unbounded web responses could consume excessive memory — fixed**
   (`src/tools/web_fetch.rs`, `bounded_text`).
   - **What this is:** `web_fetch` presents a small text excerpt.
   - **Problem:** It previously downloaded the entire response before clipping
     characters. A very large endpoint response could exhaust agent memory.
   - **Fix:** Read at most four bytes per requested Unicode character plus one
     sentinel byte. The test checks the actual bytes read and complete emoji.
   - **If we skip it:** A small requested preview can still trigger a huge
     allocation. The new bound prevents that allocation.

## Should fix

5. **Opening the editor could hide chat — fixed in 0.3.12** (`src/editor.rs`).
   - **What this is:** The editor shares a workspace with the agent.
   - **Problem:** The narrow-width branch created a separate tmux window, leaving
     only Neovim visible. Kitty images also fell back under tmux, and coarse
     fallback dots remained beneath native orb images.
   - **Fix:** Keep both panes in one window, stack only when narrow, forward
     supported graphics and draw only one orb representation. Twelve real
     PTY/editor checks and the exact 32-frame pixel contract cover the repair.
   - **If we skip it:** Users cannot converse beside edits and see a different
     working animation from the approved one.

6. **Indexing did redundant work and could trust inconsistent cache files — fixed**
   (`src/context/indexer.rs`, `load_or_build`, `scan_sources`, `finalize_calls`).
   - **What this is:** The background Python/Rust index scans source hashes and
     maintains extracted symbols and calls.
   - **Problem:** Generated directories were traversed before file filtering;
     ordinary length changes forced a rebuild; an ancestor named `target` could
     exclude the whole project. Two cache writes could leave mismatched state,
     and derived call lists accumulated during repeated incremental updates.
   - **Fix:** Prune relative directories during one scan, update changed hashes,
     clear derived calls before rebuilding them, and lock/atomically write a
     versioned cache pair with a context digest. Reject linked cache paths and
     bound individual source reads. Tests exercise change, removal, ignored
     subtrees, ancestor names and mismatched cache recovery.
   - **If we skip it:** Startup can spend time walking dependencies or present
     stale/duplicated symbols after edits.

7. **Settled chat text was highlighted on each animation frame — fixed**
   (`terminal-ui/src/code.ts`, `cachedCodeBlocks`).
   - **What this is:** A small animation changes while most transcript text stays
     identical.
   - **Problem:** Formatting the same 1,000-line fixture repeatedly measured
     roughly 30.2 ms at the median in the renderer alone.
   - **Fix:** Reuse bounded formatted blocks while layout and copy targets are
     positioned each frame. The same warm fixture measured roughly 2.5 ms after
     the change. Invalidation and eviction have a regression check.
   - **If we skip it:** Long chats waste rendering work while only the activity
     indicator changes. This measurement excludes Rust and model latency.

8. **Renderer waiting fixed; snapshot computation remains open** (`src/ui/typescript.rs`,
   `TypeScriptUi::draw`, `view`).
   - **What this is:** Rust sends a full snapshot to Node, while retaining the last validated frame.
   - **Problem:** Snapshot construction still serializes unchanged history and reconstructs active diffs, even when only animation time changed.
   - **Fix:** v0.3.20 polls one in-flight frame without blocking keyboard processing, preserves state revisions across delayed frames and limits event-drain batches. Captured commands no longer inherit terminal stdin. A real terminal fixture covers typing during a model request and after completion with a minified diff. Sending transcript/diff revisions and measuring latency under deliberately stalled rendering remain follow-ups.
   - **If we skip the remaining work:** CPU-heavy snapshot construction can still delay large sessions; the current checks do not establish an end-to-end latency bound.

9. **Diff display limits do not bound diff computation — open in part**
   (`src/ui/diff.rs`, `Diff::from_texts`).
   - **What this is:** The UI computes a full line diff, then limits display rows.
   - **Problem:** Large replacements still incur full comparison work before
     display folding, and unchanged diffs are rebuilt during animation frames.
     The old early return also undercounted totals; that part is now fixed.
   - **Fix:** Compute and cache each diff at edit completion with a deadline,
     count totals separately from rendered rows, and keep full contents available
     on expansion. Totals and full expansion now have a regression check; caching
     and a bounded comparison strategy remain open.
   - **If we skip it:** A large generated-file edit can still stall the UI even
     though its totals and expanded contents are now correct.

10. **Core modules combine several independently changing jobs — open**
    (`src/main.rs`, `src/agent.rs`, `src/commands.rs`).
    - **What this is:** Each file contains roughly 2,800–3,600 lines including
      tests, spanning lifecycle, configuration, execution and UI events.
    - **Problem:** Adding a device control or provider setting crosses broad
      modules; unrelated policies and dispatch logic become harder to review.
    - **Fix:** Follow the responsibility-based extraction order in
      [architecture](architecture.md#remaining-restructuring-in-dependency-order).
      Keep the current protocol and acceptance tests at each boundary.
    - **If we skip it:** Features remain possible, but changes carry a larger
      review and regression surface. File size alone is not the reason to split.

11. **A hyphen-prefixed search pattern could become an rg option — fixed**
    (`src/tools/search.rs`, directory search).
    - **What this is:** Search sends model-supplied patterns to ripgrep.
    - **Problem:** Without `--`, an input such as `--files` changes command
      behavior instead of being searched as text. A benign preprocessor probe
      did not execute a script, so this review does not claim demonstrated code
      execution through this path.
    - **Fix:** End option parsing before the pattern; keep the literal-pattern
      regression check. No shell quoting or new dependency is required.
    - **If we skip it:** Some valid patterns produce incorrect directory-search
      behavior or unexpected output.

12. **Hard-repository completion evidence is still weak — open**
    (`benchmarks/results/2026-11-16-local-qwen35-4b.json`).
    - **What this is:** The local five-task pilot measures the combined model and
      harness on unfamiliar tasks.
    - **Problem:** It has zero successful completions, with recorded startup,
      summary and timeout failures. Unit tests and UI polish do not establish
      parity with other harnesses.
    - **Fix:** Use the pinned tasks and recorded diagnostics to repair the failing
      context handoff, then repeat under the same local model and limits. Keep
      infrastructure failures separate from solved-task scores.
    - **If we skip it:** Claims about long-run reliability, efficiency or parity
      remain unsupported. No new model run or paid call was made in this review.

Verdict: The core has useful guardrails and substantial regression checks. The immediate
settings, screenshot delivery, editor visibility and wasteful formatting issues
are repaired. External-save races, blocking presentation, large diffs and real
task completion evidence remain the highest-value next work. This is not a
certification of harness parity or “ultimate Rust performance.”

No dependency removal is claimed: the native renderer and optional workflows
still have callers. No numeric line-savings estimate is justified by a completed
removal.

Not checked: line-by-line review of every triage, voice and optional intelligence
branch; dependency supply-chain/binary audit; physical iPhone control; live Mac
guest input; Windows/Linux desktop acceptance; paid provider behavior; a new
multi-hour task run. Readiness checks found no configured VM and no installed
iOS runtime. The native-pane and actual companion MCP-discovery checks passed.

## November 17 follow-up

The Rust application remains a single binary crate with a bundled TypeScript
renderer. There is no justification for turning each feature into a crate.
Storage, process supervision, file guards and provider transport already provide
useful boundaries. Runtime event ownership and presentation are the main weak
connections between them.

### Must fix

13. **Saved edit history differs between execution modes — open**
    (`src/main.rs`, `run_headless`; `src/rpc.rs`, event loop).
    - **What this is:** Interactive, headless and RPC modes consume the same
      agent events and save sessions.
    - **Problem:** Interactive mode records `ToolDiff` in both session changes
      and undo history. Headless mode records neither; RPC records only session
      changes. A local fixture changed a file successfully but saved zero session
      changes and zero undo entries.
    - **Fix:** Apply session events through one shared reducer. Keep terminal,
      JSON emission and permission-response effects in their respective adapters.
    - **If we skip it:** Resuming an automated session loses the edit/diff/undo
      capabilities users expect from an interactive session.

14. **A failed undo consumes the undo entry — open**
    (`src/commands.rs`, `undo_last_change`).
    - **What this is:** Undo uses guarded replacement to preserve later edits.
    - **Problem:** The command pops the undo stack before attempting restoration.
      A content conflict or write error returns without putting that entry back.
    - **Fix:** Inspect the last entry, restore successfully, then pop and persist.
      Add one conflict-and-retry regression check.
    - **If we skip it:** A recoverable undo conflict becomes an unavailable undo
      operation on the next attempt.

### Should fix

15. **Configuration failures become silent defaults — open**
    (`src/agent.rs`, `load_config`, `Agent::new`; `src/credentials.rs`, `get`).
    - **What this is:** Startup chooses the provider/model and reads credentials.
    - **Problem:** `.ok()` discards malformed TOML, read failures and credential
      backend errors. A broken config can look like a default model selection or
      a missing API key instead of reporting the actual problem.
    - **Fix:** Distinguish absent settings/secrets from invalid or inaccessible
      ones. Carry the diagnostic to startup and model selection.
    - **If we skip it:** Users debug the wrong provider or re-enter a credential
      that was already saved correctly.

16. **Sequential and parallel tools duplicate the result lifecycle — open**
    (`src/agent.rs`, sequential tool handling and parallel batch handling).
    - **What this is:** Both paths add loop warnings, summarize results, emit
      events, update the ledger and construct model-visible tool outputs.
    - **Problem:** The same postprocessing sequence exists twice. Changing a
      result warning, redaction or event requires finding both copies; checkpoint
      and execution-policy differences are mixed into the duplicated sequence.
    - **Fix:** Share result finalization while retaining the distinct execution
      and checkpoint rules. Separate configuration/catalog code from the runner.
    - **If we skip it:** Equivalent tool results can drift across execution modes
      as new providers and tools are added.

17. **An obsolete verification gate exists only in tests — open**
    (`src/verify_stop.rs`; test-only declaration in `src/main.rs`).
    - **What this is:** A 97-line module builds mandatory verify-before-finishing
      instructions and tests its own behavior.
    - **Problem:** It is compiled only under `cfg(test)` and has no production
      callers, while architecture and PRRE docs still describe it as a feature.
      Whole-tree searches found only its declaration, self-tests and those docs.
    - **Fix:** Delete the module and its declaration; correct the two stale doc
      references. Preserve the separately used verification ledger.
    - **If we skip it:** Passing tests continue to suggest coverage of behavior
      the actual application does not execute.

18. **CI separates renderer tests from the native frame consumer — open**
    (`.github/workflows/ci.yml`; `src/ui/typescript.rs`).
    - **What this is:** Node renders frames that Rust validates and displays.
    - **Problem:** TS snapshots and Rust tests run separately. The earlier
      multi-row copy-control issue passed renderer tests but failed Rust's
      one-row hit-area contract. Manual native checks caught it.
    - **Fix:** Keep one real-binary pseudo-terminal smoke check in CI, using a
      local fixture provider. Exercise a code-copy control and an expansion or
      message action through the actual frame boundary.
    - **If we skip it:** A visually correct TS change can silently trigger native
      fallback when installed.

### Cleanup order

1. Fix shared event application and undo failure handling.
2. Move interactive run/event handling out of `main.rs` into `runtime/`.
3. Separate agent execution, tool-result finalization and provider configuration.
4. Split command handlers by sessions, settings, workspace and execution.
5. Keep team storage, routing policy and presentation separate. This change
   already extracts the pure language-routing policy into `team/policy.rs`.
6. Cache presentation by revision and remove frame waits from the input loop.

Keep public event and persistence contracts stable during these moves. A smaller
file is useful only when it owns a coherent job. Do not add wrapper traits,
plugins, services or crates merely to distribute the same coupling.

Messaging fixes made alongside the audit include live-recipient routing, stable
labels, exact body preservation, post-response receipts, paginated history,
transient notices and typed peer log entries. The default conversation now uses
the same base prompt as the runtime; the old duplicate instruction string was
removed. These fixes do not resolve the open architectural findings above.

Verdict: Useful foundations, but shared runtime state and rendering need cleanup
before claiming consistently smooth long runs or parity with other harnesses.

Lean: at least 99 Rust lines can be removed by deleting the unused 97-line gate
and its two-line declaration. No dependency removal is justified by this audit.

Not checked: Clippy is unavailable in the installed Rust toolchain. No new paid
provider run, multi-hour endurance run or line-by-line audit of every optional
subsystem was performed. The review used code tracing, local reproductions,
unit/UI checks, independent-process fixture checks and native terminal smoke
checks; it is not a performance or security certification.
