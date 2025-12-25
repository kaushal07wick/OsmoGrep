//! logger.rs
//!
//! Centralized logging API for Osmogrep.
//!
//! Responsibilities:
//! - Append structured log entries to AgentState
//! - Emit structured markers for UI rendering
//! - Mark UI as dirty on log changes

use crate::state::{AgentState, LogLevel};

/// Delegates buffering to the log buffer owned by AgentState.
pub fn log(
    state: &mut AgentState,
    level: LogLevel,
    msg: impl Into<String>,
) {
    state.logs.push(level, msg.into());
    state.ui.dirty = true;
}

/// Emits a structured marker indicating diff analysis output.
pub fn log_diff_analysis(state: &mut AgentState) {
    state.logs.push(LogLevel::Info, LogMarker::DiffAnalysis.as_str());
    state.ui.dirty = true;
}

enum LogMarker {
    DiffAnalysis,
}

impl LogMarker {
    fn as_str(&self) -> &'static str {
        match self {
            LogMarker::DiffAnalysis => "__DIFF_ANALYSIS__",
        }
    }
}
