
<p align="center">
  <img src="osmogrep.png" alt="osmogrep" width="600"/>
</p>


## What Osmogrep Is

Osmogrep is a Rust-native terminal UI for running autonomous AI agents, with structured logging, tool invocation, and streaming execution control.

* A **local-first AI agent**
* Running **inside your terminal**
* Operating on a **real Git repository**
* Using **explicit, auditable tools**

## Core Concepts

The agent acts through tools:

* **Shell** — run real commands
* **Search** — grep / ripgrep style search
* **Read File** — inspect file contents
* **Write File** — make concrete edits

## UI

Osmogrep ships with a **high-performance terminal UI**:

* Streaming agent output
* Tool calls rendered hierarchically
* Scrollable execution history
* Clear separation between:

  * user input
  * tool execution
  * agent output

## Model Support

Osmogrep works with any **OpenAI-compatible API**, including:

* OpenAI
* Anthropic-compatible endpoints
* Local models via **Ollama**

Model choice is orthogonal to execution correctness.

## Installation

### From crates.io

```bash
cargo install osmogrep
```

### Latest from GitHub

```bash
curl -fsSL https://raw.githubusercontent.com/kaushal07wick/osmogrep/master/install.sh | sh
```

## Usage

Run inside any Git repository:

```bash
osmogrep
```

Run PR/Issue triage for a GitHub repository:

```bash
osmogrep triage \
  --repo owner/repo \
  --state open \
  --limit 300 \
  --vision ./VISION.md \
  --out triage-report.json
```

`GITHUB_TOKEN` is recommended for higher API limits.

You interact with the agent directly:

* Ask it to inspect code
* Search for symbols
* Modify files
* Run commands
* Validate changes

All actions are visible and reversible via Git.

## Voice Input (vLLM Realtime + iPhone Mic)

Osmogrep can accept **live voice input** via the vLLM realtime API and stream transcriptions directly into the input box.

### Requirements

* **GPU machine** capable of running vLLM
* **vLLM** server with realtime support enabled
* **iPhone** (or any browser mic) to capture audio
* **ngrok** (or any HTTPS tunnel) for iOS mic access

### vLLM Server

Run vLLM with a realtime-capable model (example):

```bash
vllm serve mistralai/Voxtral-Mini-4B-Realtime-2602 \
  --enable-realtime \
  --host 0.0.0.0 \
  --port 8000
```

### Osmogrep Voice Proxy

Osmogrep opens a websocket proxy on `7001` and forwards to vLLM:

```bash
VLLM_REALTIME_PROXY_LISTEN=0.0.0.0:7001 \
VLLM_REALTIME_URL=ws://127.0.0.1:8000/v1/realtime \
VLLM_REALTIME_MODEL=mistralai/Voxtral-Mini-4B-Realtime-2602 \
VLLM_REALTIME_SILENCE_MS=1200 \
osmogrep
```

### Reverse Proxy (single HTTPS endpoint)

Because iOS requires HTTPS for `getUserMedia`, run the local reverse proxy:

```bash
node tools/ws_reverse_proxy.js
```

This serves `mic.html` and proxies `/v1/realtime` to `localhost:7001`.

### ngrok (HTTPS tunnel)

Run a single tunnel to the reverse proxy:

```bash
ngrok http 8080
```

Open the HTTPS URL on iPhone:

```text
https://<ngrok-url>/mic.html
```

### What You Get

* Live transcription shown above the input box
* Final sentence **inserted directly into the input box**
* Press **Enter** to send as a normal prompt

## Commands

Osmogrep supports a small, explicit set of **slash commands**.
Anything else is sent directly to the agent.

### Slash Commands

| Command  | Description                      |
| -------- | -------------------------------- |
| `/help`  | Show available commands          |
| `/clear` | Clear execution logs             |
| `/key`   | Enter OpenAI API key mode        |
| `/model` | Show/switch provider + model     |
| `/test`  | Run auto-detected project tests  |
| `/undo`  | Revert last agent file change    |
| `/diff`  | Show session file changes        |
| `/new`   | Start a fresh conversation       |
| `/approve` | Toggle dangerous tool auto-approve |
| `/quit`  | Stop the currently running agent |
| `/q`     | Alias for `/quit`                |
| `/exit`  | Exit Osmogrep                    |

During agent execution:
- `Esc` requests cancellation instead of exiting.
- Dangerous tools (`run_shell`, `write_file`, `edit_file`) prompt for approval unless `/approve` is enabled.
- `/model <provider> <model> [base_url]` switches runtime model config.
- `/test <target>` runs targeted tests (e.g. `cargo test foo`, `pytest tests/test_x.py`).
- Session state and undo checkpoints are persisted per-repo under `~/.config/osmogrep/sessions/`.

Agent toolset now also includes: `run_tests`, `list_dir`, `git_diff`, `git_log`, `regex_search`, `web_fetch`.

Hooks can be configured in `~/.config/osmogrep/config.toml`:

```toml
[hooks]
pre_shell = "echo running {cmd}"
pre_edit = "echo editing {path}"
post_edit = "cargo check -q"
```

## License
[MIT License](LICENSE).
