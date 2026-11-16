# Benchmarking OsmoGrep

This setup evaluates the released OsmoGrep coding agent through [Harbor's installed-agent interface](https://docs.harborframework.com/agents/custom-agents). It preserves the native system prompt, tools, provider transport and agent loop. Harbor supplies isolated task containers and an independent verifier; an agent saying “done” is not a passing score.

**Status: the free Debian-container controls passed; the first local-Ollama pilot finished without a successful task completion. Paid-provider runs remain disabled.** The adapter runs against Harbor 0.26.0. Existing Rust/UI/provider fixtures are regression checks, not a public coding-performance score.

## Install the optional runner

Run from the repository root with Python 3.12 or newer. The runner is separate from the product and does not change your installed OsmoGrep binary.

```sh
python3 -m venv /tmp/osmogrep-benchmark-venv
/tmp/osmogrep-benchmark-venv/bin/python -m pip install -r benchmarks/harbor/requirements.txt
LITELLM_LOCAL_MODEL_COST_MAP=True /tmp/osmogrep-benchmark-venv/bin/python -m unittest benchmarks.harbor.test_adapter benchmarks.harbor.test_evaluation -v
```

You can also check CLI discovery without starting a trial:

```sh
PYTHONPATH=. LITELLM_LOCAL_MODEL_COST_MAP=True /tmp/osmogrep-benchmark-venv/bin/harbor agent auth benchmarks.harbor.osmogrep:OsmoGrepAgent
```

To test the real local binary through the adapter without paid calls, run:

```sh
cargo build --locked
PYTHONPATH=. LITELLM_LOCAL_MODEL_COST_MAP=True /tmp/osmogrep-benchmark-venv/bin/python benchmarks/harbor/smoke.py --binary target/debug/osmogrep
```

This smoke check creates a temporary repository and fake localhost model endpoint. It reads one fixture file through the real agent tool loop and checks the collected usage. It does not run the container installer or contact a model provider.

The offline checks cover provider routing for Responses, Chat Completions and Anthropic Messages, custom endpoint delivery, exact prompt delivery without shell interpolation, failed-run propagation, log collection and token accounting. They execute neither installation commands nor model requests.

## Local Ollama checks

A local model can exercise the real agent without paid API calls. Use an installed, tool-capable GGUF model. The local-only runner rejects cloud model metadata and non-loopback endpoints, isolates application configuration, and drops inherited provider credentials. Create a separate alias so the server context agrees with the agent budget:

```sh
printf 'FROM qwen3.5:4b\nPARAMETER num_ctx 16384\nPARAMETER num_predict 2048\nPARAMETER temperature 0.2\nPARAMETER seed 42\n' > /tmp/osmogrep-eval.Modelfile
ollama create osmogrep-eval-qwen35-4b -f /tmp/osmogrep-eval.Modelfile
OSMOGREP_PROVIDER=ollama OSMOGREP_MODEL=osmogrep-eval-qwen35-4b OSMOGREP_BASE_URL=http://127.0.0.1:11434/v1 python3 terminal-ui/scripts/coding-live-acceptance.py --binary target/debug/osmogrep --output /tmp/osmogrep-local-coding.json --local-only
```

These three small coding checks are smoke tests, not the five-task repository pilot. Grader version 2 rejects duplicated README headings, hard-coded answers and incomplete renames; reports preserve candidate files, actual usage, latency and failed outcomes. A failed first attempt and a later successful replay remain separate evidence.

The same pinned Harbor pilot can use `ollama/<model>` with a local server endpoint reachable from its task containers. The adapter accepts explicit `context_window`, `max_output_tokens` and `reasoning_effort` settings and writes them into isolated per-trial model configuration. The local run uses one agent at a time, 16K context, 2K maximum output, thinking disabled, 12 coding rounds and a five-minute agent timeout. These reduced limits and x86 emulation on Apple Silicon make it a diagnostic run, not a comparable Terminal-Bench leaderboard submission. Record setup failures separately from coding failures. Ollama must continue using local weights; a localhost address alone does not rule out a cloud-routed model.

### First local pilot: November 16, 2026

The [recorded results](results/2026-11-16-local-qwen35-4b.json) retain all five attempts with released OsmoGrep 0.3.9 and local Qwen3.5 4B. Four tasks received zero verifier reward; one could not start Kafka. All attempts ended with exceptions: three context-summary connection failures, one agent timeout, and one environment startup failure. The requested 300-second agent timeout did not bound observed phase time on this emulated host; those durations remain in the report. These results establish neither harness parity nor a useful comparative score.

Investigation found that context summarization discarded explicit `reasoning = "none"`. The repair preserves that setting without enabling expensive reasoning options. Its regression checks are separate from the original pilot; the five tasks have not been rerun with the repair. A separate tiny documentation replay exposed a weak grader: it had accepted a duplicated heading. Grader version 2 correctly rejects that replay. Earlier successful smoke checks are not upgraded into stronger scores retroactively.

## Pilot command — only after approving model spending

A running Docker engine with x86_64 Linux support is required. On Apple Silicon this first adapter requires amd64 emulation; use an x86_64 Linux host for comparable performance. From 0.3.9, Linux release builds target glibc 2.35+ (Ubuntu 22.04 and Debian 12 are compatible). Older releases require glibc 2.39+. The installer checks the requirement before downloading. Other task images need a binary built for that image and `--ak skip_install=true`; do not silently change benchmark task images to make the agent fit.

Load a provider key through your normal secret manager or shell environment; never put it in the command or a committed job file. The adapter reuses Harbor's provider/key routing. `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or explicit `--api-key-name` work. Endpoint overrides use `--base-url` and protocol overrides use `--api-format`.

The fixed pilot uses five CPU coding tasks from Terminal-Bench 4.0.0, each pinned to its content digest in [pilot.json](harbor/pilot.json):

| Task | What it exercises |
| --- | --- |
| payments-pipeline-fix | Distributed worker startup and correct notifications |
| html-js-filter | Implementing a constrained feature with adversarial cases |
| session-window-debug | Understanding unfamiliar architecture while preserving read-only files |
| wal-recovery-ordering | Stateful recovery, ordering and strict API constraints |
| vf2-speedup-networkx | A compatible multi-function implementation with performance requirements |

The source dataset resolves to `sha256:39d9f44b40420cde8fdcc087579c0d72a7e14fa3656d603c3f0d22fb35e27732`. Selection was based on coding coverage, CPU resources, and available task metadata, before any OsmoGrep outcomes. A GUI billing task was excluded after inspecting its category. The pilot is not a representative leaderboard sample or a private held-out suite. Benchmark task instructions, tests and solutions are not copied into this repository.

The committed config selects **nop**, so it makes no model calls by default. It runs the independent task verifiers to establish an unchanged-workspace baseline; the containers and verifiers may still take time and download dependencies.

For an actual coding run, the command below is **not offline** and must wait for explicit model-spending authorization. Replace the model with an exact available ID. Other harnesses use the same pinned config with their own `-a`, version and supported reasoning controls. Keep those settings and all results.

```sh
PYTHONPATH=. /tmp/osmogrep-benchmark-venv/bin/harbor run \
  -c benchmarks/harbor/pilot.json \
  -a benchmarks.harbor.osmogrep:OsmoGrepAgent \
  -m openai/EXACT_MODEL_ID \
  --ak version=0.3.9 --ak max_iterations=90 \
  --job-name osmogrep-pilot
```

Summarize a completed job without contacting a provider:

```sh
python3 benchmarks/harbor/report.py /tmp/osmogrep-benchmark-results/osmogrep-pilot --output /tmp/osmogrep-pilot-report.json
```

The report retains failures/errors/ungraded trials and records task checksums, model/version/settings, measured agent time, reported input/output/cache, tool calls and read targets. Missing cost or usage stays unknown. Native response counts include reported successful responses, not every retry or failed request. Repeated reads alone do not prove wasted work. Only direct trial results are read; agent-created result files cannot add passing trials. Multi-step reward interpretation is not supported yet; this pilot uses single-step tasks.

## Free container validation

[contract.json](harbor/contract.json) runs a tiny, locally authored task three ways in separate Debian 12 containers: no-op must fail, the fixture solution must pass, and the real native agent using a deterministic localhost model must pass. The grader checks actual code behavior and preservation of user content. This calibrates infrastructure, not model intelligence.

The `Benchmark infrastructure (no paid models)` CI job builds a compatible binary on Ubuntu 22.04, validates the pinned configs, runs these controls through Harbor, checks native tool/usage delivery, and uploads the full evidence. It uses no provider secret. Locally, with a compatible Linux binary and Docker:

```sh
cp /path/to/linux/osmogrep benchmarks/harbor/tasks/adapter-contract/environment/osmogrep
PYTHONPATH=. LITELLM_LOCAL_MODEL_COST_MAP=True OSMOGREP_FIXTURE_KEY=fixture-key /tmp/osmogrep-benchmark-venv/bin/harbor run -c benchmarks/harbor/contract.json
python3 benchmarks/harbor/report.py /tmp/osmogrep-harbor-contract/osmogrep-adapter-contract --check-contract --output /tmp/osmogrep-contract-report.json
```

The existing `harness-acceptance.py` and `long-run-acceptance.py` CI checks cover repeated compaction, preserved instructions, cancellation, ownership, crash recovery and 100+ model rounds using local deterministic fixtures. Those remain separate from graded real-model performance and actual multi-hour endurance.

The installer downloads the pinned Linux release and verifies its SHA-256 sidecar before extracting. Full tool permissions apply inside the task container only; the adapter does not change interactive permissions, enable teams or load your personal sessions. Each trial uses its own configuration directory. The task instruction is uploaded as a file, not interpolated into a shell command. Headless mode never opens the interactive updater.

Keep `events.jsonl`, `stderr.log`, Harbor's job configuration, verifier output and per-task results. Events and task files may contain sensitive material; inspect them before publishing. Input totals include cached input; cache is a subset, not extra tokens. Unknown usage and dollar cost stay unknown. A 90-iteration limit is not a dollar cap; Harbor's task timeout and concurrency controls are not spending limits either.

## A comparison we can defend

1. Start with five fixed tasks to check installation, provider requests, logs and independent grading. Report this as a pilot, not a leaderboard result.
2. Compare harnesses on identical tasks, model snapshot, reasoning settings, tools/network access, resources, request/token limits and timeout. Preserve each harness's native prompt and disclose it; no per-task prompt fixes or selected successful retries.
3. Run the complete pinned dataset, preferably with multiple attempts. Publish pass rate with uncertainty, errors/refusals separately, all attempts, reported tokens, measured runtime and measured cost. Count failed attempts too.
4. Use [SWE-bench Verified](https://www.swebench.com/SWE-bench/) separately for repository issue-to-patch work. Its scores are not interchangeable with Terminal-Bench.

The official [Terminal-Bench 4.0 release](https://www.tbench.ai/news/terminal-bench-4-0) explains that major dataset/resource changes require fresh trials. We pin 4.0.0 here to match the version claimed by Unharnessed, not whichever dataset happens to be latest.

## What the Unharnessed result establishes

Checked 2026-11-16 against the [site](https://unharnessed.dev/en#bench) and [published run records](https://unharnessed.dev/run): 47 passing records out of 66 is 71.2%. The per-task costs sum to about $804.52. Its persistent Python REPL and batching are plausible useful design choices.

The records also disclose mixed Opus versions on `atrx-vep-crispr` and `formal-crypto`, an OOM rerun for `jax-speedrun-gpu`, and a prompt-change rerun for `production-planning`. The landing-page comparison graphic contains a “Placeholder data” label. These are reasons to distinguish a self-reported demonstration from a reproduced, matched comparison; they do not by themselves invalidate every result. A shell-capable harness can already batch Python work, so the claim that fixed tools necessarily require one model call per file is too broad.

We have not reproduced their runs, verified their reported bills, or established that OsmoGrep outperforms any harness. This adapter prepares that measurement without inventing a score.
