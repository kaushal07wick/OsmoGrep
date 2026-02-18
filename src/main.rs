mod agent;
mod commands;
mod context;
mod hooks;
mod logger;
mod mcp;
mod persistence;
mod state;
mod test_harness;
mod tools;
mod triage;
mod ui;
mod voice;

use std::{
    error::Error,
    io,
    sync::mpsc,
    time::{Duration, Instant},
};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::{
    event, execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};

use clap::{Parser, Subcommand};

use crate::{
    agent::{Agent, AgentEvent, CancelToken},
    context::ContextEvent,
    logger::{
        flush_streaming_log, log, log_agent_output, log_status, log_tool_call, log_tool_result,
        log_user_input, update_streaming_log,
    },
    state::{AgentState, DiffSnapshot, InputMode, LogLevel, MAX_CONVERSATION_TOKENS},
    ui::{main_ui::handle_event, tui::draw_ui},
};

#[derive(Parser)]
#[command(
    name = "osmogrep",
    version,
    about = "A lightweight Rust-based TUI Agent, for debugging, code reviews, and runtime bug catching."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Analyze GitHub PRs/issues for duplicates, ranking, and scope drift
    Triage(triage::TriageArgs),
}

fn env_truthy(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(val) => {
            let v = val.to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => default,
    }
}

fn run_shell(state: &mut AgentState, cmd: &str) {
    log(state, LogLevel::Info, &format!("SHELL : $ {}", cmd));

    match std::process::Command::new("sh").arg("-c").arg(cmd).output() {
        Ok(out) => {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                log(state, LogLevel::Info, line);
            }
            for line in String::from_utf8_lossy(&out.stderr).lines() {
                log(state, LogLevel::Error, line);
            }
        }
        Err(e) => {
            log(state, LogLevel::Error, e.to_string());
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    if let Some(CliCommand::Triage(args)) = cli.command {
        triage::run(args)?;
        return Ok(());
    }

    run_tui()
}

fn run_tui() -> Result<(), Box<dyn Error>> {
    setup_terminal()?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = init_state();
    persistence::load(&mut state);
    let mut agent = Agent::new();

    let (voice_cmd_tx, voice_cmd_rx) = mpsc::channel();
    let (voice_evt_tx, voice_evt_rx) = mpsc::channel();
    let _voice_handle = voice::spawn_voice_worker(voice_cmd_rx, voice_evt_tx.clone());
    let proxy_listen = std::env::var("VLLM_REALTIME_PROXY_LISTEN").ok();
    if let Some(listen_addr) = proxy_listen.clone() {
        let _proxy_handle = voice::spawn_voice_proxy_worker(
            listen_addr.clone(),
            state.voice.url.clone(),
            state.voice.model.clone(),
            voice_evt_tx.clone(),
        );
        state.voice.visible = true;
        state.voice.enabled = true;
    } else if env_truthy("VLLM_REALTIME_AUTOCONNECT", false) {
        let _ = voice_cmd_tx.send(voice::VoiceCommand::Start {
            url: state.voice.url.clone(),
            model: state.voice.model.clone(),
        });
        state.voice.visible = true;
        state.voice.enabled = true;
        log_status(&mut state, "Connecting voice input...");
    }

    let mut agent_rx: Option<mpsc::Receiver<AgentEvent>> = None;
    let mut agent_cancel: Option<CancelToken> = None;
    let mut context_rx: Option<mpsc::Receiver<ContextEvent>> = None;
    let voice_silence_ms: u64 = std::env::var("VLLM_REALTIME_SILENCE_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1200);

    /* ---------- SPAWN CONTEXT INDEXER ---------- */

    {
        let (tx, rx) = mpsc::channel();
        let root = state.repo_root.clone();

        context::spawn_indexer(root, tx);
        context_rx = Some(rx);
    }

    /* ---------- MAIN LOOP ---------- */

    loop {
        let (input_rect, _, exec_rect) = draw_ui(&mut terminal, &state)?;

        if event::poll(Duration::from_millis(120))? {
            let ev = event::read()?;
            handle_event(&mut state, ev, input_rect, Rect::default(), exec_rect);
        }

        if state.ui.should_exit {
            let _ = persistence::save(&state);
            break;
        }

        if let Some(mut rx) = context_rx.take() {
            let mut done = false;

            loop {
                match rx.try_recv() {
                    Ok(evt) => match evt {
                        ContextEvent::Started => {
                            state.ui.indexing = true;
                            state.ui.indexed = false;
                            state.ui.spinner_started_at = Some(Instant::now());
                        }

                        ContextEvent::Finished => {
                            state.ui.indexing = false;
                            state.ui.indexed = true;
                            state.ui.spinner_started_at = None;
                            done = true;
                        }

                        ContextEvent::Error(_e) => {
                            state.ui.indexing = false;
                            state.ui.spinner_started_at = None;
                            done = true;
                        }
                    },

                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        state.ui.indexing = false;
                        state.ui.spinner_started_at = None;
                        done = true;
                        break;
                    }
                }
            }
            if !done {
                context_rx = Some(rx);
            }
        }

        loop {
            match voice_evt_rx.try_recv() {
                Ok(evt) => match evt {
                    voice::VoiceEvent::Connected => {
                        state.voice.visible = true;
                        state.voice.connected = true;
                        state.voice.status = Some("connected".into());
                        state.voice.buffer.clear();
                        state.voice.last_activity = Some(Instant::now());
                        state.voice.last_inserted = None;
                        log_status(&mut state, "Voice connected.");
                    }
                    voice::VoiceEvent::Disconnected => {
                        state.voice.connected = false;
                        state.voice.enabled = false;
                        state.voice.status = Some("disconnected".into());
                        state.voice.partial = None;
                        state.voice.last_final = None;
                        state.voice.buffer.clear();
                        state.voice.last_activity = None;
                        state.voice.last_inserted = None;
                    }
                    voice::VoiceEvent::Partial(delta) => {
                        state.voice.partial = Some(delta);
                        state.voice.last_final = None;
                        if let Some(delta) = state.voice.partial.as_deref() {
                            state.voice.buffer.push_str(delta);
                        }
                        state.voice.last_activity = Some(Instant::now());
                        if state.ui.input.is_empty()
                            || state.voice.last_inserted.as_deref() == Some(state.ui.input.as_str())
                        {
                            state.ui.input = state.voice.buffer.clone();
                            state.voice.last_inserted = Some(state.ui.input.clone());
                        }
                    }
                    voice::VoiceEvent::Final(text) => {
                        let final_text = if text.trim().is_empty() {
                            state.voice.buffer.clone()
                        } else {
                            text
                        };
                        state.ui.input_mode = InputMode::AgentText;
                        state.ui.input_masked = false;
                        state.ui.input_placeholder = None;
                        state.ui.history_index = None;
                        state.clear_hint();
                        state.clear_autocomplete();
                        if !final_text.trim().is_empty() {
                            if state.ui.input.is_empty()
                                || state.voice.last_inserted.as_deref()
                                    == Some(state.ui.input.as_str())
                            {
                                state.ui.input = final_text.clone();
                            } else {
                                if !state.ui.input.ends_with(' ') {
                                    state.ui.input.push(' ');
                                }
                                state.ui.input.push_str(&final_text);
                            }
                            state.voice.last_inserted = Some(state.ui.input.clone());
                        }
                        state.voice.partial = None;
                        state.voice.last_final = Some(final_text);
                        state.voice.buffer.clear();
                        state.voice.last_activity = Some(Instant::now());
                    }
                    voice::VoiceEvent::Error(msg) => {
                        state.voice.visible = true;
                        state.voice.enabled = false;
                        state.voice.connected = false;
                        state.voice.status = Some(format!("error: {msg}"));
                        state.voice.buffer.clear();
                        state.voice.last_activity = None;
                        state.voice.last_inserted = None;
                        log(&mut state, LogLevel::Error, msg);
                    }
                    voice::VoiceEvent::Status(msg) => {
                        let status = msg.clone();
                        state.voice.status = Some(msg);
                        log_status(&mut state, status);
                    }
                },
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }

        if state.voice.connected {
            if let Some(last) = state.voice.last_activity {
                if !state.voice.buffer.is_empty()
                    && last.elapsed() >= Duration::from_millis(voice_silence_ms)
                {
                    let final_text = state.voice.buffer.clone();
                    if !final_text.trim().is_empty() {
                        state.ui.input_mode = InputMode::AgentText;
                        state.ui.input_masked = false;
                        state.ui.input_placeholder = None;
                        state.ui.history_index = None;
                        state.clear_hint();
                        state.clear_autocomplete();
                        if state.ui.input.is_empty()
                            || state.voice.last_inserted.as_deref() == Some(state.ui.input.as_str())
                        {
                            state.ui.input = final_text.clone();
                        } else {
                            if !state.ui.input.ends_with(' ') {
                                state.ui.input.push(' ');
                            }
                            state.ui.input.push_str(&final_text);
                        }
                        state.voice.last_final = Some(final_text);
                        state.voice.last_inserted = Some(state.ui.input.clone());
                    }
                    state.voice.buffer.clear();
                    state.voice.partial = None;
                    state.voice.last_activity = Some(Instant::now());
                }
            }
        }

        if state.ui.cancel_requested {
            if let Some(token) = agent_cancel.as_ref() {
                token.cancel();
                log_status(&mut state, "Cancellation requested.");
            }
            state.ui.cancel_requested = false;
        }

        if let Some(rx) = agent_rx.as_ref() {
            loop {
                match rx.try_recv() {
                    Ok(evt) => match evt {
                        AgentEvent::ToolCall { name, args } => {
                            let cmd = match args {
                                serde_json::Value::Object(ref map) => map
                                    .values()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(" "),
                                _ => String::new(),
                            };

                            log_tool_call(&mut state, &name, cmd);
                        }

                        AgentEvent::ToolResult { summary } => {
                            log_tool_result(&mut state, summary);
                        }

                        AgentEvent::ToolDiff {
                            tool,
                            target,
                            before,
                            after,
                        } => {
                            let snap = DiffSnapshot {
                                tool,
                                target,
                                before,
                                after,
                            };

                            state.session_changes.push(snap.clone());
                            state.undo_stack.push(snap.clone());
                            state.ui.diff_active = true;
                            state.ui.diff_snapshot = vec![snap];
                            let _ = persistence::save(&state);
                        }

                        AgentEvent::PreviewDiff {
                            tool,
                            target,
                            before,
                            after,
                        } => {
                            state.ui.diff_active = true;
                            state.ui.diff_snapshot = vec![DiffSnapshot {
                                tool: format!("preview:{tool}"),
                                target,
                                before,
                                after,
                            }];
                        }

                        AgentEvent::OutputText(text) => {
                            state.usage.completion_tokens += (text.len() / 4).max(1);
                            if !state.ui.streaming_active {
                                log_agent_output(&mut state, &text);
                            }
                            let _ = persistence::save(&state);
                        }

                        AgentEvent::StreamDelta(delta) => {
                            state.ui.streaming_active = true;
                            state.ui.follow_tail = true;
                            state.ui.streaming_buffer.push_str(&delta);
                            update_streaming_log(&mut state);
                        }

                        AgentEvent::StreamDone => {
                            if state.ui.streaming_active {
                                flush_streaming_log(&mut state);
                            }
                            state.ui.streaming_active = false;
                        }

                        AgentEvent::PermissionRequest {
                            tool_name,
                            args_summary,
                            reply_tx,
                        } => {
                            if state.ui.auto_approve {
                                let _ = reply_tx.send(true);
                                log_status(
                                    &mut state,
                                    format!("Auto-approved {} ({})", tool_name, args_summary),
                                );
                            } else {
                                state.ui.pending_permission =
                                    Some(crate::state::PendingPermission {
                                        tool_name,
                                        args_summary,
                                        reply_tx,
                                    });
                            }
                        }

                        AgentEvent::ConversationUpdate(messages) => {
                            state.conversation.set_messages(messages);
                            state.conversation.trim_to_budget(MAX_CONVERSATION_TOKENS);
                            let _ = persistence::save(&state);
                        }

                        AgentEvent::Cancelled => {
                            state.ui.streaming_active = false;
                            flush_streaming_log(&mut state);
                            log(&mut state, LogLevel::Warn, "Agent cancelled.");
                            state.ui.spinner_started_at = None;
                            state.ui.agent_running = false;
                            state.ui.pending_permission = None;
                            agent_cancel = None;
                            agent_rx = None;
                            break;
                        }

                        AgentEvent::Error(e) => {
                            log(&mut state, LogLevel::Error, e);
                            state.ui.streaming_active = false;
                            flush_streaming_log(&mut state);
                            state.ui.spinner_started_at = None;
                            state.ui.agent_running = false;
                            state.ui.pending_permission = None;
                            agent_cancel = None;
                            agent_rx = None;
                            break;
                        }

                        AgentEvent::Done => {
                            state.ui.spinner_started_at = None;
                            state.ui.agent_running = false;
                            state.ui.pending_permission = None;
                            agent_cancel = None;
                            agent_rx = None;
                            break;
                        }
                    },

                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        state.ui.streaming_active = false;
                        flush_streaming_log(&mut state);
                        state.ui.spinner_started_at = None;
                        state.ui.agent_running = false;
                        state.ui.pending_permission = None;
                        agent_cancel = None;
                        agent_rx = None;
                        break;
                    }
                }
            }
        }

        if state.ui.execution_pending {
            state.ui.execution_pending = false;

            let mode = state.ui.input_mode;
            let raw = state.commit_input();
            let text = raw.trim();

            state.ui.hint = None;
            state.ui.autocomplete = None;

            match mode {
                InputMode::ApiKey => {
                    log(&mut state, LogLevel::Info, "[api key entered]");
                }

                _ if !text.is_empty() => {
                    log_user_input(&mut state, text);
                }

                _ => {}
            }

            match mode {
                InputMode::Command => {
                    commands::handle_command(
                        &mut state,
                        text,
                        Some(&voice_cmd_tx),
                        Some(&mut agent),
                    );
                }

                InputMode::Shell => {
                    if !text.is_empty() {
                        run_shell(&mut state, text);
                    }
                    continue;
                }

                InputMode::AgentText => {
                    if agent.is_configured() && !text.is_empty() {
                        let (tx, rx) = mpsc::channel();
                        let repo_root = state.repo_root.clone();
                        let prior_messages = state.conversation.messages.clone();
                        let auto_approve = state.ui.auto_approve;

                        let cancel = agent.spawn(
                            repo_root,
                            text.to_string(),
                            prior_messages,
                            auto_approve,
                            tx,
                        );
                        agent_rx = Some(rx);
                        agent_cancel = Some(cancel);

                        state.ui.spinner_started_at = Some(Instant::now());
                        state.ui.agent_running = true;
                        state.usage.prompt_tokens += (text.len() / 4).max(1);
                        state.ui.streaming_active = false;
                        state.ui.streaming_buffer.clear();
                        state.ui.streaming_lines_logged = 0;
                        let _ = persistence::save(&state);
                    } else if !agent.is_configured() {
                        log(
                            &mut state,
                            LogLevel::Warn,
                            "OPENAI_API_KEY not set. Use /key",
                        );
                    }
                }

                InputMode::ApiKey => {
                    if !text.is_empty() {
                        agent.set_api_key(text.to_string());
                        log(&mut state, LogLevel::Success, "API key saved.");

                        state.ui.input_mode = InputMode::AgentText;
                        state.ui.input_masked = false;
                        state.ui.input_placeholder = None;
                    }
                }
            }
        }

        if !state.ui.execution_pending && agent_rx.is_none() {
            commands::update_command_hints(&mut state);
        }
    }

    teardown_terminal(&mut terminal)?;
    Ok(())
}

fn init_state() -> AgentState {
    let voice_url = std::env::var("VLLM_REALTIME_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:8000/v1/realtime".into());
    let voice_model = std::env::var("VLLM_REALTIME_MODEL")
        .unwrap_or_else(|_| "mistralai/Voxtral-Mini-4B-Realtime-2602".into());

    AgentState {
        ui: crate::state::UiState {
            input: String::new(),
            input_mode: InputMode::AgentText,
            input_masked: false,
            input_placeholder: None,
            execution_pending: false,
            should_exit: false,
            history: Vec::new(),
            history_index: None,
            hint: None,
            autocomplete: None,
            command_items: Vec::new(),
            command_selected: 0,
            last_activity: Instant::now(),
            exec_scroll: usize::MAX,
            follow_tail: true,
            active_spinner: None,
            spinner_started_at: None,
            agent_running: false,
            cancel_requested: false,
            auto_approve: false,
            pending_permission: None,
            streaming_buffer: String::new(),
            streaming_active: false,
            streaming_lines_logged: 0,
            diff_active: false,
            diff_snapshot: Vec::new(),

            indexing: false,
            indexed: false,
        },

        logs: crate::state::LogBuffer::new(),
        session_changes: Vec::new(),
        undo_stack: Vec::new(),
        usage: crate::state::UsageStats::default(),
        started_at: Instant::now(),
        repo_root: std::env::current_dir().unwrap(),
        voice: crate::state::VoiceState {
            visible: false,
            enabled: false,
            connected: false,
            status: None,
            partial: None,
            last_final: None,
            buffer: String::new(),
            last_activity: None,
            last_inserted: None,
            url: voice_url,
            model: voice_model,
        },
        conversation: crate::state::ConversationHistory::new(),
    }
}
fn setup_terminal() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    Ok(())
}

fn teardown_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
