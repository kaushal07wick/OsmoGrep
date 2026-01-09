# ![osmogrep](osmogrep.png)

# **OsmoGrep**

A **terminal-native execution agent** that validates code changes by **running real tests**, not eyeballing diffs.

> **Can your code changes be validated autonomously through safe, deterministic execution?**

## Functionality?

| Area           | What OsmoGrep Does                           |
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

```bash
curl -fsSL https://raw.githubusercontent.com/kaushal07wick/OsmoGrep/master/install.sh | sh
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
| `quit`                               | Exit OsmoGrep                                |


## License

Licensed under **MIT**.
See the [LICENSE](LICENSE) file for details.

