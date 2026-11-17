//! Optional presentation process. The Rust runtime retains all input and agent execution.
use std::{
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
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
fn poll_frame(
    frames: &Receiver<Result<String, String>>,
    sent: Instant,
) -> Result<Option<String>, String> {
    match frames.try_recv() {
        Ok(frame) => frame.map(Some),
        Err(mpsc::TryRecvError::Empty) if sent.elapsed() < FRAME_TIMEOUT => Ok(None),
        _ => Err("TypeScript UI stopped responding".into()),
    }
}

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
    scrollbar: Option<Region>,
}
impl Transcript {
    fn scroll_top_at(&self, y: u16) -> Option<usize> {
        let track = self.scrollbar.as_ref()?;
        let row = y
            .saturating_sub(track.y)
            .min(track.height.saturating_sub(1)) as usize;
        Some(
            self.max_scroll.saturating_mul(row)
                / usize::from(track.height.saturating_sub(1).max(1)),
        )
    }
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
    #[serde(default, rename = "animationIntervalMs")]
    animation_interval_ms: Option<u64>,
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
    tmux_graphics: bool,
    pane_origin: (u16, u16),
    pub(super) animation_interval_ms: Option<u64>,
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
    scrolling: bool,
    copy_pending: bool,
    copied_code: Option<String>,
    pending_since: Option<Instant>,
    last_tail_activity: Option<(usize, usize, usize, usize)>,
    requested_size: (u16, u16),
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
        let tmux_graphics = super::terminal::tmux_graphics_enabled();
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
        if tmux_graphics {
            command.env("PI_IMAGE_PROTOCOL", "kitty");
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
            tmux_graphics,
            pane_origin: (0, 0),
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
            scrolling: false,
            copy_pending: false,
            copied_code: None,
            pending_since: None,
            last_tail_activity: None,
            requested_size: (0, 0),
            animation_interval_ms: None,
        })
    }

    pub fn handle_visual_event(&mut self, event: &Event, state: &mut AgentState) -> bool {
        if let Event::Mouse(mouse) = event {
            if state.ui.input_mode != InputMode::ApiKey
                && state.ui.pending_update.is_none()
                && mouse.kind == MouseEventKind::Down(MouseButton::Left)
                && mouse.modifiers.is_empty()
                && self.controls.iter().any(|control| {
                    control.id == "expand"
                        && control
                            .area
                            .rect()
                            .contains((mouse.column, mouse.row).into())
                })
            {
                state.ui.expand_tools = !state.ui.expand_tools;
                state.ui.follow_tail = false;
                if let Some(view) = &self.transcript {
                    state.ui.exec_scroll = view.max_scroll.saturating_sub(view.start);
                }
                return true;
            }
        }
        if state.ui.pending_permission.is_some() {
            if let Event::Mouse(mouse) = event {
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    if let Some(index) = self
                        .controls
                        .iter()
                        .find(|c| {
                            c.id == "permission"
                                && c.area.rect().contains((mouse.column, mouse.row).into())
                        })
                        .and_then(|c| c.index)
                        .filter(|i| *i < 3)
                    {
                        state.ui.permission_selected = index;
                        super::main_ui::handle_event(
                            state,
                            Event::Key(crossterm::event::KeyEvent::new(
                                KeyCode::Enter,
                                KeyModifiers::NONE,
                            )),
                            Rect::default(),
                            Rect::default(),
                            Rect::default(),
                        );
                        return true;
                    }
                }
            }
        }
        if let Some(update) = state.ui.pending_update.as_mut() {
            if let Event::Mouse(mouse) = event {
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) && !update.installing {
                    if let Some(index) = self
                        .controls
                        .iter()
                        .find(|c| {
                            c.id == "update"
                                && c.area.rect().contains((mouse.column, mouse.row).into())
                        })
                        .and_then(|c| c.index)
                    {
                        match index {
                            0 => state.ui.update_install_requested = true,
                            1 => state.ui.update_later_requested = true,
                            2 => state.ui.update_skip_requested = true,
                            _ => {}
                        }
                    }
                }
                return true;
            }
        }
        if matches!(event,Event::Mouse(m) if m.kind==MouseEventKind::Down(MouseButton::Left)) {
            self.selecting = false;
            self.copy_pending = false;
            self.scrolling = false;
        }
        if state.ui.panel.is_none() && state.ui.pending_update.is_none() {
            if let (Event::Mouse(mouse), Some(view)) = (event, &self.transcript) {
                let on_track = view
                    .scrollbar
                    .as_ref()
                    .is_some_and(|track| track.rect().contains((mouse.column, mouse.row).into()));
                if mouse.modifiers.is_empty()
                    && ((mouse.kind == MouseEventKind::Down(MouseButton::Left) && on_track)
                        || (self.scrolling
                            && matches!(
                                mouse.kind,
                                MouseEventKind::Drag(MouseButton::Left)
                                    | MouseEventKind::Up(MouseButton::Left)
                            )))
                {
                    if let Some(top) = view.scroll_top_at(mouse.row) {
                        state.ui.exec_scroll = if top == view.max_scroll {
                            usize::MAX
                        } else {
                            view.max_scroll - top
                        };
                        state.ui.follow_tail = top == view.max_scroll;
                        state.ui.transcript_selection = None;
                        self.scrolling = mouse.kind != MouseEventKind::Up(MouseButton::Left);
                        return true;
                    }
                }
            }
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
                        match control.id.as_str() {
                            "link" => {
                                if let Some(url) = control.label.as_deref() {
                                    if let Err(error) = super::links::open(url) {
                                        crate::logger::log_status(state, error);
                                    }
                                }
                            }
                            "latest" => {
                                state.ui.follow_tail = true;
                                state.ui.exec_scroll = usize::MAX;
                                state.ui.transcript_selection = None;
                            }
                            "voice" => {
                                state.ui.requested_command = Some(
                                    if state.voice.enabled || state.voice.connected {
                                        "/voice off"
                                    } else {
                                        "/voice on"
                                    }
                                    .into(),
                                )
                            }
                            "copy" => {
                                if let Some(text) = control
                                    .label
                                    .as_deref()
                                    .filter(|text| !text.is_empty() && text.len() <= 100_000)
                                {
                                    match crate::clipboard::copy_text_to_clipboard(text) {
                                        Ok(_) => {
                                            self.copied_code = Some(text.into());
                                            state.ui.input_clipboard = text.into();
                                        }
                                        Err(error) => crate::logger::log(
                                            state,
                                            crate::state::LogLevel::Error,
                                            format!("Copy failed: {error}"),
                                        ),
                                    }
                                }
                            }
                            "team" => match control.index.and_then(|id| i64::try_from(id).ok()) {
                                Some(id) => super::workbench::open_team_message(state, id),
                                None => super::workbench::open(state, 11, None),
                            },
                            _ => super::workbench::open_selector(state, control.id == "provider"),
                        }
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
    ) -> Result<Option<(Rect, Rect, usize, bool)>, String> {
        if self.pending_since.is_none() {
            self.requested_size = (width, height);
            self.sequence += 1;
            let mut snapshot = view(state, model, self.sequence, width, height);
            let activity = (
                state.logs.iter().count(),
                state.logs.iter().last().map_or(0, |log| log.text.len()),
                state.ui.streaming_transcript.len(),
                state
                    .ui
                    .diff_snapshot
                    .last()
                    .map_or(0, |diff| diff.after.len()),
            );
            snapshot["newActivity"] = json!(
                !state.ui.follow_tail
                    && self.last_tail_activity.is_some_and(|seen| seen != activity)
            );
            if state.ui.follow_tail {
                self.last_tail_activity = Some(activity);
            }
            snapshot["introElapsed"] = json!(self
                .intro
                .started
                .get_or_insert_with(std::time::Instant::now)
                .elapsed()
                .as_millis() as u64);
            snapshot["introScene"] =
                json!(["horizon", "interference", "life", "lorenz"][self.intro.variant]);
            snapshot["introBright"] = json!(self.intro.bright);
            snapshot["copiedCode"] = json!(self.copied_code);
            self.input.try_send(snapshot).map_err(|e| e.to_string())?;
            self.pending_since = Some(Instant::now());
        }
        let Some(raw) = poll_frame(&self.frames, self.pending_since.unwrap())? else {
            return Ok(None);
        };
        self.pending_since = None;
        if self.requested_size != (width, height) {
            return Ok(None);
        }
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
                    || t.scrollbar.as_ref().is_some_and(|track| {
                        !track.inside(width, height)
                            || track.width != 1
                            || track.y != t.area.y
                            || track.height != t.area.height
                            || track.x < t.area.x + t.area.width
                    })
            })
            || frame.controls.iter().any(|c| {
                !matches!(
                    c.id.as_str(),
                    "model"
                        | "provider"
                        | "choice"
                        | "voice"
                        | "team"
                        | "copy"
                        | "update"
                        | "permission"
                        | "expand"
                        | "latest"
                        | "link"
                ) || c.id == "choice"
                    && (!c.index.is_some_and(|i| i <= 1000)
                        || c.label.as_ref().is_none_or(|s| s.len() > 160))
                    || c.id == "update" && !c.index.is_some_and(|i| i <= 2)
                    || c.id == "permission" && c.index.is_none_or(|i| i > 2)
                    || c.id == "copy"
                        && c.label.as_ref().is_none_or(|s| {
                            s.is_empty() || s.len() > 100_000 || s.contains('\u{1b}')
                        })
                    || c.id == "link"
                        && c.label.as_ref().is_none_or(|url| !super::links::valid(url))
                    || c.area.width == 0
                    || c.area.height != 1
                    || !c.area.inside(width, height)
            })
            || frame
                .animation_interval_ms
                .is_some_and(|ms| !(16..=1000).contains(&ms))
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
        self.intro.playing = self.intro.area.is_some()
            && frame.animate
            && self
                .intro
                .started
                .is_some_and(|started| started.elapsed() < Duration::from_secs(10));
        self.animation_interval_ms = frame.animation_interval_ms;
        let capture =
            (frame.execution.height > 0 || self.intro.area.is_some() || !frame.controls.is_empty())
                && super::terminal::artwork_mouse_enabled()
                && !super::terminal::mouse_capture_enabled();
        if capture != self.intro.capture {
            if capture {
                // Drag events support transcript selection; hover never affects artwork.
                queue!(output, Print("\x1b[?1000h\x1b[?1002h\x1b[?1006h"))
            } else {
                queue!(output, DisableMouseCapture)
            }
            .map_err(|e| e.to_string())?;
            self.intro.capture = capture;
        }
        for old in &self.images {
            let retained = old.retained_in(&frame.images);
            let delete = if retained { "i" } else { "I" };
            write!(
                output,
                "{}",
                super::terminal::graphic_sequence(
                    &format!("\x1b_Ga=d,d={delete},i={},q=2;\x1b\\", old.id),
                    self.tmux_graphics
                )
            )
            .map_err(|e| e.to_string())?;
        }
        let resized = self.size != (width, height);
        if resized && self.tmux_graphics {
            self.pane_origin = super::terminal::pane_origin()?;
        }
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
                    queue!(
                        output,
                        Print(super::terminal::graphic_at(
                            &image.sequence,
                            self.tmux_graphics,
                            self.pane_origin,
                            image.x,
                            image.y
                        ))
                    )
                    .map_err(|e| e.to_string())?;
                }
                if let Some(placement) = &image.placement {
                    queue!(
                        output,
                        Print(super::terminal::graphic_at(
                            placement,
                            self.tmux_graphics,
                            self.pane_origin,
                            image.x,
                            image.y
                        ))
                    )
                    .map_err(|e| e.to_string())?;
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
        Ok(Some((
            frame.input.rect(),
            frame.execution.rect(),
            frame.editor_width as usize,
            frame.animate,
        )))
    }
}

impl Drop for TypeScriptUi {
    fn drop(&mut self) {
        for image in &self.images {
            let _ = write!(
                io::stdout(),
                "{}",
                super::terminal::graphic_sequence(
                    &format!("\x1b_Ga=d,d=I,i={},q=2;\x1b\\", image.id),
                    self.tmux_graphics
                )
            );
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
        let diff = super::diff::Diff::from_texts(d.target.clone(), &d.before, &d.after, ui.expand_tools);
        let after_log=if d.tool.starts_with("preview:") || d.tool.starts_with("live:"){None}else{state.logs.iter().enumerate().filter(|(_,line)|line.text.trim()==format!("└ edited {}",d.target)).map(|(index,_)|index).last()};
        json!({ "file":diff.file,"added":diff.added,"removed":diff.removed,"preview":d.tool.starts_with("preview:"),"afterLog":after_log, "lines":diff.lines.iter().map(|l| json!({"text":l.text,"kind":format!("{:?}",l.kind),"oldLine":l.old_lineno,"newLine":l.new_lineno})).collect::<Vec<_>>() })
    }).collect()
    } else {
        Vec::new()
    };
    let mut snapshot = json!({
        "id":id, "width":width, "height":height, "version":env!("CARGO_PKG_VERSION"),
        "wallTime":chrono::Utc::now().timestamp_millis(),
        "cwd":cwd, "model":model.model, "provider":model.provider,
        "branch":ui.repo_branch.as_deref().unwrap_or(""), "indexed":ui.indexed, "indexing":ui.indexing,
        "theme":state.theme.as_str(), "accent":state.accent.as_str(), "density":state.density.as_str(), "session":state.session_name,
        "elapsed":state.started_at.elapsed().as_millis() as u64,
        "profile":state.permission_profile.as_str(), "autoApprove":ui.auto_approve, "planMode":state.plan_mode,
        "logs":state.logs.iter().map(|l|json!({"text":l.text,"level":format!("{:?}",l.level),"kind":format!("{:?}",l.kind)})).collect::<Vec<_>>(),
        "plan":state.plan_items, "diffs":diffs, "input":input, "cursor":cursor, "selection":selection,
        "masked":ui.input_mode == InputMode::ApiKey, "inputMode":format!("{:?}",ui.input_mode),
        "attachments":ui.attachments.iter().filter(|a|ui.input.contains(&a.label)).map(|a|json!({"id":a.id,"label":a.label,"thumbnail":a.thumbnail,"width":a.width,"height":a.height})).collect::<Vec<_>>(),
        "running":ui.agent_running, "phase":ui.run_phase, "tool":ui.current_tool,
        "followTail":ui.follow_tail, "scroll":ui.exec_scroll.min(u32::MAX as usize),
        "stream":if ui.streaming_active {ui.streaming_buffer.rsplit('\n').next().unwrap_or("")} else {""},
        "permission":ui.pending_permission.as_ref().map(|p|format!("Allow {} ({})? [y]es [n]o [a]lways", p.tool_name,p.args_summary)),
        "update":ui.pending_update.as_ref().map(super::tui::pending_update_prompt),
        "voice":if state.voice.enabled || state.voice.connected {Some(super::tui::voice_bar_text(state))} else {None},
        "commands":ui.command_items.iter().map(|c|json!({"cmd":c.cmd,"desc":c.desc})).collect::<Vec<_>>(),
        "selectedCommand":ui.command_selected, "tokens":state.usage.prompt_tokens+state.usage.completion_tokens,
    });
    snapshot["queuedMessages"] = json!(ui.queued_agent_prompts.len());
    snapshot["workspaceStatus"] = json!(ui.workspace_status);
    snapshot["userTurns"] = json!(ui.user_turns);
    snapshot["conversationStarted"] = json!(state
        .conversation
        .messages
        .iter()
        .any(|message| message.get("role").and_then(Value::as_str) == Some("user")));
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
    snapshot["release"] = json!(ui.pending_update.as_ref().map(|u|json!({"current":u.current_version,"latest":u.latest_version,"notes":u.notes,"selected":u.selected,"installing":u.installing,"progress":u.progress})));
    snapshot["runElapsed"] = json!(ui
        .run_started_at
        .map(|t| t.elapsed().as_millis() as u64)
        .unwrap_or(0));
    snapshot["expandTools"] = json!(ui.expand_tools);
    snapshot["statusLine"] = json!(super::workbench::statusline(state));
    snapshot["preferences"] = serde_json::to_value(&state.preferences).unwrap_or(Value::Null);
    snapshot["voiceState"] = json!({"enabled":state.voice.enabled,"connected":state.voice.connected,"partial":state.voice.partial.is_some()});
    let project = ui
        .team
        .peers
        .iter()
        .find(|p| p.session == state.session.id)
        .map(|p| &p.project);
    snapshot["team"] = json!({"enabled":state.preferences.team_messages,"unread":ui.team.messages.len(),"online":ui.team.peers.iter().filter(|p|p.online&&p.session!=state.session.id&&Some(&p.project)==project).count()});
    snapshot
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_pending_frame_never_blocks_keyboard_processing_and_timeouts_still_fail() {
        let (tx, rx) = std::sync::mpsc::channel();
        let now = std::time::Instant::now();
        assert!(super::poll_frame(&rx, now).unwrap().is_none());
        assert!(now.elapsed() < std::time::Duration::from_millis(500));
        tx.send(Ok("frame".into())).unwrap();
        assert_eq!(
            super::poll_frame(&rx, now).unwrap().as_deref(),
            Some("frame")
        );
        assert!(super::poll_frame(&rx, now - super::FRAME_TIMEOUT).is_err());
    }
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
            scrollbar: None,
        };
        assert_eq!(view.point(5, 5), Some([50, 2]));
        assert_eq!(view.point(100, 100), Some([51, 4]));
        assert_eq!(view.point(0, 0), Some([50, 0]));
    }

    #[test]
    fn scrollbar_reaches_both_ends_and_clamps_dragging_outside_the_track() {
        let view:Transcript=serde_json::from_value(json!({"x":3,"y":5,"width":40,"height":11,"start":50,"maxScroll":200,"lineWidths":[8],"scrollbar":{"x":44,"y":5,"width":1,"height":11}})).unwrap();
        assert_eq!(view.scroll_top_at(0), Some(0));
        assert_eq!(view.scroll_top_at(10), Some(100));
        assert_eq!(view.scroll_top_at(100), Some(200));
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
    fn inactive_voice_status_is_not_sent_to_the_renderer() {
        let mut state = crate::init_state();
        state.voice.visible = true;
        let frame = view(&state, &ModelConfig::default(), 1, 160, 44);
        assert!(frame["voice"].is_null());
        state.voice.enabled = true;
        assert!(view(&state, &ModelConfig::default(), 2, 160, 44)["voice"]
            .as_str()
            .unwrap()
            .contains("connecting"));
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
