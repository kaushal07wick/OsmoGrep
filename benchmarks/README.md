# Benchmarking OsmoGrep

This setup evaluates the released OsmoGrep coding agent through [Harbor's installed-agent interface](https://docs.harborframework.com/agents/custom-agents). It preserves the native system prompt, tools, provider transport and agent loop. Harbor supplies isolated task containers and an independent verifier; an agent saying “done” is not a passing score.

**Status: setup prepared; no scored model run or API spending authorized.** The adapter contracts run against Harbor 0.26.0 with fake execution environments. Docker-based installation and task grading still need an actual benchmark run. Existing Rust/UI/provider fixture tests are regression checks, not a public benchmark score.

## Install the optional runner

Run from the repository root with Python 3.12 or newer. The runner is separate from the product and does not change your installed OsmoGrep binary.

```sh
python3 -m venv /tmp/osmogrep-benchmark-venv
/tmp/osmogrep-benchmark-venv/bin/python -m pip install -r benchmarks/harbor/requirements.txt
LITELLM_LOCAL_MODEL_COST_MAP=True /tmp/osmogrep-benchmark-venv/bin/python -m unittest benchmarks.harbor.test_adapter -v
```

You can also check CLI discovery without starting a trial:

```sh
PYTHONPATH=. LITELLM_LOCAL_MODEL_COST_MAP=True /tmp/osmogrep-benchmark-venv/bin/harbor agent auth benchmarks.harbor.osmogrep:OsmoGrepAgent
```

The offline checks cover provider routing for Responses, Chat Completions and Anthropic Messages, custom endpoint delivery, exact prompt delivery without shell interpolation, failed-run propagation, log collection and token accounting. They execute neither installation commands nor model requests.

## Pilot command — only after approving model spending

A running Docker engine with x86_64 Linux support is required. On Apple Silicon this first adapter requires amd64 emulation; use an x86_64 Linux host for comparable performance. The current Linux release requires glibc 2.39+, so the installer supports compatible Debian/Ubuntu images (for example Ubuntu 24.04). It rejects older libc versions before installing. Other task images need a binary built for that image and `--ak skip_install=true`; do not silently change benchmark task images to make the agent fit.

Load a provider key through your normal secret manager or shell environment; never put it in the command or a committed job file. The adapter reuses Harbor's provider/key routing. `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or explicit `--api-key-name` work. Endpoint overrides use `--base-url` and protocol overrides use `--api-format`.

The example below is **not an offline command**. Replace the model with an exact available model ID before running. It selects the first five tasks in the pinned dataset: record their names before comparing another harness.

```sh
PYTHONPATH=. /tmp/osmogrep-benchmark-venv/bin/harbor run \
  -d terminal-bench/terminal-bench@4.0.0 \
  -a benchmarks.harbor.osmogrep:OsmoGrepAgent \
  -m openai/EXACT_MODEL_ID \
  --ak version=0.3.8 --ak max_iterations=90 \
  --n-tasks 5 --n-concurrent 1 --n-attempts 1 --max-retries 0 \
  --jobs-dir /tmp/osmogrep-benchmark-results --job-name osmogrep-pilot
```

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
