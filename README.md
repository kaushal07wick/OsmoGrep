
![osmogrep](osmogrep.svg)

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


## Fundamentals

OsmoGrep is a **context-first automation tool** for local repositories.

It operates under these rules:

* **Context before execution**
  Language, test framework, branch, and working tree state are detected mechanically before any action.

* **No implicit mutations**
  Nothing is checked out, applied, or executed without an explicit command.

* **Diff-scoped reasoning**
  Actions are based on the current diff, not the entire repository.

* **Isolated execution**
  Automation runs in agent branches; the original branch remains untouched.

* **Human-controlled flow**
  OsmoGrep surfaces state and options but does not make irreversible decisions.

It is designed to work safely on **uncommitted code** in active development environments.


## Terminal UI
![osmogrep-tui](osmogrep.png)

* Cursor-aware command input
* Mouse-focusable command box
* Explicit execution logs
* Command History
* Autocomplete
* Dynamic Status

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




## Build & Run
```bash
cargo build
cargo run
```

Run **inside any Git repository**.
