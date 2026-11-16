# Companion provenance

OsmoGrep launches separately installed companions. It does not vendor their
Swift implementations or redistribute their binary applications in its release.
Optional install actions download checksum-verified official packages on demand:
terminal-mac v0.2.0-6 and terminal-phone v0.4.0. Their payloads and notices stay
intact, with the corresponding MIT license retained alongside each install.

| Project | Reviewed checkout | Use |
| --- | --- | --- |
| [govijr/terminal-mac](https://github.com/govijr/terminal-mac) | `74c548f27910ba2d228637780740e5fd3f9e0450` | Packaged viewer and guest-control MCP server |
| [govijr/terminal-phone](https://github.com/govijr/terminal-phone) | `63525fe83e68e156a7d1a4755c98398de27c434a` | Packaged viewer and phone-control MCP server |

Both upstream projects use the MIT license. The Ghostty Apple Event split script
at `src/workspace/ghostty-split.js` is adapted from terminal-phone's
`Sources/TerminalPhone/TerminalSplit.swift`. Its copyright notice remains in the
script and the full upstream license is retained in
[terminal-phone/LICENSE](terminal-phone/LICENSE).
The Mac companion's notice is retained in [terminal-mac/LICENSE](terminal-mac/LICENSE).

Upstream command and MCP contracts can change. The explicit companion discovery
test (`cargo test built_companions_expose_their_real_mcp_controls -- --ignored
--nocapture`) verifies the installed checkouts without booting or controlling
devices. Native-pane acceptance is a separate opt-in macOS GUI test.
