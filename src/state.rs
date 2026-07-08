use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Instant;

pub const MAX_LOGS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    AgentText,
    Shell,
    Command,
    ApiKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Success,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
pub struct LogLine {
    pub level: LogLevel,
    pub text: String,
    pub at: Instant,
}

pub struct LogBuffer {
    logs: VecDeque<LogLine>,
}

impl LogBuffer {
    pub fn new() -> Self {
        Self {
            logs: VecDeque::with_capacity(MAX_LOGS),
        }
    }

    pub fn clear(&mut self) {
        self.logs.clear();
    }

    pub fn push(&mut self, level: LogLevel, text: impl Into<String>) {
        if self.logs.len() >= MAX_LOGS {
            self.logs.pop_front();
        }

        self.logs.push_back(LogLine {
            level,
            text: text.into(),
            at: Instant::now(),
        });
    }

    pub fn iter(&self) -> impl Iterator<Item = &LogLine> {
        self.logs.iter()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSnapshot {
    pub tool: String,
    pub target: String,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Copy)]
pub struct CommandItem {
    pub cmd: &'static str,
    pub desc: &'static str,
}

pub struct UiState {
    // input
    pub input: String,
    pub input_cursor: usize,
    pub input_clipboard: String,
    pub input_all_selected: bool,
    pub input_mode: InputMode,
    pub input_masked: bool,
    pub input_placeholder: Option<String>,
    pub execution_pending: bool,
    pub should_exit: bool,
    pub indexing: bool,
    pub indexed: bool,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub hint: Option<String>,
    pub autocomplete: Option<String>,
    pub diff_active: bool,
    pub diff_snapshot: Vec<DiffSnapshot>,
    pub command_items: Vec<CommandItem>,
    pub command_selected: usize,
    pub last_activity: Instant,
    pub exec_scroll: usize,
    pub follow_tail: bool,
    pub active_spinner: Option<String>,
    pub spinner_started_at: Option<Instant>,
    pub agent_running: bool,
    pub run_phase: String,
    pub run_detail: Option<String>,
    pub run_iteration: usize,
    pub run_iteration_limit: usize,
    pub current_tool: Option<String>,
    pub current_tool_detail: Option<String>,
    pub last_tool_status: Option<String>,
    pub cancel_requested: bool,
    pub auto_approve: bool,
    pub pending_permission: Option<PendingPermission>,
    pub pending_update: Option<PendingUpdate>,
    pub update_check_status: Option<String>,
    pub update_install_requested: bool,
    pub update_skip_requested: bool,
    pub streaming_buffer: String,
    pub streaming_transcript: String,
    pub last_rendered_output: Option<String>,
    pub streaming_active: bool,
    pub streaming_lines_logged: usize,
    pub tmux_attach_session: Option<String>,
    pub active_edit_target: Option<String>,
    pub queued_agent_prompt: Option<String>,
    pub repo_branch: Option<String>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            input: String::new(),
            input_cursor: 0,
            input_clipboard: String::new(),
            input_all_selected: false,
            input_mode: InputMode::AgentText,
            input_masked: false,
            input_placeholder: None,
            execution_pending: false,
            should_exit: false,
            indexing: false,
            indexed: false,
            history: Vec::new(),
            history_index: None,
            hint: None,
            autocomplete: None,
            diff_active: false,
            diff_snapshot: Vec::new(),
            command_items: Vec::new(),
            command_selected: 0,
            last_activity: Instant::now(),
            exec_scroll: usize::MAX,
            follow_tail: true,
            active_spinner: None,
            spinner_started_at: None,
            agent_running: false,
            run_phase: "idle".to_string(),
            run_detail: None,
            run_iteration: 0,
            run_iteration_limit: 0,
            current_tool: None,
            current_tool_detail: None,
            last_tool_status: None,
            cancel_requested: false,
            auto_approve: false,
            pending_permission: None,
            pending_update: None,
            update_check_status: None,
            update_install_requested: false,
            update_skip_requested: false,
            streaming_buffer: String::new(),
            streaming_transcript: String::new(),
            last_rendered_output: None,
            streaming_active: false,
            streaming_lines_logged: 0,
            tmux_attach_session: None,
            active_edit_target: None,
            queued_agent_prompt: None,
            repo_branch: None,
        }
    }
}

pub struct AgentState {
    pub ui: UiState,
    pub logs: LogBuffer,
    pub session_changes: Vec<DiffSnapshot>,
    pub reviewed_change_count: usize,
    pub undo_stack: Vec<DiffSnapshot>,
    pub usage: UsageStats,
    pub steer: Option<String>,
    pub auto_eval: bool,
    pub permission_profile: PermissionProfile,
    pub jobs: Vec<JobRecord>,
    pub job_queue: Vec<JobRequest>,
    pub next_job_id: u64,
    pub plan_items: Vec<PlanItem>,
    pub session_name: Option<String>,
    pub theme: UiTheme,
    pub accent: UiAccent,
    pub density: UiDensity,
    pub plan_mode: bool,

    pub started_at: Instant,
    pub repo_root: PathBuf,
    pub voice: VoiceState,
    pub conversation: ConversationHistory,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiTheme {
    Dark,
    Light,
}

impl UiTheme {
    pub fn as_str(self) -> &'static str {
        match self {
            UiTheme::Dark => "dark",
            UiTheme::Light => "light",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dark" | "night" => Some(Self::Dark),
            "light" | "day" => Some(Self::Light),
            _ => None,
        }
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self::Dark
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiAccent {
    Orange,
    Blue,
    Green,
    Violet,
    Rose,
}

impl UiAccent {
    pub fn as_str(self) -> &'static str {
        match self {
            UiAccent::Orange => "orange",
            UiAccent::Blue => "blue",
            UiAccent::Green => "green",
            UiAccent::Violet => "violet",
            UiAccent::Rose => "rose",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "orange" | "amber" => Some(Self::Orange),
            "blue" | "cyan" => Some(Self::Blue),
            "green" | "emerald" => Some(Self::Green),
            "violet" | "purple" => Some(Self::Violet),
            "rose" | "red" | "pink" => Some(Self::Rose),
            _ => None,
        }
    }
}

impl Default for UiAccent {
    fn default() -> Self {
        Self::Orange
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiDensity {
    Compact,
    Standard,
    Spacious,
}

impl UiDensity {
    pub fn as_str(self) -> &'static str {
        match self {
            UiDensity::Compact => "compact",
            UiDensity::Standard => "standard",
            UiDensity::Spacious => "spacious",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "compact" | "small" | "dense" => Some(Self::Compact),
            "standard" | "normal" | "default" => Some(Self::Standard),
            "spacious" | "large" | "roomy" => Some(Self::Spacious),
            _ => None,
        }
    }
}

impl Default for UiDensity {
    fn default() -> Self {
        Self::Standard
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionProfile {
    ReadOnly,
    WorkspaceAuto,
    FullAccess,
}

impl PermissionProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionProfile::ReadOnly => "read-only",
            PermissionProfile::WorkspaceAuto => "workspace-auto",
            PermissionProfile::FullAccess => "full-access",
        }
    }

    pub fn parse(v: &str) -> Option<Self> {
        match v.trim().to_ascii_lowercase().as_str() {
            "read-only" | "readonly" => Some(Self::ReadOnly),
            "workspace-auto" | "workspace" | "auto" => Some(Self::WorkspaceAuto),
            "full-access" | "full" => Some(Self::FullAccess),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Swarm,
    Test,
    Review,
}

impl JobKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobKind::Swarm => "swarm",
            JobKind::Test => "test",
            JobKind::Review => "review",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: u64,
    pub kind: JobKind,
    pub input: String,
    pub status: JobStatus,
    pub output: Option<String>,
}

#[derive(Clone, Debug)]
pub struct JobRequest {
    pub id: u64,
    pub kind: JobKind,
    pub input: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanItem {
    pub text: String,
    pub done: bool,
    #[serde(default)]
    pub active: bool,
}

impl AgentState {
    pub fn push_char(&mut self, c: char) {
        let mut buf = [0; 4];
        self.insert_text(c.encode_utf8(&mut buf));
    }

    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        if self.ui.input_all_selected {
            self.ui.input.clear();
            self.ui.input_cursor = 0;
            self.ui.input_all_selected = false;
        }

        let cursor = clamp_char_boundary(&self.ui.input, self.ui.input_cursor);
        self.ui.input.insert_str(cursor, text);
        self.ui.input_cursor = cursor + text.len();
        self.ui.history_index = None;
    }

    pub fn backspace(&mut self) {
        if self.ui.input_all_selected {
            self.ui.input.clear();
            self.ui.input_cursor = 0;
            self.ui.input_all_selected = false;
            self.ui.history_index = None;
            return;
        }

        let cursor = clamp_char_boundary(&self.ui.input, self.ui.input_cursor);
        if cursor == 0 {
            return;
        }

        let previous = previous_char_boundary(&self.ui.input, cursor);
        self.ui.input.drain(previous..cursor);
        self.ui.input_cursor = previous;
        self.ui.history_index = None;
    }

    pub fn delete_forward(&mut self) {
        if self.ui.input_all_selected {
            self.clear_input();
            return;
        }

        let cursor = clamp_char_boundary(&self.ui.input, self.ui.input_cursor);
        if cursor >= self.ui.input.len() {
            return;
        }

        let next = next_char_boundary(&self.ui.input, cursor);
        self.ui.input.drain(cursor..next);
        self.ui.input_cursor = cursor;
        self.ui.history_index = None;
    }

    pub fn move_cursor_left(&mut self) {
        self.ui.input_all_selected = false;
        let cursor = clamp_char_boundary(&self.ui.input, self.ui.input_cursor);
        self.ui.input_cursor = previous_char_boundary(&self.ui.input, cursor);
    }

    pub fn move_cursor_right(&mut self) {
        self.ui.input_all_selected = false;
        let cursor = clamp_char_boundary(&self.ui.input, self.ui.input_cursor);
        self.ui.input_cursor = next_char_boundary(&self.ui.input, cursor);
    }

    pub fn move_cursor_line_start(&mut self) {
        self.ui.input_all_selected = false;
        let cursor = clamp_char_boundary(&self.ui.input, self.ui.input_cursor);
        self.ui.input_cursor = current_line_bounds(&self.ui.input, cursor).0;
    }

    pub fn move_cursor_line_end(&mut self) {
        self.ui.input_all_selected = false;
        let cursor = clamp_char_boundary(&self.ui.input, self.ui.input_cursor);
        self.ui.input_cursor = current_line_bounds(&self.ui.input, cursor).1;
    }

    pub fn move_cursor_up(&mut self) -> bool {
        self.move_cursor_vertical(-1)
    }

    pub fn move_cursor_down(&mut self) -> bool {
        self.move_cursor_vertical(1)
    }

    fn move_cursor_vertical(&mut self, direction: i8) -> bool {
        self.ui.input_all_selected = false;
        let cursor = clamp_char_boundary(&self.ui.input, self.ui.input_cursor);
        let (start, end) = current_line_bounds(&self.ui.input, cursor);
        let column = self.ui.input[start..cursor].chars().count();

        let Some((target_start, target_end)) =
            adjacent_line_bounds(&self.ui.input, start, end, direction)
        else {
            return false;
        };

        self.ui.input_cursor =
            byte_index_at_char_column(&self.ui.input, target_start, target_end, column);
        true
    }

    pub fn select_all_input(&mut self) -> bool {
        if self.ui.input.is_empty() {
            return false;
        }

        self.ui.input_all_selected = true;
        true
    }

    pub fn copy_input(&mut self) -> bool {
        if self.ui.input.is_empty() {
            return false;
        }

        self.ui.input_clipboard = self.ui.input.clone();
        true
    }

    pub fn cut_input(&mut self) -> bool {
        if !self.copy_input() {
            return false;
        }

        self.clear_input();
        true
    }

    pub fn kill_to_line_end(&mut self) -> bool {
        if self.ui.input_all_selected {
            return self.cut_input();
        }

        let cursor = clamp_char_boundary(&self.ui.input, self.ui.input_cursor);
        if cursor >= self.ui.input.len() {
            return false;
        }

        let (_, line_end) = current_line_bounds(&self.ui.input, cursor);
        let kill_end = if cursor < line_end {
            line_end
        } else {
            next_char_boundary(&self.ui.input, cursor)
        };

        if kill_end <= cursor {
            return false;
        }

        self.ui.input_clipboard = self.ui.input[cursor..kill_end].to_string();
        self.ui.input.drain(cursor..kill_end);
        self.ui.input_cursor = cursor;
        self.ui.history_index = None;
        true
    }

    pub fn delete_previous_word(&mut self) -> bool {
        if self.ui.input_all_selected {
            return self.cut_input();
        }

        let cursor = clamp_char_boundary(&self.ui.input, self.ui.input_cursor);
        if cursor == 0 {
            return false;
        }

        let mut start = cursor;
        while start > 0 {
            let prev = previous_char_boundary(&self.ui.input, start);
            let ch = self.ui.input[prev..start].chars().next().unwrap_or(' ');
            if !ch.is_whitespace() {
                break;
            }
            start = prev;
        }
        while start > 0 {
            let prev = previous_char_boundary(&self.ui.input, start);
            let ch = self.ui.input[prev..start].chars().next().unwrap_or(' ');
            if ch.is_whitespace() {
                break;
            }
            start = prev;
        }

        if start == cursor {
            return false;
        }

        self.ui.input_clipboard = self.ui.input[start..cursor].to_string();
        self.ui.input.drain(start..cursor);
        self.ui.input_cursor = start;
        self.ui.history_index = None;
        true
    }

    pub fn paste_input(&mut self) -> bool {
        if self.ui.input_clipboard.is_empty() {
            return false;
        }

        if self.ui.input_all_selected {
            self.ui.input.clear();
            self.ui.input_cursor = 0;
        }

        let clipboard = self.ui.input_clipboard.clone();
        let cursor = clamp_char_boundary(&self.ui.input, self.ui.input_cursor);
        self.ui.input.insert_str(cursor, &clipboard);
        self.ui.input_cursor = cursor + clipboard.len();
        self.ui.input_all_selected = false;
        self.ui.history_index = None;
        true
    }

    pub fn clear_input(&mut self) -> bool {
        let changed = !self.ui.input.is_empty() || self.ui.input_all_selected;
        self.ui.input.clear();
        self.ui.input_cursor = 0;
        self.ui.input_all_selected = false;
        self.ui.history_index = None;
        changed
    }

    pub fn history_prev(&mut self) {
        if self.ui.history.is_empty() {
            return;
        }

        let idx = match self.ui.history_index {
            Some(i) if i > 0 => i - 1,
            Some(i) => i,
            None => self.ui.history.len() - 1,
        };

        self.ui.history_index = Some(idx);
        self.ui.input = self.ui.history[idx].clone();
        self.ui.input_cursor = self.ui.input.len();
        self.ui.input_all_selected = false;
    }

    pub fn history_next(&mut self) {
        match self.ui.history_index {
            Some(i) if i + 1 < self.ui.history.len() => {
                self.ui.history_index = Some(i + 1);
                self.ui.input = self.ui.history[i + 1].clone();
                self.ui.input_cursor = self.ui.input.len();
                self.ui.input_all_selected = false;
            }
            _ => {
                self.ui.history_index = None;
                self.ui.input.clear();
                self.ui.input_cursor = 0;
                self.ui.input_all_selected = false;
            }
        }
    }

    pub fn commit_input(&mut self) -> String {
        let raw = self.ui.input.clone();
        let trimmed = raw.trim();

        if !trimmed.is_empty() {
            self.ui.history.push(trimmed.to_string());
        }

        self.ui.input.clear();
        self.ui.input_cursor = 0;
        self.ui.input_all_selected = false;
        self.ui.history_index = None;
        self.ui.hint = None;
        self.ui.autocomplete = None;

        raw
    }

    pub fn clear_hint(&mut self) {
        self.ui.hint = None;
    }

    pub fn clear_autocomplete(&mut self) {
        self.ui.autocomplete = None;
    }

    pub fn stop_spinner(&mut self) {
        self.ui.active_spinner = None;
        self.ui.spinner_started_at = None;
    }
}

fn clamp_char_boundary(value: &str, index: usize) -> usize {
    let mut index = index.min(value.len());
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn previous_char_boundary(value: &str, index: usize) -> usize {
    let index = clamp_char_boundary(value, index);
    if index == 0 {
        return 0;
    }

    value[..index]
        .char_indices()
        .last()
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn next_char_boundary(value: &str, index: usize) -> usize {
    let index = clamp_char_boundary(value, index);
    if index >= value.len() {
        return value.len();
    }

    value[index..]
        .char_indices()
        .nth(1)
        .map(|(idx, _)| index + idx)
        .unwrap_or(value.len())
}

fn current_line_bounds(value: &str, cursor: usize) -> (usize, usize) {
    let cursor = clamp_char_boundary(value, cursor);
    let start = value[..cursor]
        .rfind('\n')
        .map(|idx| idx + '\n'.len_utf8())
        .unwrap_or(0);
    let end = value[cursor..]
        .find('\n')
        .map(|idx| cursor + idx)
        .unwrap_or(value.len());
    (start, end)
}

fn adjacent_line_bounds(
    value: &str,
    start: usize,
    end: usize,
    direction: i8,
) -> Option<(usize, usize)> {
    if direction < 0 {
        if start == 0 {
            return None;
        }

        let previous_end = start.saturating_sub('\n'.len_utf8());
        let previous_start = value[..previous_end]
            .rfind('\n')
            .map(|idx| idx + '\n'.len_utf8())
            .unwrap_or(0);
        Some((previous_start, previous_end))
    } else {
        if end >= value.len() {
            return None;
        }

        let next_start = end + '\n'.len_utf8();
        let next_end = value[next_start..]
            .find('\n')
            .map(|idx| next_start + idx)
            .unwrap_or(value.len());
        Some((next_start, next_end))
    }
}

fn byte_index_at_char_column(value: &str, start: usize, end: usize, column: usize) -> usize {
    for (current_column, (offset, _)) in value[start..end].char_indices().enumerate() {
        if current_column == column {
            return start + offset;
        }
    }
    end
}

pub struct VoiceState {
    pub visible: bool,
    pub enabled: bool,
    pub connected: bool,
    pub status: Option<String>,
    pub partial: Option<String>,
    pub last_final: Option<String>,
    pub buffer: String,
    pub last_activity: Option<Instant>,
    pub last_inserted: Option<String>,
    pub url: String,
    pub model: String,
}

impl Default for VoiceState {
    fn default() -> Self {
        Self {
            visible: false,
            enabled: false,
            connected: false,
            status: None,
            partial: None,
            last_final: None,
            buffer: String::new(),
            last_activity: None,
            last_inserted: None,
            url: String::new(),
            model: String::new(),
        }
    }
}

#[derive(Default)]
pub struct UsageStats {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

pub struct PendingPermission {
    pub tool_name: String,
    pub args_summary: String,
    pub reply_tx: Sender<bool>,
}

pub struct PendingUpdate {
    pub current_version: String,
    pub latest_version: String,
    pub asset_url: String,
    pub asset_name: String,
    pub checksum_url: String,
    pub installing: bool,
}

pub const MAX_CONVERSATION_TOKENS: usize = 90_000;

pub struct ConversationHistory {
    pub messages: Vec<Value>,
    pub token_estimate: usize,
}

impl ConversationHistory {
    pub fn new() -> Self {
        let messages = vec![json!({
            "role": "system",
            "content":
                "You are Osmogrep, an AI coding agent working inside this repository.\n\
                \n\
                This repository may include a machine-generated index stored as `.context/context.json` at the repository root.\n\
                This file, if present, describes the structure of the codebase: files, symbols, and call relationships.\n\
                \n\
                The index is a regular file and must be discovered and read using tools.\n\
                It may or may not exist.\n\
                \n\
                When working:\n\
                - Check for the presence of `.context/context.json` using tools when repository structure matters.\n\
                - If present, read it to understand the repository before acting.\n\
                - Use tools to inspect other files or make changes as needed.\n\
                - If `.context/context.json` is missing or insufficient, proceed normally and use tools freely.\n\
                - For non-trivial coding work, use a disciplined loop: investigate, plan, implement, verify, and review residual risk.\n\
                - Treat tests, builds, diffs, and command outputs as evidence. Do not claim the repo is green unless a relevant command actually passed.\n\
                \n\
                Philosophy:\n\
                This repository will outlive any single contributor.\n\
                Many people will read, use, and maintain this code over time.\n\
                Leave the codebase better than you found it.\n\
                Prefer clear structure, small deterministic functions, and reliable behavior.\n\
                Make changes that are easy to understand, review, and maintain."
        })];

        let mut this = Self {
            messages,
            token_estimate: 0,
        };
        this.recompute_estimate();
        this
    }

    pub fn clear(&mut self) {
        let system = self
            .messages
            .iter()
            .find(|m| m.get("role").and_then(Value::as_str) == Some("system"))
            .cloned()
            .unwrap_or_else(|| ConversationHistory::new().messages[0].clone());

        self.messages = vec![system];
        self.recompute_estimate();
    }

    pub fn trim_to_budget(&mut self, max_tokens: usize) {
        if self.token_estimate <= max_tokens {
            return;
        }

        let mut removed = Vec::new();
        while self.messages.len() > 2 && self.token_estimate > max_tokens {
            let msg = self.messages.remove(1);
            removed.push(msg);
            self.recompute_estimate();
        }

        if !removed.is_empty() {
            self.merge_summary(removed);
        }

        while self.messages.len() > 2 && self.token_estimate > max_tokens {
            self.messages.remove(1);
            self.recompute_estimate();
        }
    }

    pub fn set_messages(&mut self, messages: Vec<Value>) {
        self.messages = messages;
        self.recompute_estimate();
    }

    fn recompute_estimate(&mut self) {
        self.token_estimate = self.messages.iter().map(estimate_message_tokens).sum();
    }

    fn merge_summary(&mut self, removed: Vec<Value>) {
        let mut entries = Vec::new();
        for msg in removed {
            if let Some(entry) = summarize_message(&msg) {
                entries.push(entry);
            }
        }
        if entries.is_empty() {
            return;
        }

        let existing = self
            .messages
            .get(1)
            .and_then(|m| m.get("content").and_then(Value::as_str))
            .filter(|c| c.starts_with("[conversation-summary]"))
            .map(|s| s.to_string());

        let mut lines = Vec::new();
        if let Some(prev) = existing {
            for line in prev.lines().skip(1) {
                if !line.trim().is_empty() {
                    lines.push(line.to_string());
                }
            }
            self.messages.remove(1);
        }
        lines.extend(entries);
        if lines.len() > 36 {
            lines = lines[lines.len() - 36..].to_vec();
        }

        let summary = format!("[conversation-summary]\n{}", lines.join("\n"));

        self.messages.insert(
            1,
            json!({
                "role": "assistant",
                "content": summary,
            }),
        );
        self.recompute_estimate();
    }
}

fn estimate_message_tokens(msg: &Value) -> usize {
    let content = msg.get("content").and_then(Value::as_str).unwrap_or("");
    (content.len() / 4).max(1)
}

fn summarize_message(msg: &Value) -> Option<String> {
    let role = msg.get("role").and_then(Value::as_str)?;
    if role == "system" {
        return None;
    }

    let content = msg.get("content").and_then(Value::as_str)?;
    let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }

    let clipped = if flat.chars().count() > 180 {
        let mut s: String = flat.chars().take(180).collect();
        s.push_str("...");
        s
    } else {
        flat
    };

    let who = if role == "user" { "U" } else { "A" };
    Some(format!("- {}: {}", who, clipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn agent_state_with_input(input: &str) -> AgentState {
        AgentState {
            ui: UiState {
                input: input.to_string(),
                ..UiState::default()
            },
            logs: LogBuffer::new(),
            session_changes: Vec::new(),
            reviewed_change_count: 0,
            undo_stack: Vec::new(),
            usage: UsageStats::default(),
            steer: None,
            auto_eval: false,
            permission_profile: PermissionProfile::WorkspaceAuto,
            jobs: Vec::new(),
            job_queue: Vec::new(),
            next_job_id: 1,
            plan_items: Vec::new(),
            session_name: None,
            theme: UiTheme::default(),
            accent: UiAccent::default(),
            density: UiDensity::default(),
            plan_mode: false,
            started_at: Instant::now(),
            repo_root: PathBuf::from("."),
            voice: VoiceState::default(),
            conversation: ConversationHistory::new(),
        }
    }

    #[test]
    fn select_all_replaces_input_on_next_character() {
        let mut state = agent_state_with_input("large pasted prompt");

        assert!(state.select_all_input());
        assert!(state.ui.input_all_selected);

        state.push_char('x');

        assert_eq!(state.ui.input, "x");
        assert!(!state.ui.input_all_selected);
    }

    #[test]
    fn select_all_replaces_input_on_next_text_insert() {
        let mut state = agent_state_with_input("large pasted prompt");

        assert!(state.select_all_input());
        state.insert_text("first\nsecond");

        assert_eq!(state.ui.input, "first\nsecond");
        assert_eq!(state.ui.input_cursor, "first\nsecond".len());
        assert!(!state.ui.input_all_selected);
    }

    #[test]
    fn backspace_after_select_all_clears_entire_input() {
        let mut state = agent_state_with_input("first line\nsecond line");

        assert!(state.select_all_input());
        state.backspace();

        assert!(state.ui.input.is_empty());
        assert!(!state.ui.input_all_selected);
    }

    #[test]
    fn cut_and_paste_round_trips_internal_clipboard() {
        let mut state = agent_state_with_input("keep this");

        assert!(state.cut_input());
        assert_eq!(state.ui.input_clipboard, "keep this");
        assert!(state.ui.input.is_empty());

        assert!(state.paste_input());
        assert_eq!(state.ui.input, "keep this");
    }

    #[test]
    fn kill_to_line_end_and_yank_round_trip() {
        let mut state = agent_state_with_input("first line\nsecond line");
        state.ui.input_cursor = "first ".len();

        assert!(state.kill_to_line_end());
        assert_eq!(state.ui.input_clipboard, "line");
        assert_eq!(state.ui.input, "first \nsecond line");
        assert_eq!(state.ui.input_cursor, "first ".len());

        assert!(state.paste_input());
        assert_eq!(state.ui.input, "first line\nsecond line");
    }

    #[test]
    fn kill_to_line_end_removes_newline_at_line_end() {
        let mut state = agent_state_with_input("first\nsecond");
        state.ui.input_cursor = "first".len();

        assert!(state.kill_to_line_end());
        assert_eq!(state.ui.input_clipboard, "\n");
        assert_eq!(state.ui.input, "firstsecond");
    }

    #[test]
    fn delete_previous_word_is_utf8_safe() {
        let mut state = agent_state_with_input("alpha  界beta");
        state.ui.input_cursor = "alpha  界beta".len();

        assert!(state.delete_previous_word());

        assert_eq!(state.ui.input_clipboard, "界beta");
        assert_eq!(state.ui.input, "alpha  ");
        assert_eq!(state.ui.input_cursor, "alpha  ".len());
    }

    #[test]
    fn inserts_and_deletes_at_cursor_without_splitting_multibyte_chars() {
        let mut state = agent_state_with_input("a界c");
        state.ui.input_cursor = "a界".len();

        state.push_char('b');
        assert_eq!(state.ui.input, "a界bc");
        assert_eq!(state.ui.input_cursor, "a界b".len());

        state.move_cursor_left();
        state.backspace();
        assert_eq!(state.ui.input, "abc");
        assert_eq!(state.ui.input_cursor, "a".len());

        state.delete_forward();
        assert_eq!(state.ui.input, "ac");
        assert_eq!(state.ui.input_cursor, "a".len());
    }

    #[test]
    fn vertical_cursor_motion_preserves_character_column() {
        let mut state = agent_state_with_input("one\ntwo words\nthree");
        state.ui.input_cursor = "one\ntwo".len();

        assert!(state.move_cursor_down());
        assert_eq!(state.ui.input_cursor, "one\ntwo words\nthr".len());

        assert!(state.move_cursor_up());
        assert_eq!(state.ui.input_cursor, "one\ntwo".len());
    }

    #[test]
    fn parses_user_theme_controls() {
        assert_eq!(UiTheme::parse("dark"), Some(UiTheme::Dark));
        assert_eq!(UiTheme::parse("day"), Some(UiTheme::Light));
        assert_eq!(UiTheme::parse("sepia"), None);
    }

    #[test]
    fn parses_user_accent_controls() {
        assert_eq!(UiAccent::parse("orange"), Some(UiAccent::Orange));
        assert_eq!(UiAccent::parse("cyan"), Some(UiAccent::Blue));
        assert_eq!(UiAccent::parse("pink"), Some(UiAccent::Rose));
        assert_eq!(UiAccent::parse("mono"), None);
    }

    #[test]
    fn parses_user_density_controls() {
        assert_eq!(UiDensity::parse("compact"), Some(UiDensity::Compact));
        assert_eq!(UiDensity::parse("default"), Some(UiDensity::Standard));
        assert_eq!(UiDensity::parse("roomy"), Some(UiDensity::Spacious));
        assert_eq!(UiDensity::parse("huge"), None);
    }
}
