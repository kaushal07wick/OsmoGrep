
````md
 ██████╗  ██████╗ ███╗   ███╗  ██████╗  ██████╗ ██████╗ ███████╗██████╗ 
██╔═══██╗██╔════╝ ████╗ ████║ ██╔═══██╗██╔════╝ ██╔══██╗██╔════╝██╔══██╗
██║   ██║███████╗ ██╔████╔██║ ██║   ██║██║  ███╗██████╔╝█████╗  ██████╔╝
██║   ██║╚════██║ ██║╚██╔╝██║ ██║   ██║██║   ██║██╔══██╗██╔══╝  ██╔═══╝ 
╚██████╔╝███████║ ██║ ╚═╝ ██║ ╚██████╔╝╚██████╔╝██║  ██║███████╗██║     
 ╚═════╝ ╚══════╝ ╚═╝     ╚═╝  ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝     
````

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

## 🧠 How OsmoGrep Thinks

OsmoGrep does **not** behave like a CI bot.
It behaves like a **careful engineer sitting beside you**.

Its mental model is deliberate, staged, and conservative by design.

---

## 🧩 1. Context Before Action

OsmoGrep **never executes first**.

It begins by answering simple, mechanical questions:

* *What language is this repository actually written in?*
* *Is there a test framework, or none at all?*
* *What branch am I on?*
* *Is the working tree dirty?*

These answers are **derived, not inferred**.
No LLM. No guessing. No “best effort”.

> **If the context is wrong, everything downstream is wrong.**

That is why OsmoGrep surfaces this information **visibly in the UI** before you do anything.

---

### 🧠 2. Static Facts > AI Assumptions

OsmoGrep treats AI as a **tool**, not an authority.

Before any LLM is even considered, OsmoGrep establishes **hard constraints**:

* Language is detected via file extensions
* Test frameworks are detected via repo structure
* Execution capability is validated mechanically

If no test framework exists, OsmoGrep will say:

> ⚪ *No tests detected*

It will **not** hallucinate one.

---

### ✂️ 3. Diffs Are the Unit of Thought

OsmoGrep does not reason about “the repo”.

It reasons about **what changed**.

Everything downstream — tests, execution, validation — is anchored to:

* The **exact diff**
* The **surrounding code**
* The **type of change** (logic, interface, state)

> **A pure function change is not an E2E test candidate.**

OsmoGrep refuses to do meaningless work.

---

### 🧭 4. Intent Is Explicit

OsmoGrep assumes **you are in control**.

Nothing happens unless you say so:

* `/exec` means *you want execution*
* `/new` means *you want isolation*
* `/rollback` means *you want safety*

There are **no background actions**.
There are **no silent checkouts**.
There are **no surprise mutations**.

> *Automation without consent is a bug.*

---

### 🧑‍⚖️ 5. Human-in-the-Loop Is Not Optional

When OsmoGrep eventually generates tests or runs them, it does not “fix” failures automatically.

Instead, it asks:

* ❌ Is the test wrong?
* ❌ Is the code wrong?
* ❌ Is the assumption wrong?
* ❌ Is setup missing?

And then it presents **choices**, not decisions.

> **OsmoGrep assists judgment — it does not replace it.**

---

### 🧪 6. Execution Is a Sandbox, Not a Gamble

Execution happens in a **dedicated agent branch**.

Your workflow remains intact:

* Original branch is preserved
* Agent branches are reused by default
* Working tree is applied *only on command*
* Rollback is always available

If something breaks, it breaks **somewhere safe**.

---

### 🛑 7. Conservative by Default, Powerful by Design

OsmoGrep is intentionally **slow to act** and **hard to misuse**.

That is not a limitation.

That is the feature.

> **“First, do no damage.”** — Engineering principle

---

### 🧠 In One Sentence

**OsmoGrep thinks like a senior engineer:
verify context, constrain scope, act deliberately, and never assume intent.**



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
