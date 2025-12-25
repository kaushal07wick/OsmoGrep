//! logger.rs
//!
//! Centralized logging API for Osmogrep.

use crate::state::{AgentState, LogLevel};

pub fn log(
    state: &mut AgentState,
    level: LogLevel,
    msg: impl Into<String>,
) {
    state.logs.push(level, msg.into());
    state.ui.dirty = true;
}

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
