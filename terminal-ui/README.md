# OsmoGrep terminal UI

TypeScript owns presentation; Rust owns the editor buffer, keyboard events,
conversation, commands, model calls, permissions, tools, jobs and cancellation.
There is no second agent or tool runner. Display components and Unicode/Markdown
layout reuse `@earendil-works/pi-tui`.

The current interface has a compact brand/model/directory header, a scrollable
transcript, a fixed composer, and a configurable status line with model/provider
selectors. The deployed **osmogrep.com** pixel-ring branding is adapted to a black
canvas in dark mode. Graphite and Cotton are native pixel-paper materials behind
real editable text.

The header mark stays still. An empty conversation automatically plays one of
four central dot-dithered morphs: horizon, interference, Game of Life, or Lorenz.
The smaller artwork has explicit clearance above the composer, uses smoother
easing, and settles into a brighter fine-dot ring after ten seconds. Its orange
highlight revolves clockwise every twelve seconds at a cached eight frames per
second, without moving the ring geometry. A click brightens and
replays it; hover has no effect. Reduce Motion keeps a static pose. Drafts,
conversation and dialogs hide the artwork; short terminals omit it.

Kitty-compatible graphics provide fine separated pixel dots and paper textures.
Other terminals use Braille artwork and matte material colors. Both artwork paths
are generated locally without model calls or external assets at runtime.

Button and wheel reporting are enabled for the interactive transcript, selectors,
and artwork; hover reporting is not requested. The transcript supports wheel and
PageUp/PageDown navigation, a draggable right-gutter rail, drag-to-copy selection,
and an End shortcut to return to live output. Streaming does not pull a scrolled
viewport to the bottom. `OSMOGREP_MOUSE=off` disables app mouse interaction while
retaining keyboard navigation.

`brand/provenance.json` records the live asset hashes. `brand/logo.svg` preserves
the deployed mark; `src/site-art.ts` holds its pixel grid.
To refresh them after inspecting a new website deployment, download its homepage
and run `node scripts/import-site-brand.mjs homepage.html`.
The importer is a development tool; running OsmoGrep never contacts the website.

`osmogrep` selects this renderer when Node.js **22.19+** is available. The
JavaScript bundle is embedded in the Rust binary, extracted into a private
temporary directory and removed on exit. Users don't run npm install. Missing
Node, a renderer crash, a timeout or invalid geometry restores the existing
Rust renderer with a visible warning. Use `OSMOGREP_UI=native osmogrep` to select
that fallback explicitly. Noninteractive commands don't start Node.

## Composer

- Typing `/` opens a compact command card above the composer. Up/down moves the
  soft accent selection; Enter runs the selected command. Up to five entries remain
  visible at a time, with a position counter and keyboard hints when space allows.
- A continuous charcoal panel provides blank rows above and below the draft,
  horizontal padding and a gap before the footer. Dark-mode surroundings stay black.
- Type or paste multiline text; the composer expands to eight visible lines,
  then scrolls to the cursor. The buffer has no imposed text length limit.
- Left/right, up/down and Home/End edit within the draft. Ctrl+Home/End moves
  to the start/end. Alt+Left/Right moves by word. Shift+arrows selects text.
- Ctrl+A selects all. Ctrl+C copies selected text, or the whole draft when no
  range is selected. Ctrl+X cuts; Ctrl+V pastes from the Mac clipboard. Normal
  terminal Cmd+C/Cmd+V shortcuts also retain terminal-managed behavior.
- Ctrl+K kills to line end, Ctrl+W deletes the preceding word, Ctrl+Y yanks the
  internal clipboard. Shift+Enter inserts a newline; Enter sends. While a run
  is active, Enter or Tab queues a separate follow-up; Escape cancels the run and
  pauses the queue. An empty Enter resumes a paused queue.
- Drag or paste a PNG/JPEG/GIF/WebP path to attach it. Ctrl+V reads a copied
  clipboard image on macOS. Each image inserts `[Image #N]` at the cursor.
  Backspace/Delete removes the whole image label, including from inside it.
  Partial selections expand to the whole attachment, and removing its label
  excludes the image from the message.
- Graphics-capable terminals (including Ghostty and iTerm2) show a real
  thumbnail above the text. Other terminals show the attachment label. Image
  entry uses at most eight images, 20 MiB each, with bounded decoding. Native
  Rust fallback shows labels and still sends the attachments.
- The original image bytes, rather than the thumbnail, become `input_image`
  content in the existing provider request. The chosen model must support
  image input. Image messages entered during another run are queued, preserving
  their bytes. Existing per-repository session history remains local.

Applied diffs are anchored to their edit result before the final assistant reply.
Tool output can be folded/expanded with Ctrl+T; approvals summarize the target path
rather than dumping patch payloads. The agent prompt requests a short change
summary for ordinary edits. Escape never exits an idle app; `/exit` does.

## README previews

The root README uses PNG/GIF exports of this renderer with illustrative data,
not screenshots of a private repository. Regenerate them with
`python3 terminal-ui/scripts/readme-previews.py` from the repo root. This optional
development script needs Pillow plus Menlo or DejaVu Sans Mono; those are not
application runtime dependencies. The chart preview is explicitly sample data.

## Development

Run `npm ci`, `npm test` and `npm run build` here. Commit
`bundle/renderer.mjs` with its source so Rust builds remain self-contained.
CI checks that regenerating the bundle leaves no diff.

The private local IPC protocol sends one display snapshot per requested frame
and returns styled rows, cursor coordinates, input/transcript bounds and image
placements. Keyboard data never goes to the Node stdin stream. The parent
validates replies and owns synchronized terminal updates and cleanup. The
renderer receives no provider credentials, and strips terminal control sequences
from repository text before styling it.
