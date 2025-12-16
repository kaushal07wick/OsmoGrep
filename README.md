# **OsmoGrep**

<p align="left">
  <img src="https://img.shields.io/badge/status-active-success?style=flat-square" />
  <img src="https://img.shields.io/badge/interface-terminal--ui-blue?style=flat-square" />
  <img src="https://img.shields.io/badge/language-rust-orange?style=flat-square" />
  <img src="https://img.shields.io/badge/human--in--the--loop-required-critical?style=flat-square" />
  <img src="https://img.shields.io/badge/no%20implicit-git%20mutations-red?style=flat-square" />
  <img src="https://img.shields.io/badge/branch%20reuse-default-green?style=flat-square" />
  <img src="https://img.shields.io/badge/build-cargo-orange?style=flat-square" />
</p>


**OsmoGrep** is an interactive, terminal-native **AI E2E execution agent** designed to safely run tests, experiments, and analysis **on uncommitted working trees** without polluting your main branches.

It gives you a **controlled execution sandbox** inside your own Git repository, with explicit user intent for every destructive action.

---

## Why OsmoGrep Exists


Most tools assume:

* Code is committed
* Branches are disposable
* Automation should “just run”

That is wrong.

Real engineering happens in **dirty working trees**, half-written code, and local experiments.

OsmoGrep is built for that reality.

> **“First, do no damage.”**

---

## Core Principles



* **No implicit mutations**
  Nothing is checked out, applied, or deleted unless *you* command it.

* **Works on uncommitted code**
  Your working tree stays intact until execution is explicitly triggered.

* **Branch safety by default**
  Existing agent branches are reused. New ones are created only on request.

* **Human-in-the-loop execution**
  Every action is visible, logged, and reversible.

---

## What OsmoGrep Does


1. Detects repository context
2. Detects base branch
3. Detects existing `osmogrep/*` agent branches
4. Allows you to:

   * Reuse an agent branch
   * Create a new agent branch
   * Apply current working tree on demand
   * Execute tests or automation
   * Roll back cleanly

All from a **single terminal UI**.

---

## Terminal UI

```
┌──────────────────────────────────────────┐
│ OSMOGREP — AI E2E Testing Agent          │
└──────────────────────────────────────────┘
┌ Status ─────────────────────────────────┐
│ Phase: Idle                              │
│ Base: master                             │
└──────────────────────────────────────────┘
┌ Execution ──────────────────────────────┐
│ Base branch detected: master             │
│ Reusing osmogrep/20251216162555          │
│ Uncommitted changes detected             │
└──────────────────────────────────────────┘
┌ Command ────────────────────────────────┐
│ /exec                                   │
└──────────────────────────────────────────┘
```

* Cursor-aware command input
* Mouse-focusable command box
* Command history & autocomplete
* Explicit execution logs

---

## Commands



All commands are prefixed with `/`.

| Command     | Action                                       |
| ----------- | -------------------------------------------- |
| `/help`     | Show all commands                            |
| `/exec`     | Checkout agent branch and apply working tree |
| `/new`      | Create a new agent branch (no checkout)      |
| `/rollback` | Return to original branch                    |
| `/quit`     | Exit OsmoGrep                                |

### Input UX

* `Tab` → autocomplete
* `↑ / ↓` → history navigation
* `Enter` → execute command
* Mouse click → focus input

---

## Execution Model

**Nothing runs automatically.**

1. OsmoGrep inspects your repo
2. Your working tree remains untouched
3. On `/exec`:

   * Agent branch is checked out
   * Working tree diff is applied
   * Tests or automation can run
4. On `/rollback`:

   * You return to the original branch
   * Agent state is cleaned

This guarantees **zero accidental data loss**.

---

## Project Structure



```
src/
├── main.rs        # Event loop, input handling
├── state.rs       # State machine & command UX
├── ui.rs          # Ratatui rendering
├── commands.rs    # Commands, hints, autocomplete
├── machine.rs    # Execution lifecycle
├── git.rs        # Git operations
└── logger.rs     # Structured logging
```

Each module does **one thing well**.

---

## Build & Run



```bash
cargo build
cargo run
```

Run **inside any Git repository**.

---

## What OsmoGrep Is NOT


* Not a CI system
* Not a background runner
* Not a blind automation tool
* Not a Git wrapper

It is a **deliberate execution agent**.
