//! logger.rs

use crate::state::{AgentState, LogLevel};

const USER_TAG: &str = "USER|";
const TOOL_PREFIX: &str = "● ";
const CHILD_PREFIX: &str = "  └ ";

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
        LogLevel::Info,
        format!("{USER_TAG}{}", input.into()),
    );
}


fn format_tool_name(raw: &str) -> &str {
    match raw {
        "run_shell" | "shell" | "bash" => "Shell",
        "search" => "Search",
        "glob_files" => "Glob",
        "write_file" => "Write",
        "read_file" => "Read",
        "edit_file" => "Edit",
        other => other,
    }
}

pub fn log_tool_call(
    state: &mut AgentState,
    tool: impl AsRef<str>,
    command: impl Into<String>,
) {
    let tool_name = format_tool_name(tool.as_ref());
    let command = command.into();

    state.logs.push(
        LogLevel::Success,
        format!("{TOOL_PREFIX}({tool_name}) {command}"),
    );
}


pub fn log_tool_result(
    state: &mut AgentState,
    output: impl Into<String>,
) {
    let output = output.into();

    for line in output.lines().map(str::trim).filter(|l| !l.is_empty()) {
        state.logs.push(
            LogLevel::Info,
            format!("{CHILD_PREFIX}{line}"),
        );
    }
}


pub fn log_agent_output(
    state: &mut AgentState,
    text: &str,
) {
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            // preserve paragraph breaks
            state.logs.push(LogLevel::Info, String::new());
        } else {
            state.logs.push(LogLevel::Info, line.to_string());
        }
    }
}


pub fn parse_user_input_log(line: &str) -> Option<&str> {
    line.strip_prefix(USER_TAG)
}
