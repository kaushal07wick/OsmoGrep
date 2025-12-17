use std::time::Instant;
use crate::state::{AgentState, LogLevel, LogLine};

pub fn log(state: &mut AgentState, level: LogLevel, msg: impl Into<String>) {
    state.logs.push(LogLine {
        level,
        text: msg.into(),
        at: Instant::now(),
    });
}

pub fn log_diff_analysis(state: &mut AgentState) {
    state.logs.push(LogLine {
        level: LogLevel::Info,
        text: "__DIFF_ANALYSIS__".into(),
        at: Instant::now(),
    });
}
