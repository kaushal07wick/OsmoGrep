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

///events emitted by agent
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

//results from single test event
#[derive(Debug, Clone)]
pub enum TestResult {
    Passed,
    Failed { output: String },
}

///agent phase (lifecycle)
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

///source level change for a symbol (code)
#[derive(Debug, Clone)]
pub struct SymbolDelta {
    pub old_source: String,
    pub new_source: String,
}

///baseline when computing diffs
#[derive(Debug, Clone, Copy)]
pub enum DiffBaseline {
    BaseBranch, // base → HEAD / index
    Staged,     // HEAD → index
}

///classification of semantic surface
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

///test decision (if test should be generated or not)
#[derive(Debug, Clone)]
pub enum TestDecision {
    Yes,
    No,
    Conditional,
}

///estimated risk level of change
#[derive(Debug, Clone)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

///analysis of single changed file
#[derive(Debug, Clone)]
pub struct DiffAnalysis {
    pub file: String,
    pub symbol: Option<String>,
    pub surface: ChangeSurface,
    pub delta: Option<SymbolDelta>,
    pub summary: Option<SemanticSummary>,
}

///fully resolved semantic change ready for test synthesis
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

///intended purpose of a generated test
#[derive(Debug, Clone)]
pub enum TestIntent {
    Regression,
    NewBehavior,
    Guardrail,
}

/// UI focus area
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Input,
    Diff,
    Execution,
}

// UI panel view state
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

/// security level for log entries
#[derive(Clone, Debug)]
pub enum LogLevel {
    Info,
    Success,
    Warn,
    Error,
}

///timestamped log entries
#[derive(Clone, Debug)]
pub struct LogLine {
    pub level: LogLevel,
    pub text: String,
    pub at: Instant,
}
///fixed size rolling buffer for logs
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

/// mutable state tracking the agent lifecylce
pub struct LifecycleState {
    pub phase: Phase,
    pub base_branch: Option<String>,
    pub original_branch: Option<String>,
    pub current_branch: Option<String>,
    pub agent_branch: Option<String>,
    pub language: Option<Language>,
    pub framework: Option<TestFramework>,
}

/// execution context holding diff analysis and test artifacts
pub struct AgentContext {
    pub diff_analysis: Vec<DiffAnalysis>,
    pub test_candidates: Vec<TestCandidate>,

    pub last_generated_test: Option<String>,
    pub generated_tests_ready: bool,
    pub last_test_result: Option<TestResult>,
}

/// complete UI state.
pub struct UiState {
    pub input: String,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub hint: Option<String>,
    pub autocomplete: Option<String>,
    pub last_activity: Instant,
    pub selected_diff: Option<usize>,
    pub diff_scroll: usize,
    pub diff_scroll_x: usize,
    pub in_diff_view: bool,
    pub diff_side_by_side: bool,
    pub focus: Focus,
    pub exec_scroll: usize,
    pub active_spinner: Option<String>,
    pub spinner_started_at: Option<Instant>,
    pub spinner_elapsed: Option<Duration>,
    pub panel_view: Option<SinglePanelView>,
    pub panel_scroll: usize,
    pub panel_scroll_x: usize,
    pub cached_log_lines: Vec<ratatui::text::Line<'static>>,
    pub last_log_len: usize,
    pub dirty: bool,
    pub last_draw: Instant,
}

/// root container for the entire agent.
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
        self.logs.push(level, text);
    }

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
