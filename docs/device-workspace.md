# Mac and iPhone beside your agent

Open **Settings → Workspace** for the editor and device controls. The same actions
are available through `/workspace`. These use the existing
[terminal-mac](https://github.com/govijr/terminal-mac) and
[terminal-phone](https://github.com/govijr/terminal-phone) companions.

| Action | Command |
| --- | --- |
| Code editor, following saved agent edits | `/nv [file]` |
| View an existing virtual Mac | `/workspace mac [name]` |
| View an existing phone | `/workspace phone [name]` |
| Set up a virtual Mac | `/workspace setup mac [name]` |
| Set up a simulator | `/workspace setup phone [name]` |

Names default to `default`. Use the same name when setting up and viewing a device.
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

The editor uses a private tmux workspace. Device viewers use **native Ghostty
panes**, outside tmux: terminal-mac does not support drawing through tmux. This
also keeps device graphics out of the agent's transcript. Allow enough window
width for three panes; narrow editor/agent workspaces stack vertically.

Automatic companion splits currently require **Ghostty 1.3+ on Apple silicon
macOS**. Other terminals receive a manual viewer command. OsmoGrep does not
change terminal settings or personal Neovim configuration.

## Find the companions

OsmoGrep checks, in order:

1. `OSMOGREP_TERMINAL_MAC` or `OSMOGREP_TERMINAL_PHONE`.
2. The paths in `[workspace]` below.
3. Executables on `PATH`.
4. Sibling checkouts: `../terminal-mac/dist/terminal-mac` and
   `../terminal-phone/dist/terminal-phone`, relative to the project root.

```toml
[workspace]
terminal_mac = "/absolute/path/terminal-mac/dist/terminal-mac"
terminal_phone = "/absolute/path/terminal-phone/dist/terminal-phone"
```

The companions remain separate installations. OsmoGrep releases do not bundle
their Swift applications, VM images, Xcode, or simulator runtimes. Build the cloned
companions using their documented packaging scripts, rather than copying an
unsigned raw Swift executable: the Mac package needs its virtualization
entitlement and bundled guest helper.

## First setup

Opening **View** never creates a missing device. Its pane shows the setup error
and stays open so you can read it. Choose **Setup** explicitly when ready.

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

After opening a viewer, ask for the task: “Open the app on this phone and check
the sign-in screen,” for example. The companion becomes an ephemeral MCP
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

## Verified boundary

On the development Mac, both packaged companions built successfully. Real MCP
discovery returned eight Mac tools and seven phone tools. A dedicated Ghostty
test window kept its owner pane visible beside both companion panes, and
missing-device errors remained readable. No OS image was downloaded, no device
was booted, and no physical phone was controlled during these checks.

There was no configured Mac VM or installed iOS runtime, so **live device
observation/input is not yet verified on this machine**. See
[workspace validation](device-workspace-validation.json) and the upstream
[provenance](../third_party/README.md).
