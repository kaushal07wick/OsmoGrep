# ![  osmogrep](  osmogrep.png)

# **  osmogrep**

A **terminal-native execution agent** that validates code changes by **running real tests**, not eyeballing diffs.

> **Can your code changes be validated autonomously through safe, deterministic execution?**

![  osmogrep-working](  osmogrep.gif)
## Functionality?

| Area           | What   osmogrep Does                           |
| -------------- | -------------------------------------------- |
| **Input**      | Uncommitted git diffs                        |
| **Context**    | AST-based symbol extraction + test graph     |
| **Action**     | Generates + runs real tests                  |
| **Isolation**  | Executes in temporary agent branches only    |
| **Validation** | Single-test or full-suite execution          |
| **Artifacts**  | Logs, reports, test files, structured output |
| **UI**         | Highly optimized terminal TUI                |

Everything runs in an **isolated agent branch**, fully reversible and safe.

Supports:

* Local models (Ollama)
* OpenAI-compatible APIs


## Core Capabilities

| Capability                 | Description                                       |
| -------------------------- | ------------------------------------------------- |
| **Diff Inspector**         | Parses diffs, extracts symbols, metrics, risk     |
| **Context Graph**          | AST-driven symbol and call-graph extraction       |
| **Single-Test Generation** | LLM generates minimal reproduction tests          |
| **Full Suite Execution**   | Runs repo tests and builds a structured report    |
| **Deterministic Cache**    | Avoids regenerating tests that already passed     |
| **Sandboxed Execution**    | Uses OS-level worktrees for isolation             |
| **Artifacts**              | Test files, logs, markdown, machine-readable JSON |

Designed for real engineering workflows—not toy demos.


## Installation

### from crates.io

```bash
cargo install   osmogrep
```

### Install latest from github


```bash
curl -fsSL https://raw.githubusercontent.com/kaushal07wick/  osmogrep/master/install.sh | sh
```


Then run inside **any git repository**:

```bash
  osmogrep
```

## Usage

Write code → `git add .` → run:

```bash
  osmogrep
```

The agent inspects your diff, builds context, and executes in a sandbox branch.

## Commands Overview

| Command                              | Description                                  |
| ------------------------------------ | -------------------------------------------- |
| `help`                               | Show all commands                            |
| `inspect`                            | Analyze git changes and build context        |
| `changes`                            | List analyzed changes                        |
| `changes <n>`                        | Open diff in split TUI                       |
| `agent run <n>`                      | Generate + run test for specific change      |
| `agent run --all`                    | Generate tests for all diffs (parallel mode) |
| `agent run --full`                   | Execute entire test suite with report        |
| `model use openai/anthropic <model>` | Configure remote model                       |
| `model use ollama <model>`           | Use local Ollama model                       |
| `model show`                         | Show active model                            |
| `agent cancel`                       | Cancel running agent                         |
| `agent status`                       | Show current status                          |
| `branch list`                        | List available git branches                  |
| `clear` / `logs clear`               | Clear logs                                   |
| `close`                              | Close result panel                           |
| `quit`                               | Exit   osmogrep                                |

## Particle Physics

osmogrep is domain-agnostic, but it fits especially well with **particle physics / HEP** workflows where changes must be validated against **real, reproducible executions** (unit tests, integration tests, small-sample analyses).

Common ways to use it in HEP projects:

- **Reconstruction & simulation code**: catch regressions by generating focused tests for a specific diff (e.g., a tracking or calorimeter module) and executing them in an isolated worktree.
- **Analysis pipelines**: validate that an algorithmic change still produces expected event selections or derived quantities by running a minimal “golden sample” test.
- **Histogram/ntuple stability checks**: treat reference outputs (small ROOT files, JSON summaries, or text snapshots) as fixtures and have tests assert on key quantities (yields, means, efficiencies).
- **Deterministic review**: instead of reviewing diffs in the abstract, you get *evidence*—logs + test artifacts—showing what actually passed.

Practical tips:

- Keep a small, fast test dataset in-repo (or downloadable in CI) and wire it into your test command.
- Prefer “thin” assertions: check a few physics-relevant scalars (event counts, cutflow totals, χ²/KS distance thresholds) rather than entire files.
- If your stack is containerized (common in HEP), run osmogrep inside the same image to keep execution reproducible.

## License

Licensed under **MIT**.
See the [LICENSE](LICENSE) file for details.

