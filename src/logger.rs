use std::time::Instant;

use crate::state::{AgentState, LogLevel, LogLine, MAX_LOGS};

pub fn log(state: &mut AgentState, level: LogLevel, msg: impl Into<String>) {
    if state.logs.len() >= MAX_LOGS {
        state.logs.pop_front();
    }

    state.logs.push_back(LogLine {
        level,
        text: msg.into(),
        at: Instant::now(),
    });
}

pub fn log_diff_analysis(state: &mut AgentState) {
    if state.logs.len() >= MAX_LOGS {
        state.logs.pop_front();
    }

    state.logs.push_back(LogLine {
        level: LogLevel::Info,
        text: "__DIFF_ANALYSIS__".into(),
        at: Instant::now(),
    });
}
