// src/logger.rs
use crate::state::{AgentState, LogLevel, LogLine};

pub fn log(state: &mut AgentState, level: LogLevel, msg: impl Into<String>) {
    state.logs.push(LogLine {
        level,
        text: msg.into(),
    });
}
