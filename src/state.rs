use crate::detectors::{framework::TestFramework, language::Language};

#[derive(Clone, Debug)]
pub enum Phase {
    Init,
    DetectBase,
    Idle,
    ExecuteAgent,
    CreateNewAgent,
    Rollback,
    Done,
}

/*---------- symbol delta----------- */
#[derive(Debug, Clone)]
pub struct SymbolDelta {
    pub file: String,
    pub symbol: String,
    pub old_source: String,
    pub new_source: String,
}


/* ---------- logging ---------- */

#[derive(Clone, Debug)]
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
}

/* ---------- diff analysis ---------- */

#[derive(Debug, Clone)]
pub enum ChangeSurface {
    PureLogic,
    Branching,
    Contract,
    ErrorPath,
    State,
    Integration,
    Observability,
    Cosmetic,
}

#[derive(Debug, Clone)]
pub enum TestDecision {
    Yes,
    No,
    Conditional,
}

#[derive(Debug, Clone)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub struct DiffAnalysis {
    pub file: String,
    pub symbol: Option<String>,
    pub surface: ChangeSurface,
    pub test_required: TestDecision,
    pub risk: RiskLevel,
    pub reason: String,
    pub delta: Option<SymbolDelta>,
    pub pretty: Option<String>,
}

/* ---------- agent state ---------- */

pub struct AgentState {
    /* lifecycle */
    pub phase: Phase,
    pub base_branch: Option<String>,
    pub original_branch: Option<String>,
    pub agent_branch: Option<String>,

    /* input */
    pub input: String,
    pub input_focused: bool,

    /* command UX */
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub hint: Option<String>,
    pub autocomplete: Option<String>,

    /* logs */
    pub logs: Vec<LogLine>,

    /* analysis */
    pub diff_analysis: Vec<DiffAnalysis>,

    /* detection (STATUS only) */
    pub language: Option<Language>,
    pub framework: Option<TestFramework>,

    /* ui */
    pub spinner_tick: usize,
}

/* ---------- helpers ---------- */

impl AgentState {
    pub fn push_char(&mut self, c: char) {
        self.input.push(c);
        self.history_index = None;
    }

    pub fn backspace(&mut self) {
        self.input.pop();
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }

        let idx = match self.history_index {
            Some(i) if i > 0 => i - 1,
            Some(i) => i,
            None => self.history.len() - 1,
        };

        self.history_index = Some(idx);
        self.input = self.history[idx].clone();
    }

    pub fn history_next(&mut self) {
        match self.history_index {
            Some(i) if i + 1 < self.history.len() => {
                self.history_index = Some(i + 1);
                self.input = self.history[i + 1].clone();
            }
            _ => {
                self.history_index = None;
                self.input.clear();
            }
        }
    }

    pub fn commit_input(&mut self) -> String {
        let cmd = self.input.trim().to_string();

        if !cmd.is_empty() {
            self.history.push(cmd.clone());
        }

        self.input.clear();
        self.history_index = None;
        self.hint = None;
        self.autocomplete = None;

        cmd
    }

    pub fn set_hint(&mut self, hint: impl Into<String>) {
        self.hint = Some(hint.into());
    }

    pub fn clear_hint(&mut self) {
        self.hint = None;
    }

    pub fn set_autocomplete(&mut self, text: impl Into<String>) {
        self.autocomplete = Some(text.into());
    }

    pub fn clear_autocomplete(&mut self) {
        self.autocomplete = None;
    }
}
