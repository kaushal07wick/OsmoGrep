# OsmoGrep terminal UI

TypeScript owns presentation; Rust owns the editor buffer, keyboard events,
conversation, commands, model calls, permissions, tools, jobs and cancellation.
There is no second agent or tool runner. Display components and Unicode/Markdown
layout reuse `@earendil-works/pi-tui`.

The UI follows the supplied Claude Code reference: compact brand/model/directory
at the upper left, full-width transcript, fixed bottom composer and a small
permissions footer. Branding comes from the live **osmogrep.com** deployment:
the pixel-ring logo, lowercase wordmark and warm orange accent. The canvas stays
black in dark mode. The header mark stays still. In a fresh, empty conversation,
a dot-dithered ring holds briefly, unfolds into an origami bird with an orange
beak, folds its wings once, then reforms into the OsmoGrep ring over ten seconds. The
settled pose stays in the center. Hover has no effect, and clicks during playback
are ignored; clicking the settled mark replays it. Drafts, messages, plans and
dialogs hide it. Short terminals omit it.

Ghostty and other Kitty-compatible terminals reuse the existing inline image
path for three times the linear dot resolution (nine times as many sample
positions). Other terminals use eight Braille dot positions per cell. Both
paths generate the same geometry locally, with no model or network requests.

Button reporting is enabled automatically only while that idle artwork is visible,
then released when editing starts. `OSMOGREP_MOUSE=off osmogrep` disables this
interaction; explicit `OSMOGREP_MOUSE=on` retains normal capture for the session.
The artwork never requests hover reporting. Keyboard input, wheel scrolling and
modified clicks keep their existing handlers.

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
  soft accent selection; Enter runs the selected command. Six entries remain
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
  is active, Tab queues a follow-up; Escape cancels the run.
- Drag or paste a PNG/JPEG/GIF/WebP path to attach it. Ctrl+V reads a copied
  clipboard image on macOS. Each image inserts `[Image #N]` at the cursor.
  Removing that label excludes the image from the message.
- Graphics-capable terminals (including Ghostty and iTerm2) show a real
  thumbnail above the text. Other terminals show the attachment label. Image
  entry uses at most eight images, 20 MiB each, with bounded decoding. Native
  Rust fallback shows labels and still sends the attachments.
- The original image bytes, rather than the thumbnail, become `input_image`
  content in the existing provider request. The chosen model must support
  image input. Image messages entered during another run are queued, preserving
  their bytes. Existing per-repository session history remains local.

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
