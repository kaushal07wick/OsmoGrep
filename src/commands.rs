// src/commands.rs
//
// Slash-command dispatcher only.
// No git. No agent orchestration. No testgen.

use std::time::Instant;

use crate::logger::log;
use crate::state::{
    AgentState,
    InputMode,
    LogLevel,
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
    let input = state.ui.input.trim().to_string();

    state.clear_hint();
    state.clear_autocomplete();

    if input.starts_with('!') || !input.starts_with('/') {
        return;
    }

    if input == "/" {
        state.set_hint("Type /help to see available commands");
        return;
    }

    let commands: &[(&str, &str)] = &[
        ("/help", "Show available commands"),
        ("/clear", "Clear logs"),
        ("/key", "Set OpenAI API key"),
        ("/quit", "Stop agent execution"),
        ("/q", "Stop agent execution"),
        ("/exit", "Exit Osmogrep"),
    ];

    for (cmd, desc) in commands {
        if cmd.starts_with(&input) {
            state.set_autocomplete((*cmd).to_string());
            state.set_hint(*desc);
            return;
        }
    }

    for (cmd, desc) in commands {
        if input.starts_with(cmd) {
            state.set_hint(*desc);
            return;
        }
    }

    state.set_hint("Unknown command. Type /help");
}
