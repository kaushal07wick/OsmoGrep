![osmogrep](osmogrep.svg)

OsmoGrep is a terminal-native execution agent for validating code changes by
**running real tests** against real repositories, with strict isolation and
deterministic behavior.

It is designed to explore a simple question:

Can code changes be validated autonomously by executing the code, not by
reviewing diffs or chatting with the repository?

## What OsmoGrep Does

OsmoGrep operates locally on a Git repository and provides:

- Diff-based code inspection
- AST-backed context extraction
- AI-generated tests scoped to changes
- Deterministic test execution
- Full test suite execution with reports
- Strict branch isolation (agent branches only)
- Explicit, inspectable execution logs

## Core Capabilities

### Diff Inspector
- Parses git diffs
- Extracts changed files, symbols, and surfaces
- Displays risk and behavioral summaries
- No full-repo search

### Context Graph
- AST-based symbol extraction (Python, Rust)
- File-level and test-level context snapshots
- Context is built mechanically, not inferred

### Single-Test Execution
- Generates minimal tests for a specific change
- Runs only the generated test
- Retries with feedback on failure
- Semantic cache for passing tests

### Full Test Suite Execution
- Runs the repository’s real test suite
- Produces a structured report (logs, failures, metadata)
- No LLM retries in this mode
- Used strictly for validation

### Reports & Artifacts
- Raw test output
- Structured failure extraction
- Markdown report
- JSON index for automation

All artifacts are written to disk and reproducible.

## Execution Model

OsmoGrep never mutates your working branch.

- All automation runs in an agent branch
- The original branch is preserved
- No implicit checkouts
- No implicit commits
- No hidden file edits

Every action requires an explicit command.

## Terminal UI

- Interactive TUI
- Diff view
- Context inspection
- Execution logs
- Test progress and results
- Deterministic, replayable output

## LLM Integration

OsmoGrep supports multiple backends:

- Local models (Ollama)
- Remote APIs (OpenAI-compatible)

LLMs are used only for:
- Test generation
- Feedback-driven retries

They are not used for navigation, search, or freeform reasoning.

## Supported Languages

- Python
- Rust (partial, expanding)

## Design Constraints (Intentional)

OsmoGrep does **not** do the following:

- No freeform repo search
- No autonomous code editing
- No speculative refactors
- No chat-based agent loop

It is a validation engine, not a general coding assistant.

## Build & Run

```bash
cargo build
cargo run
