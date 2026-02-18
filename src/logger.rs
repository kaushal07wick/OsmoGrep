//! logger.rs

use crate::state::{AgentState, LogLevel};

const USER_TAG: &str = "USER|";
const TOOL_PREFIX: &str = "● ";
const CHILD_PREFIX: &str = "  └ ";
const STATUS_PREFIX: &str = "· ";

pub fn log(
    state: &mut AgentState,
    level: LogLevel,
    msg: impl Into<String>,
) {
    state.logs.push(level, msg.into());
}

pub fn log_status(
    state: &mut AgentState,
    msg: impl Into<String>,
) {
    state.logs.push(LogLevel::Info, format!("{STATUS_PREFIX}{}", msg.into()));
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
        "run_tests" => "Test",
        "list_dir" => "ListDir",
        "git_diff" => "GitDiff",
        "git_log" => "GitLog",
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

pub fn update_streaming_log(state: &mut AgentState) {
    let total_lines = state.ui.streaming_buffer.matches('\n').count();
    if total_lines <= state.ui.streaming_lines_logged {
        return;
    }

    let lines: Vec<&str> = state.ui.streaming_buffer.split('\n').collect();
    for line in lines
        .iter()
        .take(total_lines)
        .skip(state.ui.streaming_lines_logged)
    {
        state.logs.push(LogLevel::Info, (*line).to_string());
    }
    state.ui.streaming_lines_logged = total_lines;
}

pub fn flush_streaming_log(state: &mut AgentState) {
    let tail = state
        .ui
        .streaming_buffer
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .trim_end();

    if !tail.is_empty() {
        state.logs.push(LogLevel::Info, tail.to_string());
    }

    state.ui.streaming_buffer.clear();
    state.ui.streaming_lines_logged = 0;
}
