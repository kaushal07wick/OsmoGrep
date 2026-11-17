# Command-line workflows

Run `osmogrep` for a fresh chat. `osmogrep agents` opens saved sessions; `--resume ID` reopens one directly. Existing task supervision, JSONL RPC and code-intelligence commands remain available through `osmogrep --help`.

## Updates

```sh
osmogrep update --check
osmogrep update
osmogrep update --yes
```

The default asks before installation. `--check` only reports availability; `--yes` installs without prompting. Downloads use the same checksums and binary replacement as the terminal UI. Restart open instances after installation. Automated callers must explicitly choose `--yes` to install.

## Models and settings

```sh
osmogrep models
osmogrep models --provider ollama --json
osmogrep models --provider vllm --base-url http://127.0.0.1:8000/v1
osmogrep models --current
osmogrep config show
osmogrep config path
osmogrep config get model
osmogrep config set provider ollama
osmogrep config set model qwen3.5:4b
osmogrep config set base-url http://127.0.0.1:11434/v1
osmogrep config set reasoning high
```

Model discovery reuses the configured provider's catalog and credentials. `--current` reports the selected model, protocol, configured public options and context limits without a catalog request. Capability metadata is not available uniformly: configured reasoning values are not a promise that a particular model supports them.

`config` manages saved model and connection settings. Supported keys are `provider`, `model`, `base-url`, `api`, and `reasoning`. Use `base-url default` or `api auto` to restore automatic choices. Changes preserve unrelated configuration and apply to new processes; environment overrides take precedence. Provider changes clear the previous endpoint/protocol and preserve legacy credentials under their original provider. Credential values cannot be set or printed by this command. Theme, shortcuts and other workspace preferences remain in Settings.

## Team inspection

```sh
osmogrep team
osmogrep team --all --json
osmogrep team history --session SESSION_UUID
osmogrep team history --session SESSION_UUID --before MESSAGE_ID --json
```

The view is scoped to the current repository; `--repo-root PATH` selects another workspace. History returns 50 messages per page, with full bodies and receipt metadata in JSON. Reading history does not register an agent or mark messages read. To communicate, ask your coding agent naturally; these commands are inspection tools.

## Doctor

```sh
osmogrep doctor
osmogrep doctor --deep --json
osmogrep doctor --fix
```

The CLI and **Settings → Doctor** share checks for configuration readability, private storage access and permissions, saved sessions, mailbox integrity, the selected provider and the renderer's Node runtime. Git, Neovim and device companions are classified as optional integrations. Failures produce a nonzero CLI exit status; warnings and unused optional features do not.

Deep checks parse up to 2,000 saved snapshots of at most 32 MiB each, check SQLite integrity and contact the provider's model catalog. The report states incomplete coverage and lists damaged snapshots without printing their contents. It does not run model generation or download device runtimes.

`--fix` creates missing application directories and secures private Unix file permissions, then reruns checks. It preserves invalid configuration and damaged sessions/databases, and refuses linked application directories. Other problems receive suggested next steps. On Windows, file ACL repair remains manual.

The TUI shows the current check while it runs, adds completed results as they arrive, and colors OK, WARN and FAIL statuses. A completed scan includes its duration and warning/failure totals. Checks run without artificial delays.

In the TUI, **Ask agent to investigate** explicitly queues the report into the current conversation. The model can inspect evidence and make focused repairs under the normal tool and approval policy. Opening Doctor or running a scan alone never starts a model turn.

## Shell completions

```sh
osmogrep completions zsh
osmogrep completions bash
osmogrep completions fish
```

These print completion scripts generated from the current command tree. Load the script using your shell's completion setup; OsmoGrep does not edit your shell profile automatically. For the current zsh session, use `source <(osmogrep completions zsh)` after completion support is initialized.

## Transcript controls and links

Large diffs fold by visual rows. Expanded blocks retain full content and expose Show less while you read. Scrolling back pauses automatic following, including before the current viewport fills. The contextual **↓ Latest** tab resumes live output; its dot indicates activity received while reading history.

HTTP(S) links use a subtle blue tint and clickable targets. If `~/.codex/open-personal-link` exists, it handles the link; otherwise `OSMOGREP_BROWSER` can specify a browser launcher executable, with the platform browser as the fallback. URL arguments are passed directly without a shell, and executable/file URL schemes are rejected.
