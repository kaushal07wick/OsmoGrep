//! One local workspace drawer. Actions never invoke a model implicitly.
mod actions;
mod index;
use super::{references, review};
use crate::state::{AgentState, InputMode, LogLevel};
pub use actions::source;
use actions::{act, queue_draft, steer_draft};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use index::{index_files, search_saved};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Page {
    Find,
    Context,
    Queue,
    Changes,
    Retry,
    Pins,
    Layout,
    Timing,
}
impl Page {
    pub const ALL: [Self; 8] = [
        Self::Find,
        Self::Context,
        Self::Queue,
        Self::Changes,
        Self::Retry,
        Self::Pins,
        Self::Layout,
        Self::Timing,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Find => "Find",
            Self::Context => "Context",
            Self::Queue => "Queue",
            Self::Changes => "Changes",
            Self::Retry => "Retry",
            Self::Pins => "Pins",
            Self::Layout => "Layout",
            Self::Timing => "Timing",
        }
    }
    fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(value))
    }
}
#[derive(Clone)]
struct Source {
    path: String,
    line: Option<usize>,
    end: Option<usize>,
    symbol: Option<String>,
}
#[derive(Clone)]
enum Target {
    File(Source),
    Log(usize),
    Shell(String),
    Command(String),
    SavedLog { session: String, index: usize },
    Queue(usize, String),
    Hunk(super::review::Hunk),
    Prompt(usize),
    Pin(String),
    Layout(&'static str),
    Timing,
}
#[derive(Clone)]
struct Entry {
    id: String,
    label: String,
    detail: String,
    kind: &'static str,
    target: Target,
}
struct Job {
    receiver: mpsc::Receiver<IndexEvent>,
    cancel: Arc<AtomicBool>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
enum IndexEvent {
    Files(Vec<Source>),
    Saved {
        session: String,
        index: usize,
        title: String,
    },
    Done,
    Error(String),
}
pub struct Panel {
    page: Page,
    query: String,
    selected: usize,
    detail_offset: usize,
    page_offset: usize,
    notice: String,
    files: Vec<Source>,
    contextual: Vec<Entry>,
    hunks: Vec<super::review::Hunk>,
    change_count: usize,
    saved: Vec<Entry>,
    index: Option<Job>,
    search: Option<Job>,
    search_due: Option<std::time::Instant>,
    edit: Option<(usize, String, String, usize)>,
    confirm: Option<String>,
    return_input: Option<(String, usize)>,
}
#[derive(Serialize)]
pub struct Row {
    id: String,
    label: String,
    detail: String,
    kind: &'static str,
}
#[derive(Serialize)]
pub struct Button {
    id: String,
    label: String,
}
#[derive(Serialize)]
pub struct View {
    page: Page,
    tabs: Vec<String>,
    query: String,
    selected: usize,
    rows: Vec<Row>,
    preview: String,
    preview_offset: usize,
    notice: String,
    buttons: Vec<Button>,
    busy: bool,
    editing: bool,
    cursor: usize,
}

pub fn open(state: &mut AgentState, page: Page) {
    if state.ui.input_mode == InputMode::ApiKey {
        return;
    }
    if let Some(panel) = state.ui.experience.take() {
        close(state, panel);
    }
    state.ui.panel = None;
    state.ui.command_items.clear();
    let mut panel = Panel {
        page,
        query: String::new(),
        selected: 0,
        detail_offset: 0,
        page_offset: 0,
        notice: String::new(),
        files: vec![],
        contextual: vec![],
        hunks: if page == Page::Changes {
            super::review::hunks(&state.session_changes)
        } else {
            vec![]
        },
        change_count: if page == Page::Changes {
            state.session_changes.len()
        } else {
            0
        },
        saved: vec![],
        index: None,
        search: None,
        search_due: None,
        edit: None,
        confirm: None,
        return_input: None,
    };
    if matches!(page, Page::Find | Page::Context) {
        panel.index = Some(index_files(state.repo_root.clone()));
    }
    if page == Page::Queue {
        state.ui.queue_paused = true;
        panel.notice = "Queue paused while editing · Resume starts the next turn when ready".into();
    }
    state.ui.experience = Some(panel);
}
pub fn open_context(state: &mut AgentState, from_at: bool) {
    let input = state.ui.input.clone();
    let cursor = state.ui.input_cursor;
    open(state, Page::Context);
    if from_at {
        state.ui.experience.as_mut().unwrap().return_input = Some((input, cursor));
    }
}
fn source_id(source: &Source) -> String {
    format!(
        "file:{}",
        &crate::file_guard::digest(format!("{}:{:?}", source.path, source.line).as_bytes())[..24]
    )
}
fn id(kind: &str, index: usize, text: &str) -> String {
    format!(
        "{kind}:{index}:{}",
        &crate::file_guard::digest(text.as_bytes())[..16]
    )
}
fn matches(query: &str, label: &str) -> bool {
    query
        .split_whitespace()
        .all(|q| label.to_lowercase().contains(q))
}
fn entries(state: &AgentState, panel: &Panel) -> Vec<Entry> {
    let query = panel.query.to_lowercase();
    let query = query.trim_start_matches(['@', '>']);
    let mut rows = Vec::new();
    if matches!(panel.page, Page::Find | Page::Context) {
        if panel.page == Page::Find {
            for (command, label) in [
                ("/find", "Find anything"),
                ("/context", "Attach context"),
                ("/queue", "Edit queued messages"),
                ("/changes", "Review changes"),
                ("/retry", "Edit an earlier request"),
                ("/pins", "Pinned references"),
                ("/layout", "Workspace layout"),
                ("/timing", "Turn timing"),
            ] {
                rows.push(Entry {
                    id: format!("command:{command}"),
                    label: label.into(),
                    detail: command.into(),
                    kind: "action",
                    target: Target::Command(command.into()),
                });
            }
            for command in crate::commands::command_palette_items()
                .iter()
                .chain(crate::commands::advanced_command_items())
            {
                rows.push(Entry {
                    id: format!("command:{}", command.cmd),
                    label: command.cmd.into(),
                    detail: command.desc.into(),
                    kind: "command",
                    target: Target::Command(command.cmd.into()),
                });
            }
        }
        if let Some((path, range)) = panel.query.rsplit_once(':') {
            let parsed = range.split_once('-').map_or_else(
                || range.parse::<usize>().ok().map(|n| (n, n)),
                |(a, b)| Some((a.parse().ok()?, b.parse().ok()?)),
            );
            if let Some((start, end)) = parsed.filter(|(a, b)| *a > 0 && b >= a) {
                if panel
                    .files
                    .iter()
                    .any(|f| f.path == path && f.line.is_none())
                {
                    let source = Source {
                        path: path.into(),
                        line: Some(start),
                        end: Some(end),
                        symbol: None,
                    };
                    rows.push(Entry {
                        id: source_id(&source),
                        label: panel.query.clone(),
                        detail: "Selected lines · Attach to snapshot".into(),
                        kind: "range",
                        target: Target::File(source),
                    });
                }
            }
        }
        for source in &panel.files {
            let label = source.symbol.clone().unwrap_or_else(|| source.path.clone());
            let detail = source.line.map_or_else(
                || {
                    if source.path.ends_with('/') {
                        "Folder · attach file list".into()
                    } else {
                        "File".into()
                    }
                },
                |line| format!("{}:{line}", source.path),
            );
            rows.push(Entry {
                id: source_id(source),
                label,
                detail,
                kind: if source.symbol.is_some() {
                    "symbol"
                } else {
                    "file"
                },
                target: Target::File(source.clone()),
            });
        }
        for (index, line) in state
            .logs
            .iter()
            .enumerate()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .filter(|(_, l)| l.kind != crate::state::LogKind::Peer)
        {
            rows.push(Entry {
                id: id("log", index, &line.text),
                label: line
                    .text
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .trim_start_matches("USER|")
                    .chars()
                    .take(120)
                    .collect(),
                detail: format!("Current conversation · message {}", index + 1),
                kind: if line.text.starts_with("USER|") {
                    "request"
                } else {
                    "output"
                },
                target: Target::Log(index),
            });
        }
        if panel.page == Page::Find {
            rows.extend(panel.saved.clone());
        }
    }
    match panel.page {
        Page::Queue => {
            for (index, text) in state.ui.queued_agent_prompts.iter().enumerate() {
                rows.push(Entry {
                    id: id("queue", index, text),
                    label: text.lines().next().unwrap_or("").into(),
                    detail: format!("Next {}", index + 1),
                    kind: "queued",
                    target: Target::Queue(index, text.clone()),
                });
            }
        }
        Page::Changes => {
            for hunk in panel.hunks.iter().cloned() {
                let kept = state.ui.reviewed_hunks.contains(&hunk.id);
                rows.push(Entry {
                    id: hunk.id.clone(),
                    label: format!("{}:{}", hunk.path, hunk.new_start + 1),
                    detail: format!(
                        "+{} −{}{}",
                        hunk.new_end - hunk.new_start,
                        hunk.old_end - hunk.old_start,
                        if kept { " · kept" } else { "" }
                    ),
                    kind: "hunk",
                    target: Target::Hunk(hunk),
                });
            }
        }
        Page::Retry => {
            for (index, message) in state
                .conversation
                .messages
                .iter()
                .enumerate()
                .rev()
                .filter(|(_, m)| m["role"] == "user" && m["content"].is_string())
            {
                let text = message["content"].as_str().unwrap();
                rows.push(Entry {
                    id: id("prompt", index, text),
                    label: text.lines().next().unwrap_or("").into(),
                    detail: "Edit in a new conversation branch".into(),
                    kind: "request",
                    target: Target::Prompt(index),
                });
            }
        }
        Page::Layout => {
            for (mode, label, description) in [
                (
                    "auto",
                    "Chat + code",
                    "Responsive split with remembered editor width",
                ),
                ("chat", "Focus chat", "Temporarily maximize the chat pane"),
                ("code", "Focus code", "Temporarily maximize the editor pane"),
                ("review", "Review changes", "Read and review selected hunks"),
                (
                    "preview",
                    "Focus preview",
                    "Open Workspace to choose an existing app viewer",
                ),
            ] {
                rows.push(Entry {
                    id: format!("layout:{mode}"),
                    label: label.into(),
                    detail: description.into(),
                    kind: if state.preferences.workspace_layout == mode {
                        "active"
                    } else {
                        "layout"
                    },
                    target: Target::Layout(mode),
                });
            }
        }
        Page::Timing => {
            for (index, (label, value)) in state.ui.timing.rows().into_iter().enumerate() {
                rows.push(Entry {
                    id: format!("timing:{index}"),
                    label,
                    detail: value,
                    kind: "measured",
                    target: Target::Timing,
                });
            }
        }
        _ => {}
    }
    if matches!(panel.page, Page::Pins | Page::Find | Page::Context) {
        for pin in &state.preferences.pins {
            rows.push(Entry {
                id: format!("pin:{}", pin.id),
                label: pin.title.clone(),
                detail: format!("{} chars · local reference", pin.body.chars().count()),
                kind: "pin",
                target: Target::Pin(pin.id.clone()),
            });
        }
    }
    if !panel.contextual.is_empty() {
        rows = panel.contextual.clone();
    }
    rows.retain(|row| {
        if matches!(row.target, Target::SavedLog { .. }) {
            return true;
        }
        let body = match &row.target {
            Target::Log(i) => state.logs.iter().nth(*i).map(|l| l.text.as_str()),
            Target::Pin(id) => state
                .preferences
                .pins
                .iter()
                .find(|p| &p.id == id)
                .map(|p| p.body.as_str()),
            Target::Queue(_, text) => Some(text.as_str()),
            _ => None,
        };
        matches(
            query,
            &format!("{} {} {}", row.label, row.detail, body.unwrap_or("")),
        )
    });
    rows
}
fn preview(state: &AgentState, panel: &Panel, entry: &Entry) -> Result<String, String> {
    match &entry.target {
        Target::File(source) => {
            if source.path.ends_with('/') {
                return Ok(panel.files.iter().filter(|f|f.line.is_none()&&!f.path.ends_with('/')&&f.path.starts_with(&source.path)).map(|f|f.path.clone()).collect::<Vec<_>>().join("\n"));
            }
            if crate::attachments::dropped_image(&source.path,&state.repo_root).is_some() {
                return Ok(format!("{}\n\nAttach adds an image chip and thumbnail to the composer.",source.path));
            }
            let text=super::references::read_file(&state.repo_root,&source.path)?;
            if let Some(line)=source.line {
                Ok(text.lines().enumerate().skip(line.saturating_sub(4)).take(source.end.unwrap_or(line).saturating_sub(line)+8).map(|(i,l)|format!("{:>4}  {l}",i+1)).collect::<Vec<_>>().join("\n"))
            } else { Ok(text) }
        }
        Target::Log(index) => state.logs.iter().nth(*index).map(|l|l.text.trim_start_matches("USER|").to_string()).ok_or_else(||"Message changed; search again.".into()),
        Target::SavedLog {session,index} => crate::persistence::read_session_log(&state.repo_root,session,*index),
        Target::Queue(_,text)|Target::Shell(text) => Ok(text.clone()),
        Target::Hunk(hunk) => Ok(super::review::preview(hunk)),
        Target::Prompt(index) => Ok(state.conversation.messages[*index]["content"].as_str().unwrap_or("").into()),
        Target::Pin(id) => state.preferences.pins.iter().find(|p|&p.id==id).map(|p|p.body.clone()).ok_or_else(||"Pin removed".into()),
        Target::Timing => Ok("Observed on this client.\n\nProvider time includes the request and streamed response. Approval waits are measured separately. UI round trips overlap agent work.\n\nFirst visible output is measured from submission; provider-internal thinking and network-only timing are not supplied.".into()),
        _ => Ok(format!("{}\n\n{}",entry.label,entry.detail)),
    }
}
fn verbs(page: Page, target: &Target) -> Vec<(&'static str, &'static str)> {
    match target {
        Target::File(source) => {
            if page == Page::Context && source.symbol.as_deref() != Some("source location") {
                vec![("attach", "Attach"), ("open", "Open"), ("pin", "Pin")]
            } else {
                vec![("open", "Open"), ("attach", "Attach"), ("pin", "Pin")]
            }
        }
        Target::Shell(_) => vec![("rerun", "Prepare rerun")],
        Target::Log(_) if page == Page::Find => vec![
            ("jump", "Jump to message"),
            ("attach", "Use as context"),
            ("pin", "Pin"),
            ("ask", "Ask about this"),
        ],
        Target::Log(_) | Target::SavedLog { .. } => vec![
            ("attach", "Use as context"),
            ("pin", "Pin"),
            ("ask", "Ask about this"),
        ],
        Target::Command(_) => vec![("open", "Open")],
        Target::Queue(..) => vec![
            ("edit", "Edit"),
            ("up", "Move up"),
            ("down", "Move down"),
            ("remove", "Remove"),
            ("steer", "Steer now"),
        ],
        Target::Hunk(_) => vec![
            ("keep", "Keep"),
            ("revert", "Revert lines"),
            ("open", "Open source"),
        ],
        Target::Prompt(_) => vec![("retry", "Edit and retry")],
        Target::Pin(_) => vec![("attach", "Attach"), ("remove", "Unpin")],
        Target::Layout(_) => vec![
            ("open", "Apply"),
            ("narrow", "Code −5%"),
            ("widen", "Code +5%"),
        ],
        Target::Timing => vec![],
    }
}
pub fn view(state: &AgentState) -> Option<View> {
    let panel = state.ui.experience.as_ref()?;
    let all = entries(state, panel);
    let selected = panel.selected.min(all.len().saturating_sub(1));
    let entry = all.get(selected);
    let rows = all
        .iter()
        .skip(panel.page_offset)
        .take(100)
        .map(|e| Row {
            id: e.id.clone(),
            label: e.label.clone(),
            detail: e.detail.clone(),
            kind: e.kind,
        })
        .collect();
    let mut buttons = entry
        .map(|e| {
            verbs(panel.page, &e.target)
                .into_iter()
                .map(|(verb, label)| Button {
                    id: format!("act:{verb}:{}", e.id),
                    label: label.into(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if panel.page == Page::Queue {
        buttons.push(Button {
            id: "resume".into(),
            label: "Resume queue".into(),
        });
        buttons.push(Button {
            id: "send-next".into(),
            label: "Send draft next".into(),
        });
        buttons.push(Button {
            id: "steer-draft".into(),
            label: "Steer draft now".into(),
        });
    }
    if all.len() > panel.page_offset + 100 {
        buttons.push(Button {
            id: "next-page".into(),
            label: "Next results".into(),
        });
    }
    if panel.page_offset > 0 {
        buttons.push(Button {
            id: "previous-page".into(),
            label: "Previous results".into(),
        });
    }
    Some(View {
        page: panel.page,
        tabs: Page::ALL.map(|p| p.name().to_string()).to_vec(),
        query: panel
            .edit
            .as_ref()
            .map_or_else(|| panel.query.clone(), |_| state.ui.input.clone()),
        selected: selected.saturating_sub(panel.page_offset),
        rows,
        preview: entry
            .map(|e| preview(state, panel, e).unwrap_or_else(|e| e))
            .unwrap_or_else(|| match panel.page {
                Page::Pins => {
                    "Pin a useful answer or selection from Find. Pins stay local until attached."
                        .into()
                }
                Page::Queue => {
                    "No queued messages. Your current draft stays in the composer.".into()
                }
                Page::Changes => "No agent changes in this session.".into(),
                _ => "Type to search · ↑↓ select · enter open".into(),
            }),
        preview_offset: panel.detail_offset,
        notice: panel.notice.clone(),
        buttons,
        busy: panel.index.is_some() || panel.search.is_some(),
        editing: panel.edit.is_some(),
        cursor: if panel.edit.is_some() {
            state.ui.input[..state.ui.input_cursor].chars().count()
        } else {
            panel.query.chars().count()
        },
    })
}
pub fn poll(state: &mut AgentState) -> bool {
    let Some(mut panel) = state.ui.experience.take() else {
        return false;
    };
    let mut changed = false;
    if panel
        .search_due
        .is_some_and(|due| std::time::Instant::now() >= due)
    {
        panel.search_due = None;
        panel.search = Some(search_saved(
            state.repo_root.clone(),
            panel.query.to_lowercase(),
        ));
        changed = true;
    }
    for saved in [false, true] {
        let job = if saved {
            &mut panel.search
        } else {
            &mut panel.index
        };
        if let Some(worker) = job.as_ref() {
            let mut done = false;
            for _ in 0..16 {
                match worker.receiver.try_recv() {
                    Ok(IndexEvent::Files(files)) => {
                        panel.files.extend(files);
                        changed = true;
                    }
                    Ok(IndexEvent::Saved {
                        session,
                        index,
                        title,
                    }) => {
                        panel.saved.push(Entry {
                            id: format!("saved:{session}:{index}"),
                            label: title,
                            detail: "Saved conversation".into(),
                            kind: "history",
                            target: Target::SavedLog { session, index },
                        });
                        changed = true;
                    }
                    Ok(IndexEvent::Error(e)) => {
                        panel.notice = e;
                        done = true;
                        changed = true;
                        break;
                    }
                    Ok(IndexEvent::Done) | Err(mpsc::TryRecvError::Disconnected) => {
                        done = true;
                        changed = true;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                }
            }
            if done {
                *job = None;
            }
        }
    }
    state.ui.experience = Some(panel);
    changed
}
fn search_changed(_state: &AgentState, panel: &mut Panel) {
    panel.selected = 0;
    panel.page_offset = 0;
    panel.detail_offset = 0;
    panel.confirm = None;
    panel.saved.clear();
    panel.search = None;
    panel.search_due = None;
    if panel.page == Page::Find && panel.query.chars().count() >= 2 {
        panel.search_due = Some(std::time::Instant::now() + std::time::Duration::from_millis(180));
    }
}
fn close(state: &mut AgentState, mut panel: Panel) {
    if let Some((_, _, input, cursor)) = panel.edit.take() {
        state.ui.input = input;
        state.ui.input_cursor = cursor;
        state.ui.input_selection_anchor = None;
    }
    if let Some((input, cursor)) = panel.return_input.take() {
        state.ui.input = input;
        state.ui.input_cursor = cursor;
    }
}
pub fn choose(state: &mut AgentState, action: &str) -> bool {
    if let Some(page) = action.strip_prefix("open:").and_then(Page::parse) {
        open(state, page);
        return true;
    }
    if matches!(action, "send-next" | "steer-draft") && state.ui.experience.is_none() {
        let result = if action == "send-next" {
            queue_draft(state)
        } else {
            steer_draft(state)
        };
        if let Err(e) = result {
            crate::logger::log(state, LogLevel::Warn, e);
        }
        return true;
    }
    let Some(mut panel) = state.ui.experience.take() else {
        return false;
    };
    let mut closed = false;
    if panel.edit.is_some() {
        state.ui.experience = Some(panel);
        return true;
    }
    if let Some(page) = action.strip_prefix("tab:").and_then(Page::parse) {
        panel.page = page;
        if page == Page::Changes {
            panel.hunks = super::review::hunks(&state.session_changes);
            panel.change_count = state.session_changes.len();
        }
        panel.query.clear();
        panel.selected = 0;
        panel.page_offset = 0;
        panel.detail_offset = 0;
        panel.edit = None;
        panel.confirm = None;
        if matches!(page, Page::Find | Page::Context)
            && panel.files.is_empty()
            && panel.index.is_none()
        {
            panel.index = Some(index_files(state.repo_root.clone()));
        }
        if page == Page::Queue {
            state.ui.queue_paused = true;
            panel.notice = "Queue paused while editing".into();
        } else {
            panel.notice.clear();
        }
    } else if action == "resume" && panel.page == Page::Queue {
        state.ui.queue_paused = false;
        closed = true;
    } else if action == "send-next" || action == "steer-draft" {
        let r = if action == "send-next" {
            queue_draft(state)
        } else {
            steer_draft(state)
        };
        if let Err(e) = r {
            panel.notice = e;
        } else {
            panel.notice = "Draft queued".into();
        }
    } else if action == "next-page" {
        let count = entries(state, &panel).len();
        if panel.page_offset + 100 < count {
            panel.page_offset += 100;
            panel.selected = panel.page_offset;
            panel.detail_offset = 0;
        }
    } else if action == "previous-page" {
        panel.page_offset = panel.page_offset.saturating_sub(100);
        panel.selected = panel.page_offset;
        panel.detail_offset = 0;
    } else {
        let rows = entries(state, &panel);
        if let Some(id) = action.strip_prefix("select:") {
            if let Some(index) = rows.iter().position(|e| e.id == id) {
                panel.selected = index;
                panel.detail_offset = 0;
                panel.confirm = None;
            }
        } else if let Some(rest) = action.strip_prefix("act:") {
            if let Some((verb, id)) = rest.split_once(':') {
                if let Some(entry) = rows.iter().find(|e| e.id == id) {
                    if verbs(panel.page, &entry.target)
                        .iter()
                        .any(|(v, _)| *v == verb)
                    {
                        match act(state, &mut panel, verb, entry) {
                            Ok(close) => closed = close,
                            Err(e) => panel.notice = e,
                        }
                    }
                }
            }
        }
    }
    if closed {
        close(state, panel);
    } else {
        state.ui.experience = Some(panel);
    }
    true
}
pub fn handle_key(state: &mut AgentState, key: KeyEvent) -> bool {
    let Some(mut panel) = state.ui.experience.take() else {
        return false;
    };
    if let Some((index, expected, original, cursor)) = panel.edit.clone() {
        let handled = match key.code {
            KeyCode::Esc => {
                state.ui.input = original;
                state.ui.input_cursor = cursor;
                state.ui.input_selection_anchor = None;
                panel.edit = None;
                true
            }
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                if state.ui.queued_agent_prompts.get(index) != Some(&expected) {
                    panel.notice = "Queue changed; copy your edits before cancelling.".into();
                } else if state.ui.input.trim().is_empty() {
                    panel.notice = "Use Remove to delete a queued message.".into();
                } else {
                    state.ui.queued_agent_prompts[index] = state.ui.input.clone();
                    if let Err(e) = crate::persistence::save(state) {
                        state.ui.queued_agent_prompts[index] = expected;
                        panel.notice = e;
                    } else {
                        state.ui.input = original;
                        state.ui.input_cursor = cursor;
                        state.ui.input_selection_anchor = None;
                        panel.edit = None;
                        panel.notice = "Queued message saved".into();
                    }
                }
                true
            }
            KeyCode::Tab => {
                state.insert_text("    ");
                true
            }
            _ => false,
        };
        state.ui.experience = Some(panel);
        return handled;
    }
    if let KeyCode::Char(digit @ '1'..='9') = key.code {
        if key.modifiers.contains(KeyModifiers::ALT) {
            state.ui.experience = Some(panel);
            let choice = view(state).and_then(|v| {
                v.buttons
                    .get((digit as u8 - b'1') as usize)
                    .map(|b| b.id.clone())
            });
            return choice.is_some_and(|choice| choose(state, &choice));
        }
    }
    let count = entries(state, &panel).len();
    match key.code {
        KeyCode::Esc => {
            close(state, panel);
            return true;
        }
        KeyCode::Up => panel.selected = panel.selected.saturating_sub(1),
        KeyCode::Down => panel.selected = (panel.selected + 1).min(count.saturating_sub(1)),
        KeyCode::PageUp => panel.detail_offset = panel.detail_offset.saturating_sub(8),
        KeyCode::PageDown => panel.detail_offset = panel.detail_offset.saturating_add(8),
        KeyCode::Home => panel.detail_offset = 0,
        KeyCode::End => panel.detail_offset = u32::MAX as usize,
        KeyCode::Tab | KeyCode::BackTab => {
            let at = Page::ALL.iter().position(|p| *p == panel.page).unwrap();
            let back = key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT);
            let next = Page::ALL[(at + 8 + if back { 7 } else { 1 }) % 8];
            state.ui.experience = Some(panel);
            return choose(state, &format!("tab:{}", next.name()));
        }
        KeyCode::Enter => {
            if let Some(entry) = entries(state, &panel).get(panel.selected) {
                if let Some((verb, _)) = verbs(panel.page, &entry.target).first() {
                    let action = format!("act:{verb}:{}", entry.id);
                    state.ui.experience = Some(panel);
                    return choose(state, &action);
                }
            }
        }
        KeyCode::Backspace => {
            panel.query.pop();
            search_changed(state, &mut panel);
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            panel.query.clear();
            search_changed(state, &mut panel);
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) =>
        {
            panel.query.push(c);
            search_changed(state, &mut panel);
        }
        _ => {}
    }
    panel.page_offset = panel.selected / 100 * 100;
    if matches!(key.code, KeyCode::Up | KeyCode::Down) {
        panel.detail_offset = 0;
        panel.confirm = None;
    }
    state.ui.experience = Some(panel);
    true
}
pub fn paste(state: &mut AgentState, text: &str) {
    if let Some(mut panel) = state.ui.experience.take() {
        let clean = text.replace('\r', "\n");
        if panel.edit.is_some() {
            state.insert_text(&clean);
        } else {
            panel
                .query
                .extend(clean.chars().filter(|c| !c.is_control()));
            search_changed(state, &mut panel);
        }
        state.ui.experience = Some(panel);
    }
}
pub fn scrolled(state: &mut AgentState, requested: usize, offset: usize) {
    if let Some(panel) = state.ui.experience.as_mut() {
        if panel.detail_offset == requested {
            panel.detail_offset = offset.min(requested);
        }
    }
}

pub fn editing(state: &AgentState) -> bool {
    state
        .ui
        .experience
        .as_ref()
        .is_some_and(|p| p.edit.is_some())
}

#[cfg(test)]
mod tests;

/// The native renderer keeps keyboard access when Node is unavailable.
pub fn draw(frame: &mut ratatui::Frame, state: &AgentState) -> bool {
    use ratatui::{
        style::{Color, Style},
        text::Line,
        widgets::{Clear, Paragraph, Wrap},
    };
    let Some(view) = view(state) else {
        return false;
    };
    let area = frame.size();
    let orange = Style::default().fg(Color::Rgb(235, 155, 105));
    let mut lines = vec![
        Line::styled(
            view.tabs
                .iter()
                .map(|tab| {
                    if tab.eq_ignore_ascii_case(view.page.name()) {
                        format!("● {tab}")
                    } else {
                        tab.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("  "),
            orange,
        ),
        Line::from(""),
        Line::from(format!(
            "{}: {}",
            if view.editing { "Edit" } else { "Find" },
            view.query
        )),
        Line::from(""),
    ];
    let count = usize::from(area.height.saturating_sub(12)).min(6);
    let start = view
        .selected
        .saturating_sub(count / 2)
        .min(view.rows.len().saturating_sub(count));
    for (i, row) in view.rows.iter().enumerate().skip(start).take(count) {
        lines.push(Line::styled(
            format!(
                "{} {}",
                if i == view.selected { "›" } else { " " },
                row.label
            ),
            if i == view.selected {
                orange
            } else {
                Style::default().fg(Color::White)
            },
        ));
    }
    lines.push(Line::from(""));
    let body = view.preview.lines().collect::<Vec<_>>();
    let height = area.height.saturating_sub(lines.len() as u16 + 4) as usize;
    let offset = view.preview_offset.min(body.len().saturating_sub(height));
    lines.extend(
        body.into_iter()
            .skip(offset)
            .take(height)
            .map(|line| Line::from(line.to_owned())),
    );
    lines.push(Line::styled(
        view.buttons
            .iter()
            .enumerate()
            .map(|(i, b)| format!("Alt+{} {}", i + 1, b.label))
            .collect::<Vec<_>>()
            .join(" · "),
        orange,
    ));
    lines.push(Line::from(view.notice));
    lines.push(Line::from(
        "↑↓ select · enter open · pgup/pgdn preview · esc close",
    ));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false }),
        area,
    );
    true
}
