<p align="center">
  <a href="https://osmogrep.com">
    <img src="docs/assets/readme-brand.svg" alt="OsmoGrep — terminal logo and wordmark" width="432" />
  </a>
</p>

<p align="center"><strong>Catch the bug. Ship the fix.</strong></p>
<p align="center">Build features, understand a codebase, and fix bugs from your terminal.</p>

<p align="center">
  <a href="https://github.com/kaushal07wick/OsmoGrep/releases/latest"><img src="https://img.shields.io/github/v/release/kaushal07wick/OsmoGrep?style=flat-square&amp;logo=github&amp;label=release&amp;labelColor=161b22&amp;color=ea580c" alt="Latest release" /></a>
  <a href="https://github.com/kaushal07wick/OsmoGrep/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/kaushal07wick/OsmoGrep/ci.yml?branch=master&amp;style=flat-square&amp;logo=github&amp;label=CI&amp;labelColor=161b22" alt="CI status on master" /></a>
  <a href="https://github.com/kaushal07wick/OsmoGrep/releases"><img src="https://img.shields.io/github/downloads/kaushal07wick/OsmoGrep/total?style=flat-square&amp;labelColor=161b22&amp;color=ea580c" alt="Release downloads" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/kaushal07wick/OsmoGrep?style=flat-square&amp;labelColor=161b22&amp;color=ea580c" alt="MIT license" /></a>
  <a href="https://github.com/kaushal07wick/OsmoGrep/stargazers"><img src="https://img.shields.io/github/stars/kaushal07wick/OsmoGrep?style=flat-square&amp;logo=github&amp;labelColor=161b22&amp;color=ea580c" alt="GitHub stars" /></a>
  <a href="https://github.com/kaushal07wick/OsmoGrep/graphs/contributors"><img src="https://img.shields.io/github/contributors/kaushal07wick/OsmoGrep?style=flat-square&amp;labelColor=161b22&amp;color=ea580c" alt="Contributors" /></a>
</p>

<p align="center">
  <a href="https://osmogrep.com">Website</a> ·
  <a href="#get-started">Get started</a> ·
  <a href="#models">Models</a> ·
  <a href="#documentation">Documentation</a>
</p>

![OsmoGrep: a code change, an inline diff, and a short final response](docs/assets/terminal-workspace.png)

## Get started

Install the current source:

```sh
cargo install --git https://github.com/kaushal07wick/OsmoGrep --locked
```

Open a project and start a conversation:

```sh
cd your-project
export OPENAI_API_KEY="your-api-key"
osmogrep
```

Requires Rust 1.89+ to install and **Node.js 22.19+** for the full terminal UI. You can enter your key with `/key` instead, or connect another provider below. [Installation options and platform notes →](docs/reference.md#quickstart)

New GitHub releases appear in the terminal with a short changelog and **Update now**, **Later**, or **Skip this version**. Nothing installs without your choice.

## From a question to a code change

```text
Explain how request cancellation works in this repo.
Add retry support for HTTP 429 responses.
Review these changes before I commit them.
```

OsmoGrep can search your code, follow symbols, edit files, apply patches, run commands, and check the result. Tool activity stays visible. Changes have numbered diffs. Commands and snippets have highlighted code blocks with click-to-copy. The final reply tells you what changed.

| Work | What OsmoGrep brings |
| --- | --- |
| Understand a codebase | File search, definitions, references, and repository context |
| Build and debug | Real edits, shell commands, focused tests, and diagnostics |
| Review the change | Inline diffs, expandable output, and conflict-aware turn checkpoints |
| Keep the conversation moving | Multiline drafts, image attachments, and queued follow-ups |
| Set your boundaries | Read-only mode, scoped approvals, optional OS sandbox, and explicit cancellation |
| Bring your workflow | Project instructions, skills, configurable shortcuts, and Neovim integration |

## A terminal worth working in

A black canvas, a quiet pixel logo, and a paper-textured composer. **Graphite** for dark mode. **Cotton** for light mode.

Paste a screenshot, edit a long prompt, or queue the next request while a run continues. Search commands with `/`, choose a model from the status line, and scroll or copy the transcript without losing your draft.

<table>
  <tr>
    <td width="50%"><img src="docs/assets/terminal-commands.png" alt="Graphite composer with an attached command tray" /></td>
    <td width="50%"><img src="docs/assets/terminal-cotton.png" alt="Cotton light theme with an inline code diff" /></td>
  </tr>
  <tr>
    <td align="center"><strong>Commands within reach.</strong> Search and act from the composer.</td>
    <td align="center"><strong>A lighter workspace.</strong> The same tools in Cotton.</td>
  </tr>
</table>

Make it yours with `/settings`, `/keymap`, and `/statusline`. On Kitty-compatible terminals, native graphics add fine paper grain and dot-dithered animation. Other terminals use a text fallback.

<details>
<summary><strong>See the motion and usage views</strong></summary>

The central logo morphs and settles into the background. Click to replay; Reduce Motion keeps it still.

<img src="docs/assets/terminal-morph.gif" alt="OsmoGrep pixel-ring morph, shown during click replay" width="680" />

`/usage` summarizes input, output, cache reuse and context at a glance. Weekly usage puts this week, last week, and input/output totals on one readable scale. `/stats` switches between recorded activity and an explicitly labeled sample view.

![Weekly token usage with sample data clearly labeled](docs/assets/terminal-usage.png)

</details>

*Terminal previews use the current renderer with illustrative workspace data.*

## Models

Use **OpenAI, Anthropic, OpenRouter, Groq, Mistral, DeepSeek, or xAI**. Run local models through **Ollama or LM Studio**. Bring a custom gateway when your setup needs one.

Select your provider and model from the footer or `/model`. OsmoGrep supports Responses, Chat Completions, and native Anthropic Messages, with separately stored provider credentials, reasoning controls, and configurable model limits. New saved keys use your OS credential store; `/key migrate` moves legacy plaintext keys.

[Connect a provider →](docs/reference.md#providers) · [Custom endpoints and API support →](docs/agent-provider-design.md#configuration)

## Your workspace, your permissions

Start read-only, approve actions as they come, or enable full access through `/permissions`. Review the diff and use `/undo` for the last captured file edit. **Escape interrupts the current run; `/exit` closes the app.**

Resume or fork independent conversations with `/sessions`, `/resume`, and `/fork`. Model-aware compaction archives original context; `/recover` opens an interrupted run without replaying unfinished tools. Interactive commands accept stdin and can keep running while the agent works. Subagents run only when you explicitly request delegation; writable workers use isolated worktrees.

Press **Shift+Left** to manage sessions across projects: search, filter, pin, preview, resume, and fork. Ask naturally to talk to another agent in the same repository. Instances discover one another by session ID and task metadata, exchange durable messages and progress, and use file claims to reduce conflicting edits. [Sessions and teams →](docs/team-runtime.md)

For sustained work, `osmogrep task start` adds a durable objective, acceptance criteria, progress checkpoints, and explicit time/request/token budgets. Interrupted tasks can resume without replaying uncertain tools. [Supervised long runs →](docs/long-runs.md)

Sessions and preferences stay on your machine. Prompts, attachments, and tool context go to the model provider you select. [Permissions and data handling →](docs/reference.md#permissions-and-data)

## Documentation

| Guide | What's inside |
| --- | --- |
| [User reference](docs/reference.md) | Installation, providers, controls, configuration, and headless use |
| [Terminal guide](terminal-ui/README.md) | Editing, attachments, graphics, and UI development |
| [Workspace controls](docs/terminal-workspace.md) | Skills, shortcuts, permissions, and status-line settings |
| [Supervised long runs](docs/long-runs.md) | Durable objectives, completion evidence, budgets, stop, and recovery |
| [Sessions and teams](docs/team-runtime.md) | Session center, cross-process messaging, activity, and coordinated edits |
| [Harness foundations](docs/harness-foundations.md) | Sessions, checkpoints, process sessions, MCP/LSP, credentials, and RPC |
| [Provider design](docs/agent-provider-design.md) | API support, conversation handling, and integration limits |
| [Regression intelligence](docs/prre.md) | Optional revision-aware indexing and analysis |
| [Contributing and development](docs/reference.md#development) | Build, test, renderer bundle, and repository structure |

Use OsmoGrep in scripts with `osmogrep run --prompt "..." --json-events`. See the [headless guide](docs/reference.md#commands-and-headless-use) for permission flags and events.

---

[MIT](LICENSE) · Built by [Kaushal Choudhary](https://github.com/kaushal07wick)
