use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Sender, Receiver};
use std::time::{Instant, Duration};

use crate::context::IndexHandle;
use crate::detectors::{framework::TestFramework, language::Language};
use crate::testgen::candidate::TestCandidate;
use crate::testgen::summarizer::SemanticSummary;

pub const MAX_LOGS: usize = 1000;

/* ============================================================
   Agent Events (Async-safe)
   ============================================================ */

#[derive(Debug)]
pub enum AgentEvent {
    Log(LogLevel, String),

    SpinnerStart(String),
    SpinnerStop,

    GeneratedTest(String),

    TestStarted,
    TestFinished(TestResult),

    Finished,
    Failed(String),
}


#[derive(Debug, Clone)]
pub enum TestResult {
    Passed,
    Failed { output: String },
}


/* ============================================================
   Agent Lifecycle
   ============================================================ */

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Init,
    DetectBase,
    Idle,
    ExecuteAgent,
    Running,
    CreateNewAgent,
    Rollback,
    Done,
}

/* ============================================================
   Diff + Symbol Delta Model (FACTS ONLY)
   ============================================================ */

/// Raw before/after source for a symbol or file.
/// No semantics. No interpretation.
#[derive(Debug, Clone)]
pub struct SymbolDelta {
    pub old_source: String,
    pub new_source: String,
}

#[derive(Debug, Clone, Copy)]
pub enum DiffBaseline {
    BaseBranch, // base → HEAD / index
    Staged,     // HEAD → index
}


/* ============================================================
   Diff Analysis (STATIC FACTS)
   ============================================================ */

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

/// Output of static diff analysis.
/// Contains NO interpretation beyond surface classification.
#[derive(Debug, Clone)]
pub struct DiffAnalysis {
    pub file: String,
    pub symbol: Option<String>,
    pub surface: ChangeSurface,

    /// Raw extracted delta (before/after).
    /// None means no meaningful code delta.
    pub delta: Option<SymbolDelta>,

    /// Lightweight semantic summary (UI + prompt hints).
    /// Populated AFTER diff analysis.
    pub summary: Option<SemanticSummary>,
}



/* ============================================================
   Semantic Change (AGENT-LOCAL, LLM-FACING)
   ============================================================ */

/// Heavy semantic object built ONLY during agent execution.
/// NEVER stored in global state.
#[derive(Debug, Clone)]
pub struct SemanticChange {
    pub file: String,
    pub symbol: String,
    pub language: Language,

    pub before: String,
    pub after: String,

    pub summary: SemanticSummary,

    pub risk: RiskLevel,
    pub test_intent: TestIntent,
    pub failure_mode: String,
}

#[derive(Debug, Clone)]
pub enum TestIntent {
    Regression,
    NewBehavior,
    Guardrail,
}

/* ============================================================
   UI Focus + Panels
   ============================================================ */

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Input,
    Diff,
    Execution,
}

#[derive(Clone, Debug)]
pub enum SinglePanelView {
    TestGenPreview {
        candidate: TestCandidate,
        generated_test: Option<String>,
    },
    TestResult {
        output: String,
        passed: bool,
    },
}

/* ============================================================
   Logging
   ============================================================ */

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

/* ============================================================
   Lifecycle State (AGENT REALITY)
   ============================================================ */

pub struct LifecycleState {
    pub phase: Phase,

    pub base_branch: Option<String>,
    pub original_branch: Option<String>,
    pub current_branch: Option<String>,
    pub agent_branch: Option<String>,

    pub language: Option<Language>,
    pub framework: Option<TestFramework>,
}

/* ============================================================
   Agent Context (THINKING / MEMORY)
   ============================================================ */

pub struct AgentContext {
    pub diff_analysis: Vec<DiffAnalysis>,
    pub test_candidates: Vec<TestCandidate>,

    pub last_generated_test: Option<String>,
    pub generated_tests_ready: bool,
    pub last_test_result: Option<TestResult>,
}


/* ============================================================
   UI State (PURE PRESENTATION)
   ============================================================ */

pub struct UiState {
    /* input */
    pub input: String,

    /* command UX */
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub hint: Option<String>,
    pub autocomplete: Option<String>,

    /* activity */
    pub last_activity: Instant,

    /* diff viewer */
    pub selected_diff: Option<usize>,
    pub diff_scroll: usize,
    pub diff_scroll_x: usize,
    pub in_diff_view: bool,
    pub diff_side_by_side: bool,
    pub focus: Focus,

    /* execution */
    pub exec_scroll: usize,

    /* spinner */
    pub active_spinner: Option<String>,
    pub spinner_started_at: Option<Instant>,
    pub spinner_elapsed: Option<Duration>,

    /* panel */
    pub panel_view: Option<SinglePanelView>,
    pub panel_scroll: usize,
    pub panel_scroll_x: usize,

    /* render cache */
    pub cached_log_lines: Vec<ratatui::text::Line<'static>>,
    pub last_log_len: usize,
    pub dirty: bool,
    pub last_draw: Instant,
}

/* ============================================================
   Root Agent State
   ============================================================ */

pub struct AgentState {
    pub lifecycle: LifecycleState,
    pub context: AgentContext,
    pub ui: UiState,
    pub logs: LogBuffer,

    pub agent_tx: Sender<AgentEvent>,
    pub agent_rx: Receiver<AgentEvent>,

    pub cancel_requested: Arc<AtomicBool>,
    pub context_index: Option<IndexHandle>,
}


/* ============================================================
   Shared Helpers
   ============================================================ */

impl AgentState {
    /* ---------- input ---------- */

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
        let cmd = self.ui.input.trim().to_string();

        if !cmd.is_empty() {
            self.ui.history.push(cmd.clone());
        }

        self.ui.input.clear();
        self.ui.history_index = None;
        self.ui.hint = None;
        self.ui.autocomplete = None;

        cmd
    }

    /* ---------- ui hints ---------- */

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

    /* ---------- logging ---------- */

    pub fn push_log(&mut self, level: LogLevel, text: impl Into<String>) {
        self.logs.push(level, text);
    }

    /* ---------- spinner ---------- */

    pub fn start_spinner(&mut self, text: impl Into<String>) {
        let now = Instant::now();
        self.ui.active_spinner = Some(text.into());
        self.ui.spinner_started_at = Some(now);
        self.ui.spinner_elapsed = Some(Duration::from_secs(0));
    }

    pub fn update_spinner(&mut self) {
        if let Some(start) = self.ui.spinner_started_at {
            self.ui.spinner_elapsed = Some(start.elapsed());
        }
    }

    pub fn stop_spinner(&mut self) {
        self.ui.active_spinner = None;
        self.ui.spinner_started_at = None;
        self.ui.spinner_elapsed = None;
    }
}
