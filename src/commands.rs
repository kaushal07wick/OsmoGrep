// src/commands.rs

use std::time::Instant;

use crate::logger::log;
use crate::state::{
    AgentState, CommandItem, InputMode, LogLevel
};
use crate::voice::VoiceCommand;
use std::sync::mpsc::Sender;

pub fn handle_command(
    state: &mut AgentState,
    raw: &str,
    voice_tx: Option<&Sender<VoiceCommand>>,
) {
    state.ui.last_activity = Instant::now();
    state.clear_hint();
    state.clear_autocomplete();

    let cmd = raw.trim();

    match cmd {
        "/help" => help(state),
        "/clear" => clear_logs(state),
        "/voice" => voice_status(state),
        "/voice on" => voice_on(state, voice_tx),
        "/voice off" => voice_off(state, voice_tx),

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
    log(state, Info, "  /voice       Show voice status");
    log(state, Info, "  /voice on    Start voice input");
    log(state, Info, "  /voice off   Stop voice input");
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

fn voice_status(state: &mut AgentState) {
    let status = state
        .voice
        .status
        .clone()
        .unwrap_or_else(|| "idle".into());

    log(
        state,
        LogLevel::Info,
        format!(
            "Voice: {} (connected: {}, url: {}, model: {})",
            if state.voice.enabled { "on" } else { "off" },
            state.voice.connected,
            state.voice.url,
            state.voice.model
        ),
    );
    log(state, LogLevel::Info, status);
}

fn voice_on(state: &mut AgentState, voice_tx: Option<&Sender<VoiceCommand>>) {
    if state.voice.enabled {
        log(state, LogLevel::Info, "Voice already enabled.");
        return;
    }

    if let Some(tx) = voice_tx {
        let _ = tx.send(VoiceCommand::Start {
            url: state.voice.url.clone(),
            model: state.voice.model.clone(),
        });
        state.voice.enabled = true;
        log(state, LogLevel::Info, "Starting voice input...");
    } else {
        log(state, LogLevel::Error, "Voice channel unavailable.");
    }
}

fn voice_off(state: &mut AgentState, voice_tx: Option<&Sender<VoiceCommand>>) {
    if !state.voice.enabled {
        log(state, LogLevel::Info, "Voice already disabled.");
        return;
    }

    if let Some(tx) = voice_tx {
        let _ = tx.send(VoiceCommand::Stop);
        state.voice.enabled = false;
        log(state, LogLevel::Info, "Stopping voice input...");
    } else {
        log(state, LogLevel::Error, "Voice channel unavailable.");
    }
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
        CommandItem { cmd: "/voice", desc: "Show voice status" },
        CommandItem { cmd: "/voice on", desc: "Start voice input" },
        CommandItem { cmd: "/voice off", desc: "Stop voice input" },
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
