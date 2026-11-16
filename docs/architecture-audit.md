What this repo does: OsmoGrep is a terminal coding agent with model adapters,
workspace tools, persistent sessions, a TypeScript UI and optional device/MCP
integrations. This review assumes one to three instances on one Mac, large
repositories and long conversations. It prioritizes data preservation, correct
tool delivery and input responsiveness over cosmetic refactoring.

Reviewed November 16, 2026. Resolved findings below describe fixes in 0.3.12 and
the following workspace integration change. Open findings are explicitly marked.

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

8. **Presentation still blocks the input thread — open** (`src/ui/typescript.rs`,
   `TypeScriptUi::draw`, `view`).
   - **What this is:** Rust sends a full snapshot to Node and waits for its frame.
   - **Problem:** A busy renderer can hold the interactive loop for up to 1.5
     seconds. Every snapshot also serializes unchanged history and reconstructs
     active diffs, even when only animation time changed.
   - **Fix:** Retain the last validated frame, consume newer frames without
     blocking input, and send transcript/diff changes by revision. First measure
     end-to-end key latency under a long transcript and stalled renderer.
   - **If we skip it:** The text cache helps average rendering time, but it does
     not guarantee smooth typing during slow frames.

9. **Diff display limits do not bound diff computation — open**
   (`src/ui/diff.rs`, `Diff::from_texts`).
   - **What this is:** The UI computes a full line diff, then limits display rows.
   - **Problem:** Large replacements still incur full comparison work before the
     500-row cap; early return also makes addition/removal totals incomplete.
   - **Fix:** Compute and cache each diff at edit completion with a deadline,
     count totals separately from rendered rows, and show an explicit bounded
     fallback for oversized comparisons.
   - **If we skip it:** A large generated-file edit can stall the UI and its
     displayed totals can undercount changes.

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
