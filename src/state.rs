use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Instant};

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
}

pub struct AgentState {
    pub ui: UiState,
    pub logs: LogBuffer,

    pub started_at: Instant,
    pub repo_root: PathBuf,
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
