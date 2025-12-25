//! logger.rs
//!
//! Centralized logging API for Osmogrep.

use crate::state::{AgentState, LogLevel};

pub fn log(
    state: &mut AgentState,
    level: LogLevel,
    msg: impl Into<String>,
) {
    let at_bottom = state.ui.auto_scroll;

    state.logs.push(level, msg.into());

    if at_bottom {
        // follow logs
        state.ui.exec_scroll = usize::MAX;
    }

    state.ui.dirty = true;
}


pub fn log_diff_analysis(state: &mut AgentState) {
    let at_bottom = state.ui.auto_scroll;

    state.logs.push(LogLevel::Info, LogMarker::DiffAnalysis.as_str());

    if at_bottom {
        state.ui.exec_scroll = usize::MAX;
    }

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
