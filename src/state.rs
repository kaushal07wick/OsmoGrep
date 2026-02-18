use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Instant;
use serde_json::{json, Value};

pub const MAX_LOGS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    AgentText, 
    Shell,     
    Command,   
    ApiKey,   
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

    pub fn len(&self) -> usize {
        self.logs.len()
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

#[derive(Debug, Clone)]
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
    pub cancel_requested: bool,
    pub auto_approve: bool,
    pub pending_permission: Option<PendingPermission>,
    pub streaming_buffer: String,
    pub streaming_active: bool,
    pub streaming_lines_logged: usize,
}

pub struct AgentState {
    pub ui: UiState,
    pub logs: LogBuffer,

    pub started_at: Instant,
    pub repo_root: PathBuf,
    pub voice: VoiceState,
    pub conversation: ConversationHistory,
}

impl AgentState {
    pub fn push_char(&mut self, c: char) {
        self.ui.input.push(c);
        self.ui.history_index = None;
    }

    pub fn backspace(&mut self) {
        self.ui.input.pop();
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
    }

    pub fn history_next(&mut self) {
        match self.ui.history_index {
            Some(i) if i + 1 < self.ui.history.len() => {
                self.ui.history_index = Some(i + 1);
                self.ui.input = self.ui.history[i + 1].clone();
            }
            _ => {
                self.ui.history_index = None;
                self.ui.input.clear();
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
        self.ui.history_index = None;
        self.ui.hint = None;
        self.ui.autocomplete = None;

        raw
    }

    pub fn set_hint(&mut self, hint: impl Into<String>) {
        self.ui.hint = Some(hint.into());
    }

    pub fn clear_hint(&mut self) {
        self.ui.hint = None;
    }

    pub fn set_autocomplete(&mut self, text: impl Into<String>) {
        self.ui.autocomplete = Some(text.into());
    }

    pub fn clear_autocomplete(&mut self) {
        self.ui.autocomplete = None;
    }

    pub fn push_log(&mut self, level: LogLevel, text: impl Into<String>) {
        crate::logger::log(self, level, text);
    }

    pub fn start_spinner(&mut self, text: impl Into<String>) {
        self.ui.active_spinner = Some(text.into());
        self.ui.spinner_started_at = Some(Instant::now());
    }

    pub fn stop_spinner(&mut self) {
        self.ui.active_spinner = None;
        self.ui.spinner_started_at = None;
    }
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

pub struct PendingPermission {
    pub tool_name: String,
    pub args_summary: String,
    pub reply_tx: Sender<bool>,
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
                This repository may include a machine-generated index stored as a file named `.context.json` at the repository root.\n\
                This file, if present, describes the structure of the codebase: files, symbols, and call relationships.\n\
                \n\
                The index is a regular file and must be discovered and read using tools.\n\
                It may or may not exist.\n\
                \n\
                When working:\n\
                - Check for the presence of `.context.json` using tools when repository structure matters.\n\
                - If present, read it to understand the repository before acting.\n\
                - Use tools to inspect other files or make changes as needed.\n\
                - If `.context.json` is missing or insufficient, proceed normally and use tools freely.\n\
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
        self.token_estimate = self
            .messages
            .iter()
            .map(estimate_message_tokens)
            .sum();
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

        let summary = format!(
            "[conversation-summary]\n{}",
            lines.join("\n")
        );

        self.messages.insert(1, json!({
            "role": "assistant",
            "content": summary,
        }));
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
    let flat = content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
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
