// src/commands.rs

use std::time::Instant;

use crate::logger::log;
use crate::state::{
    AgentState, CommandItem, InputMode, LogLevel
};

pub fn handle_command(state: &mut AgentState, raw: &str) {
    state.ui.last_activity = Instant::now();
    state.clear_hint();
    state.clear_autocomplete();

    let cmd = raw.trim();

    match cmd {
        "/help" => help(state),
        "/clear" => clear_logs(state),

        "/exit" => exit_app(state),
        "/quit" | "/q" => quit_agent(state),

        "/key" => enter_api_key_mode(state),

        "" => {}

        _ => {
            log(
                state,
                LogLevel::Warn,
                "Unknown command. Type /help",
            );
        }
    }
}


fn help(state: &mut AgentState) {
    use LogLevel::Info;

    log(state, Info, "Available commands:");
    log(state, Info, "  /help        Show this help");
    log(state, Info, "  /clear       Clear logs");
    log(state, Info, "  /key         Set OpenAI API key");
    log(state, Info, "  /quit | /q   Stop agent execution");
    log(state, Info, "  /exit        Exit Osmogrep");
    log(state, Info, "");
    log(state, Info, "Anything else is sent to the agent.");
    log(state, Info, "!<cmd> runs a shell command directly.");
}

fn clear_logs(state: &mut AgentState) {
    state.logs.clear();
    state.ui.exec_scroll = usize::MAX;

    log(state, LogLevel::Info, "Logs cleared.");
}

fn exit_app(state: &mut AgentState) {
    log(state, LogLevel::Info, "Exiting Osmogrep.");
    state.ui.should_exit = true;
}

fn quit_agent(state: &mut AgentState) {
    log(state, LogLevel::Info, "Stopping agent execution.");
    state.stop_spinner();
    state.ui.exec_scroll = usize::MAX;
}

fn enter_api_key_mode(state: &mut AgentState) {
    state.ui.input.clear();
    state.ui.input_mode = InputMode::ApiKey;
    state.ui.input_masked = true;
    state.ui.input_placeholder = Some("Enter OpenAI API key".into());

    log(
        state,
        LogLevel::Info,
        "Enter your OpenAI API key and press Enter.",
    );
}
pub fn update_command_hints(state: &mut AgentState) {
    let input = state.ui.input.trim();

    let prev_selected = state.ui.command_selected;

    state.ui.command_items.clear();

    if !input.starts_with('/') {
        state.ui.command_selected = 0;
        return;
    }

    let all: &[CommandItem] = &[
        CommandItem { cmd: "/help", desc: "Show available commands" },
        CommandItem { cmd: "/clear", desc: "Clear logs" },
        CommandItem { cmd: "/key", desc: "Set OpenAI API key" },
        CommandItem { cmd: "/quit", desc: "Stop agent execution" },
        CommandItem { cmd: "/q", desc: "Stop agent execution" },
        CommandItem { cmd: "/exit", desc: "Exit Osmogrep" },
    ];

    for item in all {
        if item.cmd.starts_with(input) {
            state.ui.command_items.push(*item);
        }
    }

    if state.ui.command_items.is_empty() {
        state.ui.command_selected = 0;
    } else {
        state.ui.command_selected =
            prev_selected.min(state.ui.command_items.len() - 1);
    }
}
