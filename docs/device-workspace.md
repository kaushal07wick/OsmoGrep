# Mac and iPhone beside your agent

Open **Settings → Workspace** for the editor and device controls. The same actions
are available through `/workspace`. These use the existing
[terminal-mac](https://github.com/govijr/terminal-mac) and
[terminal-phone](https://github.com/govijr/terminal-phone) companions.

| Action | Command |
| --- | --- |
| Code editor, following saved agent edits | `/nv [file]` |
| Download the optional Mac companion | `/workspace install mac` |
| Download the optional iPhone companion | `/workspace install phone` |
| Prepare or reuse a virtual Mac and open it | `/workspace mac [name]` |
| Prepare or reuse an iPhone simulator and open it | `/workspace phone [name]` |

Names default to `default`. Opening a missing device automatically installs its
companion if needed, downloads the required runtime, creates the device and boots
it. Existing devices are reused. The old `/workspace setup mac|phone [name]`
commands remain aliases for the same flow.
Opening a viewer leaves your chat running. Opening Neovim also keeps chat visible;
**Space a** returns focus to chat. Closing a companion viewer leaves the device
running, following the companion's lifecycle rules.

## Layout

```text
Ghostty window
┌─────────────────────────────────────────────┬─────────────────┐
│ OsmoGrep workspace                          │ Device viewer   │
│ ┌──────────────────┬──────────────────────┐ │                 │
│ │ Agent transcript │ Black Neovim editor │ │ Virtual Mac     │
│ │ and approvals    │ Follows saved edits │ │ or iPhone       │
│ │                  │ Green/red changes  │ │                 │
│ │ Paper composer   │ Space a → chat      │ │ Click and type  │
│ └──────────────────┴──────────────────────┘ │ Agent sees MCP  │
└─────────────────────────────────────────────┴─────────────────┘
```

On Ghostty, the editor and device viewers use **native panes**, each with its own
terminal surface. Chat keeps the same running process, conversation and MCP
connections when the editor opens or closes. The editor includes a compact tree,
file tabs, a live edit ticker, and an optional second code window for inspection.
Other terminals can use the private tmux editor fallback, which stacks panes in
narrow windows. terminal-mac's viewer requires a native pane outside tmux.

Automatic companion splits currently require **Ghostty 1.3+ on Apple silicon
macOS**. Other terminals receive a manual viewer command. OsmoGrep does not
change terminal settings or personal Neovim configuration.

## Find the companions

Choose **Download Mac companion** or **Download iPhone companion** in
Settings → Workspace. These install small upstream packages into OsmoGrep's
private configuration directory and save their paths automatically. Nothing is
downloaded at startup, and the main Rust binary does not contain these packages.
The download shows progress, verifies SHA-256 and macOS code signatures, and
retains bundled helpers, resources and licenses. A failed install preserves
existing configuration. No shell profile or system PATH changes are needed.

Supported download versions are terminal-mac **v0.2.0-6** (about 0.8 MB) and
terminal-phone **v0.4.0** (about 1.2 MB), from their official GitHub releases.
Mac companions require Apple silicon and macOS 14+; phone companions need macOS
15+ and Xcode for simulator setup. Downloading the companion does **not** download
macOS, iOS, Xcode, create a device, or boot it. **Open** starts device preparation
when needed. Preinstalling the small companion is optional.

OsmoGrep checks, in order:

1. `OSMOGREP_TERMINAL_MAC` or `OSMOGREP_TERMINAL_PHONE`.
2. The paths in `[workspace]` below.
3. Executables on `PATH`.
4. Checkouts in the current workspace and its parents, including
   `terminal-mac/dist/terminal-mac` and `terminal-phone/dist/terminal-phone`.

```toml
[workspace]
terminal_mac = "/absolute/path/terminal-mac/dist/terminal-mac"
terminal_phone = "/absolute/path/terminal-phone/dist/terminal-phone"
```

The companions remain optional installations. OsmoGrep releases do not bundle
their Swift applications, VM images, Xcode, or simulator runtimes. Build the cloned
companions using their documented packaging scripts, rather than copying an
unsigned raw Swift executable: the Mac package needs its virtualization
entitlement and bundled guest helper.

## First setup

Choose **Open** for Mac or iPhone in Settings → Workspace. The companion's `up`
flow handles missing runtimes, creation, reuse and boot automatically. Its pane
shows progress and any one-time setup step that needs your input. Device setup
starts only when you open a device or the agent invokes the device tool for a
task that needs it; it never runs on app startup.

- **Mac:** Apple silicon, macOS 14+, a macOS restore-image download and storage
  for the VM. Complete guest account setup and grant Screen Recording and
  Accessibility **inside the guest**. It is a separate virtual Mac, with separate
  files and login sessions. The host desktop is not controlled.
- **Phone:** Apple silicon, macOS 15+, Xcode 26+, an installed iOS runtime and the
  companion's touch-control runtime. Setup can download missing prerequisites.
  Physical iPhones additionally require pairing, signing and owner-enabled
  control; simulator setup does not silently select a personal phone.

Use the upstream guides for platform setup and device shutdown. Closing a viewer
does not stop its VM/simulator. Initial downloads can be several gigabytes.

## Ask the agent naturally

Ask for the task: “Build this iPhone app and check its sign-in screen,” for
example. The agent can call `device_workspace` with a platform (`mac` or `iphone`)
and optional name to prepare and open it. Its MCP controls become available to
that same running agent, including when the tool was discovered during the task.
Headless callers receive a background `session_id` to poll with `run_shell`, so
long setup does not block independent coding. The companion becomes an ephemeral MCP
connection for this process and repository. It does not edit global MCP settings
or enable unrelated disabled servers.

The agent discovers the companion's tools, observes the screen, acts, and checks
the result. Mac screenshots and phone observations reach vision-capable models
as actual images. Chat Completions, Responses and Anthropic Messages use their
respective image formats. Text-only models cannot inspect screenshots.

Phone calls are bound to the selected name; a conflicting phone argument is
rejected before dispatch. Opening another named phone explicitly switches that
process's phone target. Normal MCP permissions and configured OS sandbox rules
still apply. Screenshot pixels may contain sensitive information and are sent to
the selected provider when used by the model.

The device tool is hidden during greetings and unrelated tasks, can be found by
capability search, and is unavailable in read-only mode. It uses the normal
dangerous-tool approval policy. Host setup is blocked when OsmoGrep's OS sandbox
is enabled. A tool response saying `starting` is not evidence of a ready device
or a successful app test.

## Verified boundary

On the development Mac, both packaged companions built successfully. Real MCP
discovery returned eight Mac tools and seven phone tools. A dedicated Ghostty
test window kept its owner pane visible beside both companion panes, and
missing-device errors remained readable. No OS image was downloaded, no device
was booted, and no physical phone was controlled during these checks.

The automatic agent flow also passed isolated create/reuse checks for both
platforms, including setup-session polling and MCP discovery in the same task.
Official companion archives passed checksum, signature, resource and configuration
checks; the released Mac binary's setup plan includes installation and boot.
See [automatic setup validation](device-workspace-automatic-validation.json).

There was no configured Mac VM or installed iOS runtime, so **live device
observation/input is not yet verified on this machine**. See
[workspace validation](device-workspace-validation.json) and the upstream
[provenance](../third_party/README.md).
