// src/commands.rs

use std::fs;
use std::time::Instant;

use crate::agent::Agent;
use crate::logger::log;
use crate::persistence;
use crate::test_harness::run_tests;
use crate::state::{
    AgentState, CommandItem, InputMode, LogLevel
};
use crate::voice::VoiceCommand;
use std::sync::mpsc::Sender;

pub fn handle_command(
    state: &mut AgentState,
    raw: &str,
    voice_tx: Option<&Sender<VoiceCommand>>,
    agent: Option<&mut Agent>,
) {
    state.ui.last_activity = Instant::now();
    state.clear_hint();
    state.clear_autocomplete();

    let cmd_raw = raw.trim();
    let cmd = normalize_command_prefix(cmd_raw);

    if cmd.starts_with("/model ") {
        set_model(state, &cmd, agent);
        return;
    }
    if cmd.starts_with("/test ") {
        run_test(state, &cmd);
        return;
    }

    match cmd.as_str() {
        "/help" => help(state),
        "/clear" => clear_logs(state),
        "/voice" => voice_status(state),
        "/voice on" => voice_on(state, voice_tx),
        "/voice off" => voice_off(state, voice_tx),

        "/exit" => exit_app(state),
        "/quit" | "/q" => quit_agent(state),

        "/key" => enter_api_key_mode(state),
        "/new" => new_conversation(state),
        "/approve" => toggle_auto_approve(state),
        "/model" => show_model(state, agent),
        "/test" => run_test(state, &cmd),
        "/undo" => undo_last_change(state),
        "/diff" => show_session_diff(state),

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
    log(state, Info, "  /model       Show active provider/model");
    log(state, Info, "  /model <provider> <model> [base_url]");
    log(state, Info, "  /test        Run auto-detected tests");
    log(state, Info, "  /test <arg>  Run targeted tests (framework-specific)");
    log(state, Info, "  /undo        Revert the last agent file change");
    log(state, Info, "  /diff        Show all file changes this session");
    log(state, Info, "  /approve     Toggle dangerous tool auto-approve");
    log(state, Info, "  /new         Start a fresh conversation");
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
    state.ui.cancel_requested = true;
    state.stop_spinner();
    state.ui.exec_scroll = usize::MAX;
}

fn new_conversation(state: &mut AgentState) {
    state.conversation.clear();
    log(state, LogLevel::Info, "Started a new conversation.");
    let _ = persistence::save(state);
}

fn toggle_auto_approve(state: &mut AgentState) {
    state.ui.auto_approve = !state.ui.auto_approve;
    log(
        state,
        LogLevel::Info,
        format!(
            "Dangerous tool auto-approve: {}",
            if state.ui.auto_approve { "on" } else { "off" }
        ),
    );
}

fn show_model(state: &mut AgentState, agent: Option<&mut Agent>) {
    let Some(agent) = agent else {
        log(state, LogLevel::Warn, "Agent unavailable.");
        return;
    };

    let cfg = agent.model_config();
    log(
        state,
        LogLevel::Info,
        format!(
            "Model: provider={} model={} base_url={}",
            cfg.provider,
            cfg.model,
            cfg.base_url.clone().unwrap_or_else(|| "(default)".to_string())
        ),
    );
}

fn set_model(state: &mut AgentState, cmd: &str, agent: Option<&mut Agent>) {
    let Some(agent) = agent else {
        log(state, LogLevel::Warn, "Agent unavailable.");
        return;
    };

    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.len() < 3 {
        log(
            state,
            LogLevel::Warn,
            "Usage: /model <provider> <model> [base_url]",
        );
        return;
    }

    let provider = parts[1].to_string();
    let model = parts[2].to_string();
    let base_url = if parts.len() >= 4 {
        Some(parts[3].to_string())
    } else {
        None
    };

    agent.set_model_config(provider.clone(), model.clone(), base_url.clone());
    log(
        state,
        LogLevel::Success,
        format!(
            "Switched model: provider={} model={} base_url={}",
            provider,
            model,
            base_url.unwrap_or_else(|| "(default)".to_string())
        ),
    );
    let _ = persistence::save(state);
}

fn run_test(state: &mut AgentState, cmd: &str) {
    let target = cmd.strip_prefix("/test").map(str::trim).unwrap_or("");
    let target = if target.is_empty() { None } else { Some(target) };

    log(
        state,
        LogLevel::Info,
        format!("Running tests{}", target.map(|t| format!(" ({})", t)).unwrap_or_default()),
    );

    match run_tests(&state.repo_root, target) {
        Ok(run) => {
            log(
                state,
                if run.success { LogLevel::Success } else { LogLevel::Error },
                format!(
                    "Test run [{}] exit={} passed={} failed={} duration={}ms",
                    run.framework, run.exit_code, run.passed, run.failed, run.duration_ms
                ),
            );
            for line in run
                .output
                .lines()
                .rev()
                .take(30)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                log(state, LogLevel::Info, line.to_string());
            }
        }
        Err(e) => {
            log(state, LogLevel::Error, format!("Test run failed: {}", e));
        }
    }
}

fn undo_last_change(state: &mut AgentState) {
    let Some(last) = state.undo_stack.pop() else {
        log(state, LogLevel::Warn, "Nothing to undo.");
        return;
    };

    if let Err(e) = fs::write(&last.target, &last.before) {
        log(
            state,
            LogLevel::Error,
            format!("Undo failed for {}: {}", last.target, e),
        );
        return;
    }

    if let Some(pos) = state
        .session_changes
        .iter()
        .rposition(|s| s.target == last.target && s.after == last.after)
    {
        state.session_changes.remove(pos);
    }

    state.ui.diff_active = true;
    state.ui.diff_snapshot = vec![last.clone()];
    log(
        state,
        LogLevel::Success,
        format!("Undid last change on {}", last.target),
    );
    let _ = persistence::save(state);
}

fn show_session_diff(state: &mut AgentState) {
    if state.session_changes.is_empty() {
        log(state, LogLevel::Info, "No session changes to show.");
        return;
    }

    state.ui.diff_active = true;
    state.ui.diff_snapshot = state.session_changes.clone();
    log(
        state,
        LogLevel::Info,
        format!("Showing {} session change(s).", state.ui.diff_snapshot.len()),
    );
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
    state.voice.visible = true;
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
    state.voice.visible = true;
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
    state.voice.visible = true;
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
    let input = normalize_command_prefix(state.ui.input.trim());

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
        CommandItem { cmd: "/model", desc: "Show active provider/model" },
        CommandItem { cmd: "/test", desc: "Run auto-detected tests" },
        CommandItem { cmd: "/undo", desc: "Revert the last agent file change" },
        CommandItem { cmd: "/diff", desc: "Show all file changes this session" },
        CommandItem { cmd: "/approve", desc: "Toggle dangerous tool auto-approve" },
        CommandItem { cmd: "/new", desc: "Start a fresh conversation" },
        CommandItem { cmd: "/quit", desc: "Stop agent execution" },
        CommandItem { cmd: "/q", desc: "Stop agent execution" },
        CommandItem { cmd: "/exit", desc: "Exit Osmogrep" },
    ];

    for item in all {
        if input == "/" || item.cmd.starts_with(&input) {
            state.ui.command_items.push(*item);
        }
    }

    if state.ui.command_items.is_empty() {
        for item in all {
            if item.cmd.contains(&input)
                || item
                    .desc
                    .to_ascii_lowercase()
                    .contains(&input[1..].to_ascii_lowercase())
            {
                state.ui.command_items.push(*item);
            }
        }
    }

    if state.ui.command_items.is_empty() {
        state.ui.command_selected = 0;
    } else {
        state.ui.command_selected =
            prev_selected.min(state.ui.command_items.len() - 1);
    }
}

fn normalize_command_prefix(input: &str) -> String {
    if let Some(rest) = input.strip_prefix('／') {
        format!("/{}", rest)
    } else if let Some(rest) = input.strip_prefix('÷') {
        format!("/{}", rest)
    } else {
        input.to_string()
    }
}
