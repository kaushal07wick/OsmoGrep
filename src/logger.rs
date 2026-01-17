//! logger.rs

use crate::state::{AgentState, LogLevel};

const USER_TAG: &str = "USER|";
const TOOL_PREFIX: &str = "● ";
const CHILD_PREFIX: &str = "└ ";

pub fn log(
    state: &mut AgentState,
    level: LogLevel,
    msg: impl Into<String>,
) {
    state.logs.push(level, msg.into());
}

pub fn log_user_input(
    state: &mut AgentState,
    input: impl Into<String>,
) {
    state.logs.push(
        LogLevel::Info, // ← white in UI
        format!("{USER_TAG}{}", input.into()),
    );
}


pub fn log_tool_call(
    state: &mut AgentState,
    tool: impl Into<String>,
    args: impl Into<String>,
) {
    state.logs.push(
        LogLevel::Success, 
        format!(
            "{TOOL_PREFIX}TOOL|{}|{}",
            tool.into(),
            args.into()
        ),
    );
}


pub fn log_tool_result(
    state: &mut AgentState,
    msg: impl Into<String>,
) {
    state.logs.push(
        LogLevel::Info,
        format!("{CHILD_PREFIX}{}", msg.into()),
    );
}

pub fn log_agent_output(
    state: &mut AgentState,
    text: &str,
) {
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        state.logs.push(
            LogLevel::Info,
            format!("{CHILD_PREFIX}{line}"),
        );
    }
}


pub fn parse_user_input_log(line: &str) -> Option<&str> {
    line.strip_prefix(USER_TAG)
}
