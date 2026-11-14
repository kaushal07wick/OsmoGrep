//! Optional presentation process. The Rust runtime retains all input and agent execution.
use std::{
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    time::Duration,
};

use crossterm::{
    cursor::MoveTo,
    event::{DisableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind},
    execute, queue,
    style::Print,
    terminal::{Clear, ClearType},
};
use ratatui::layout::Rect;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    agent::ModelConfig,
    state::{AgentState, InputMode},
};

const BUNDLE: &str = include_str!("../../terminal-ui/bundle/renderer.mjs");
const FRAME_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Deserialize)]
struct Region {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}
impl Region {
    fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
    fn inside(&self, w: u16, h: u16) -> bool {
        self.x as u32 + self.width as u32 <= w as u32
            && self.y as u32 + self.height as u32 <= h as u32
    }
}
#[derive(Deserialize)]
struct Point {
    x: u16,
    y: u16,
}
#[derive(Deserialize, Clone)]
struct Preview {
    id: u32,
    x: u16,
    y: u16,
    sequence: String,
    #[serde(default)]
    placement: Option<String>,
}
impl Preview {
    fn retained_in(&self, images: &[Self]) -> bool {
        self.placement.is_some()
            && images.iter().any(|image| {
                image.placement.is_some() && image.id == self.id && image.sequence == self.sequence
            })
    }
}
#[derive(Deserialize)]
struct Control {
    id: String,
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    label: Option<String>,
    #[serde(flatten)]
    area: Region,
}
#[derive(Deserialize)]
struct Transcript {
    #[serde(flatten)]
    area: Region,
    start: usize,
    #[serde(rename = "maxScroll")]
    max_scroll: usize,
    #[serde(rename = "lineWidths")]
    line_widths: Vec<usize>,
}
impl Transcript {
    fn point(&self, x: u16, y: u16) -> Option<[usize; 2]> {
        if self.line_widths.is_empty() {
            return None;
        }
        let row = (y.saturating_sub(self.area.y) as usize).min(self.line_widths.len() - 1);
        Some([
            self.start + row,
            (x.saturating_sub(self.area.x) as usize).min(self.line_widths[row]),
        ])
    }
}
#[derive(Deserialize)]
struct Frame {
    id: u64,
    rows: Vec<String>,
    input: Region,
    execution: Region,
    #[serde(rename = "editorWidth")]
    editor_width: u16,
    animate: bool,
    artwork: Option<Region>,
    cursor: Point,
    #[serde(default, rename = "cursorVisible")]
    cursor_visible: Option<bool>,
    images: Vec<Preview>,
    #[serde(default)]
    controls: Vec<Control>,
    #[serde(default)]
    transcript: Option<Transcript>,
    #[serde(default, rename = "selectionText")]
    selection_text: String,
}

struct IntroInteraction {
    area: Option<Rect>,
    started: Option<std::time::Instant>,
    playing: bool,
    capture: bool,
    variant: usize,
    bright: bool,
}
impl Default for IntroInteraction {
    fn default() -> Self {
        Self {
            area: None,
            started: None,
            playing: false,
            capture: false,
            variant: (uuid::Uuid::new_v4().as_bytes()[0] % 4) as usize,
            bright: false,
        }
    }
}
impl IntroInteraction {
    fn handle_event(&mut self, event: &Event) -> bool {
        let Event::Mouse(mouse) = event else {
            return false;
        };
        let Some(area) = self.area else { return false };
        if !mouse.modifiers.is_empty() || mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return false;
        }
        if !area.contains((mouse.column, mouse.row).into()) {
            return false;
        }
        if self.playing {
            self.bright = true;
            return true;
        }
        self.started = Some(std::time::Instant::now());
        self.playing = true;
        self.bright = true;
        true
    }
}

pub struct TypeScriptUi {
    child: Child,
    input: SyncSender<Value>,
    frames: Receiver<Result<String, String>>,
    directory: PathBuf,
    sequence: u64,
    previous: Vec<String>,
    images: Vec<Preview>,
    controls: Vec<Control>,
    size: (u16, u16),
    intro: IntroInteraction,
    transcript: Option<Transcript>,
    selecting: bool,
    copy_pending: bool,
}

impl TypeScriptUi {
    pub fn start() -> Result<Self, String> {
        let node = Command::new("node")
            .arg("--version")
            .output()
            .map_err(|_| "Node.js 22.19+ unavailable")?;
        let version = String::from_utf8_lossy(&node.stdout);
        let mut parts = version.trim().trim_start_matches('v').split('.');
        let major: u32 = parts.next().unwrap_or("").parse().unwrap_or(0);
        let minor: u32 = parts.next().unwrap_or("").parse().unwrap_or(0);
        if !node.status.success() || major < 22 || major == 22 && minor < 19 {
            return Err("TypeScript UI requires Node.js 22.19+".into());
        }
        let directory = std::env::temp_dir().join(format!("osmogrep-ui-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let result = Self::spawn(directory.clone());
        if result.is_err() {
            let _ = fs::remove_dir_all(directory);
        }
        result
    }

    fn spawn(directory: PathBuf) -> Result<Self, String> {
        let script = directory.join("renderer.mjs");
        fs::write(&script, BUNDLE).map_err(|e| e.to_string())?;
        let mut command = Command::new("node");
        command
            .arg(script)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for key in [
            "PATH",
            "TERM",
            "TERM_PROGRAM",
            "COLORTERM",
            "LANG",
            "NO_COLOR",
            "KITTY_WINDOW_ID",
            "GHOSTTY_RESOURCES_DIR",
            "TMUX",
            "ITERM_SESSION_ID",
            "WEZTERM_PANE",
            "ORCA_IMAGE_PROTOCOL",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let mut input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, frames) = mpsc::sync_channel(1);
        let (requests, pending) = mpsc::sync_channel::<Value>(1);
        let errors = tx.clone();
        std::thread::spawn(move || {
            while let Ok(value) = pending.recv() {
                let result = serde_json::to_writer(&mut input, &value)
                    .map_err(io::Error::other)
                    .and_then(|_| input.write_all(b"\n"))
                    .and_then(|_| input.flush());
                if let Err(error) = result {
                    let _ = errors.send(Err(error.to_string()));
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                let result = reader.by_ref().take(16 * 1024 * 1024).read_line(&mut line);
                match result {
                    Ok(0) => break,
                    Ok(_) if line.ends_with('\n') => {
                        if tx.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    _ => {
                        let _ = tx.send(Err("Invalid TypeScript UI response".into()));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            input: requests,
            frames,
            directory,
            sequence: 0,
            previous: Vec::new(),
            images: Vec::new(),
            controls: Vec::new(),
            size: (0, 0),
            intro: IntroInteraction::default(),
            transcript: None,
            selecting: false,
            copy_pending: false,
        })
    }

    pub fn handle_visual_event(&mut self, event: &Event, state: &mut AgentState) -> bool {
        if matches!(event,Event::Mouse(m) if m.kind==MouseEventKind::Down(MouseButton::Left)) {
            self.selecting = false;
            self.copy_pending = false;
        }
        if state.ui.pending_permission.is_none() && state.ui.pending_update.is_none() {
            if let Event::Mouse(mouse) = event {
                if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && mouse.modifiers.is_empty()
                {
                    if let Some(control) = self.controls.iter().find(|c| {
                        c.id == "choice" && c.area.rect().contains((mouse.column, mouse.row).into())
                    }) {
                        if let Some(index) = control.index {
                            return super::workbench::choose_at(
                                state,
                                index,
                                control.label.as_deref().unwrap_or(""),
                            );
                        }
                    }
                }
            }
        }
        if state.ui.panel.is_none()
            && state.ui.pending_permission.is_none()
            && state.ui.pending_update.is_none()
            && state.ui.input_mode != InputMode::ApiKey
        {
            if let Event::Mouse(mouse) = event {
                if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && mouse.modifiers.is_empty()
                {
                    if let Some(control) = self.controls.iter().find(|control| {
                        control
                            .area
                            .rect()
                            .contains((mouse.column, mouse.row).into())
                    }) {
                        super::workbench::open_selector(state, control.id == "provider");
                        return true;
                    }
                }
            }
        }
        if let Event::Key(key) = event {
            if state.ui.transcript_selection.is_some()
                && key.code == KeyCode::Char('c')
                && key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER)
            {
                self.copy_pending = true;
                return true;
            }
            if matches!(
                key.code,
                KeyCode::Char(_) | KeyCode::Enter | KeyCode::Backspace
            ) && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
            {
                state.ui.transcript_selection = None;
            }
        }
        if state.ui.panel.is_none() {
            if let Event::Mouse(mouse) = event {
                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) if mouse.modifiers.is_empty() => {
                        state.ui.transcript_selection = None;
                        if let Some(view) = &self.transcript {
                            if view.area.rect().contains((mouse.column, mouse.row).into()) {
                                if let Some(point) = view.point(mouse.column, mouse.row) {
                                    state.ui.transcript_selection = Some([point, point]);
                                    self.selecting = true;
                                    state.ui.input_all_selected = false;
                                    state.ui.input_selection_anchor = None;
                                    return true;
                                }
                            }
                        }
                    }
                    MouseEventKind::Drag(MouseButton::Left)
                    | MouseEventKind::Up(MouseButton::Left)
                        if self.selecting =>
                    {
                        if let (Some(view), Some(range)) =
                            (&self.transcript, state.ui.transcript_selection.as_mut())
                        {
                            if let Some(point) = view.point(mouse.column, mouse.row) {
                                range[1] = point;
                            }
                        }
                        if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
                            self.selecting = false;
                            self.copy_pending = true;
                        }
                        return true;
                    }
                    _ => {}
                }
            }
        }
        self.intro.handle_event(event)
    }

    pub fn draw(
        &mut self,
        state: &mut AgentState,
        model: &ModelConfig,
        width: u16,
        height: u16,
    ) -> Result<(Rect, Rect, usize, bool), String> {
        self.sequence += 1;
        let mut snapshot = view(state, model, self.sequence, width, height);
        snapshot["introElapsed"] = json!(self
            .intro
            .started
            .get_or_insert_with(std::time::Instant::now)
            .elapsed()
            .as_millis() as u64);
        snapshot["introScene"] =
            json!(["horizon", "interference", "life", "lorenz"][self.intro.variant]);
        snapshot["introBright"] = json!(self.intro.bright);
        self.input.try_send(snapshot).map_err(|e| e.to_string())?;
        let raw = self
            .frames
            .recv_timeout(FRAME_TIMEOUT)
            .map_err(|_| "TypeScript UI stopped responding")??;
        let frame: Frame = serde_json::from_str(&raw).map_err(|_| "Invalid TypeScript UI frame")?;
        if frame.id != self.sequence
            || frame.rows.len() != height as usize
            || !frame.input.inside(width, height)
            || !frame.execution.inside(width, height)
            || frame.transcript.as_ref().is_some_and(|t| {
                !t.area.inside(width, height)
                    || t.start > t.max_scroll
                    || t.line_widths.len() > t.area.height as usize
                    || t.line_widths.iter().any(|w| *w > t.area.width as usize)
            })
            || frame.controls.iter().any(|c| {
                !matches!(c.id.as_str(), "model" | "provider" | "choice")
                    || c.id == "choice"
                        && (!c.index.is_some_and(|i| i <= 1000)
                            || c.label.as_ref().is_none_or(|s| s.len() > 160))
                    || c.area.width == 0
                    || c.area.height != 1
                    || !c.area.inside(width, height)
            })
            || frame.cursor.x >= width
            || frame.cursor.y >= height
            || frame.editor_width == 0
            || frame.editor_width > frame.input.width
            || frame.rows.iter().any(|s| s.contains(['\n', '\r']))
            || frame.artwork.as_ref().is_some_and(|area| {
                !area.inside(width, height)
                    || area.width == 0
                    || area.height == 0
                    || area.rect().intersection(frame.execution.rect()) != area.rect()
            })
        {
            return Err("TypeScript UI frame outside terminal bounds".into());
        }
        let mut output = io::stdout().lock();
        output
            .write_all(b"\x1b[?2026h\x1b[?25l")
            .map_err(|e| e.to_string())?;
        self.intro.area = frame.artwork.as_ref().map(Region::rect);
        self.intro.playing = self.intro.area.is_some() && frame.animate;
        let capture =
            (frame.execution.height > 0 || self.intro.area.is_some() || !frame.controls.is_empty())
                && super::terminal::artwork_mouse_enabled()
                && !super::terminal::mouse_capture_enabled();
        if capture != self.intro.capture {
            if capture {
                // Drag events support transcript selection; hover never affects artwork.
                queue!(output, Print("\x1b[?1002h\x1b[?1006h"))
            } else {
                queue!(output, DisableMouseCapture)
            }
            .map_err(|e| e.to_string())?;
            self.intro.capture = capture;
        }
        for old in &self.images {
            let retained = old.retained_in(&frame.images);
            let delete = if retained { "i" } else { "I" };
            write!(output, "\x1b_Ga=d,d={delete},i={},q=2;\x1b\\", old.id)
                .map_err(|e| e.to_string())?;
        }
        let resized = self.size != (width, height);
        if resized {
            queue!(output, Clear(ClearType::All)).map_err(|e| e.to_string())?;
        }
        for (y, row) in frame.rows.iter().enumerate() {
            if resized || self.previous.get(y) != Some(row) {
                queue!(
                    output,
                    MoveTo(0, y as u16),
                    Print("\x1b[0m"),
                    Clear(ClearType::UntilNewLine),
                    Print(row),
                    Print("\x1b[0m")
                )
                .map_err(|e| e.to_string())?;
            }
        }
        for image in &frame.images {
            if image.x < width && image.y < height {
                queue!(output, MoveTo(image.x, image.y)).map_err(|e| e.to_string())?;
                let retained = image.retained_in(&self.images);
                if !retained {
                    queue!(output, Print(&image.sequence)).map_err(|e| e.to_string())?;
                }
                if let Some(placement) = &image.placement {
                    queue!(output, Print(placement)).map_err(|e| e.to_string())?;
                }
            }
        }
        self.images = frame.images;
        self.controls = frame.controls;
        if let Some(view) = &frame.transcript {
            state.ui.exec_scroll_max = view.max_scroll;
            if !state.ui.follow_tail {
                state.ui.exec_scroll = view.max_scroll.saturating_sub(view.start);
            }
        }
        self.transcript = frame.transcript;
        queue!(
            output,
            MoveTo(frame.cursor.x, frame.cursor.y),
            Print(if frame.cursor_visible.unwrap_or(true) {
                "\x1b[0m\x1b[?25h\x1b[?2026l"
            } else {
                "\x1b[0m\x1b[?2026l"
            })
        )
        .map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
        if self.copy_pending {
            self.copy_pending = false;
            if state.ui.transcript_selection.is_some() && !frame.selection_text.is_empty() {
                state.ui.input_clipboard = frame.selection_text.clone();
                if let Err(error) = crate::clipboard::copy_text_to_clipboard(&frame.selection_text)
                {
                    crate::logger::log_status(state, format!("Copy failed: {error}"));
                }
            }
        }
        self.previous = frame.rows;
        self.size = (width, height);
        Ok((
            frame.input.rect(),
            frame.execution.rect(),
            frame.editor_width as usize,
            frame.animate,
        ))
    }
}

impl Drop for TypeScriptUi {
    fn drop(&mut self) {
        for image in &self.images {
            let _ = write!(io::stdout(), "\x1b_Ga=d,d=I,i={},q=2;\x1b\\", image.id);
        }
        let _ = io::stdout().flush();
        if self.intro.capture {
            let _ = execute!(io::stdout(), DisableMouseCapture);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn view(state: &AgentState, model: &ModelConfig, id: u64, width: u16, height: u16) -> Value {
    let ui = &state.ui;
    let cursor = ui
        .input
        .char_indices()
        .take_while(|(i, _)| *i < ui.input_cursor.min(ui.input.len()))
        .count();
    let input = if ui.input_mode == InputMode::ApiKey {
        "•".repeat(ui.input.chars().count())
    } else {
        ui.input.clone()
    };
    let selection = state.input_selection().map(|r| {
        (
            ui.input[..r.start].chars().count(),
            ui.input[..r.end].chars().count(),
        )
    });
    let cwd = state
        .repo_root
        .strip_prefix(dirs::home_dir().unwrap_or_default())
        .map(|p| format!("~/{}", p.display()))
        .unwrap_or_else(|_| state.repo_root.display().to_string());
    let diffs: Vec<_> = if ui.diff_active {
        ui.diff_snapshot.iter().map(|d| {
        let diff = super::diff::Diff::from_texts(d.target.clone(), &d.before, &d.after);
        json!({ "file":diff.file,"added":diff.added,"removed":diff.removed,"preview":d.tool.starts_with("preview:"), "lines":diff.lines.iter().map(|l| json!({"text":l.text,"kind":format!("{:?}",l.kind),"oldLine":l.old_lineno,"newLine":l.new_lineno})).collect::<Vec<_>>() })
    }).collect()
    } else {
        Vec::new()
    };
    let mut snapshot = json!({
        "id":id, "width":width, "height":height, "version":env!("CARGO_PKG_VERSION"),
        "cwd":cwd, "model":model.model, "provider":model.provider,
        "branch":ui.repo_branch.as_deref().unwrap_or(""), "indexed":ui.indexed, "indexing":ui.indexing,
        "theme":state.theme.as_str(), "accent":state.accent.as_str(), "density":state.density.as_str(), "session":state.session_name,
        "elapsed":state.started_at.elapsed().as_millis() as u64,
        "profile":state.permission_profile.as_str(), "autoApprove":ui.auto_approve, "planMode":state.plan_mode,
        "logs":state.logs.iter().map(|l|json!({"text":l.text,"level":format!("{:?}",l.level)})).collect::<Vec<_>>(),
        "plan":state.plan_items, "diffs":diffs, "input":input, "cursor":cursor, "selection":selection,
        "masked":ui.input_mode == InputMode::ApiKey, "inputMode":format!("{:?}",ui.input_mode),
        "attachments":ui.attachments.iter().filter(|a|ui.input.contains(&a.label)).map(|a|json!({"id":a.id,"label":a.label,"thumbnail":a.thumbnail,"width":a.width,"height":a.height})).collect::<Vec<_>>(),
        "running":ui.agent_running, "phase":ui.run_phase, "tool":ui.current_tool,
        "followTail":ui.follow_tail, "scroll":ui.exec_scroll.min(u32::MAX as usize),
        "stream":if ui.streaming_active {ui.streaming_buffer.rsplit('\n').next().unwrap_or("")} else {""},
        "permission":ui.pending_permission.as_ref().map(|p|format!("Allow {} ({})? [y]es [n]o [a]lways", p.tool_name,p.args_summary)),
        "update":ui.pending_update.as_ref().map(super::tui::pending_update_prompt),
        "voice":if state.voice.visible || state.voice.enabled || state.voice.connected {Some(super::tui::voice_bar_text(state))} else {None},
        "commands":ui.command_items.iter().map(|c|json!({"cmd":c.cmd,"desc":c.desc})).collect::<Vec<_>>(),
        "selectedCommand":ui.command_selected, "tokens":state.usage.prompt_tokens+state.usage.completion_tokens,
    });
    snapshot["queuedMessages"] = json!(ui.queued_agent_prompts.len());
    snapshot["queuePaused"] = json!(ui.queue_paused);
    snapshot["transcriptSelection"] = json!(ui.transcript_selection);
    if !ui.follow_tail && ui.exec_scroll_max != usize::MAX {
        snapshot["scrollTop"] = json!(ui.exec_scroll_max.saturating_sub(ui.exec_scroll));
    }
    snapshot["panel"] = serde_json::to_value(super::workbench::view(state)).unwrap_or(Value::Null);
    snapshot["shortcuts"] = json!(super::keymap::BINDINGS
        .iter()
        .map(|b| (b.id, super::keymap::effective(&state.preferences, b)))
        .collect::<std::collections::BTreeMap<_, _>>());
    snapshot["approval"] = json!(ui.pending_permission.as_ref().map(
        |p| json!({"tool":p.tool_name,"args":p.args_summary,"selected":ui.permission_selected})
    ));
    snapshot["statusEntries"] = json!(super::workbench::status_entries(state)
        .into_iter()
        .map(|(id, value)| json!({"id":id,"value":value}))
        .collect::<Vec<_>>());
    snapshot["runElapsed"] = json!(ui
        .run_started_at
        .map(|t| t.elapsed().as_millis() as u64)
        .unwrap_or(0));
    snapshot["expandTools"] = json!(ui.expand_tools);
    snapshot["statusLine"] = json!(super::workbench::statusline(state));
    snapshot["preferences"] = serde_json::to_value(&state.preferences).unwrap_or(Value::Null);
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};

    #[test]
    fn intro_ignores_hover_and_only_replays_a_settled_mark_on_click() {
        let mut intro = IntroInteraction {
            area: Some(Rect::new(20, 10, 60, 28)),
            ..IntroInteraction::default()
        };
        let mut mouse = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 50,
            row: 20,
            modifiers: KeyModifiers::NONE,
        };
        assert!(!intro.handle_event(&Event::Mouse(mouse)));
        assert!(intro.started.is_none());
        mouse.kind = MouseEventKind::Down(MouseButton::Left);
        assert!(intro.handle_event(&Event::Mouse(mouse)));
        let clicked = intro.started;
        assert!(clicked.is_some());
        assert!(intro.handle_event(&Event::Mouse(mouse)));
        assert_eq!(intro.started, clicked, "clicks must not interrupt playback");
        mouse.column = 2;
        assert!(!intro.handle_event(&Event::Mouse(mouse)));
        assert_eq!(intro.started, clicked);
        intro.started = Some(std::time::Instant::now() - Duration::from_secs(11));
        intro.playing = false;
        mouse.column = 50;
        mouse.modifiers = KeyModifiers::NONE;
        assert!(intro.handle_event(&Event::Mouse(mouse)));
        assert!(intro.started.unwrap().elapsed() < Duration::from_secs(1));
        let restarted = intro.started;
        mouse.column = 50;
        mouse.modifiers = KeyModifiers::SHIFT;
        assert!(!intro.handle_event(&Event::Mouse(mouse)));
        assert_eq!(intro.started, restarted);
    }

    #[test]
    fn intro_leaves_keyboard_wheel_and_hidden_artwork_events_with_the_normal_ui() {
        let mut intro = IntroInteraction {
            area: Some(Rect::new(20, 10, 60, 28)),
            ..IntroInteraction::default()
        };
        assert!(!intro.handle_event(&Event::Key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE
        ))));
        let mut mouse = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 50,
            row: 20,
            modifiers: KeyModifiers::NONE,
        };
        assert!(!intro.handle_event(&Event::Mouse(mouse)));
        intro.area = None;
        mouse.kind = MouseEventKind::Down(MouseButton::Left);
        assert!(!intro.handle_event(&Event::Mouse(mouse)));
        assert!(intro.started.is_none());
    }

    #[test]
    fn transcript_points_are_clamped_to_visible_text_and_global_scroll_rows() {
        let view = Transcript {
            area: Region {
                x: 3,
                y: 5,
                width: 40,
                height: 12,
            },
            start: 50,
            max_scroll: 100,
            line_widths: vec![8, 4],
        };
        assert_eq!(view.point(5, 5), Some([50, 2]));
        assert_eq!(view.point(100, 100), Some([51, 4]));
        assert_eq!(view.point(0, 0), Some([50, 0]));
    }

    #[test]
    fn static_graphics_reuse_pixels_after_moving_but_not_after_theme_or_geometry_changes() {
        let paper: Preview = serde_json::from_value(
            json!({"id":126,"x":1,"y":20,"sequence":"cotton pixels","placement":"behind text"}),
        )
        .unwrap();
        let mut next = paper.clone();
        next.y = 24;
        assert!(paper.retained_in(&[next.clone()]));
        next.sequence = "graphite pixels".into();
        assert!(!paper.retained_in(&[next]));
        assert!(!paper.retained_in(&[]));
        let animation: Preview =
            serde_json::from_value(json!({"id":127,"x":1,"y":2,"sequence":"frame"})).unwrap();
        assert!(!animation.retained_in(&[animation.clone()]));
    }

    #[test]
    fn secret_entry_is_masked_before_crossing_the_presentation_boundary() {
        let mut state = crate::init_state();
        state.ui.input_mode = InputMode::ApiKey;
        state.ui.input = "sk-example-secret".into();
        state.ui.input_cursor = state.ui.input.len();
        let model = ModelConfig {
            base_url: Some("https://private.example".into()),
            ..ModelConfig::default()
        };
        let frame = view(&state, &model, 1, 160, 44);
        assert_eq!(frame["input"], "•".repeat(state.ui.input.chars().count()));
        assert!(!frame.to_string().contains("sk-example-secret"));
        assert!(!frame.to_string().contains("private.example"));
    }
}
