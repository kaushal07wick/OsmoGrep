mod state;
mod logger;
mod ui;
mod tools;
mod commands;
mod agent;
mod context;

use std::{
    error::Error,
    io,
    sync::mpsc,
    time::{Duration, Instant},
};

use crossterm::{
    event,
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode,
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use crossterm::event::{EnableMouseCapture, DisableMouseCapture};

use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::Rect,
};

use clap::Parser;

use crate::{
    agent::{Agent, AgentEvent},
    context::{ContextEvent},
    logger::{
        log,
        log_user_input,
        log_tool_call,
        log_tool_result,
        log_agent_output,
    },
    state::{AgentState, InputMode, LogLevel, DiffSnapshot},
    ui::{main_ui::handle_event, tui::draw_ui},
};

#[derive(Parser)]
#[command(
    name = "osmogrep",
    version,
    about = "A lightweight Rust-based TUI Agent, for debugging, code reviews, and runtime bug catching."
)]
struct Cli {}

fn run_shell(state: &mut AgentState, cmd: &str) {
    log(
        state,
        LogLevel::Info,
        &format!("SHELL : $ {}", cmd),
    );

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
    let _cli = Cli::parse();
    setup_terminal()?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = init_state();
    let mut agent = Agent::new();

    let mut agent_rx: Option<mpsc::Receiver<AgentEvent>> = None;
    let mut context_rx: Option<mpsc::Receiver<ContextEvent>> = None;

    /* ---------- SPAWN CONTEXT INDEXER ---------- */

    {
        let (tx, rx) = mpsc::channel();
        let root = state.repo_root.clone();

        context::spawn_indexer(root, tx);
        context_rx = Some(rx);
    }

    /* ---------- MAIN LOOP ---------- */

    loop {
        let (input_rect, _, exec_rect) =
            draw_ui(&mut terminal, &state)?;

        if event::poll(Duration::from_millis(120))? {
            let ev = event::read()?;
            handle_event(
                &mut state,
                ev,
                input_rect,
                Rect::default(),
                exec_rect,
            );
        }

        if state.ui.should_exit {
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

        if let Some(rx) = agent_rx.as_ref() {
            loop {
                match rx.try_recv() {
                    Ok(evt) => match evt {
                        AgentEvent::ToolCall { name, args } => {
                            let cmd = match args {
                                serde_json::Value::Object(ref map) => {
                                    map.values()
                                        .filter_map(|v| v.as_str())
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                }
                                _ => String::new(),
                            };

                            log_tool_call(&mut state, &name, cmd);
                        }

                        AgentEvent::ToolResult { summary } => {
                            log_tool_result(&mut state, summary);
                        }

                        AgentEvent::ToolDiff { tool, target, before, after } => {
                            state.ui.diff_active = true;
                            state.ui.diff_snapshot.clear();

                            state.ui.diff_snapshot.push(
                                DiffSnapshot {
                                    tool,
                                    target,
                                    before,
                                    after,
                                }
                            );
                        }

                        AgentEvent::OutputText(text) => {
                            log_agent_output(&mut state, &text);
                        }

                        AgentEvent::Error(e) => {
                            log(&mut state, LogLevel::Error, e);
                            state.ui.spinner_started_at = None;
                            agent_rx = None;
                            break;
                        }

                        AgentEvent::Done => {
                            state.ui.spinner_started_at = None;
                            agent_rx = None;
                            break;
                        }
                    },

                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        state.ui.spinner_started_at = None;
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
                    commands::handle_command(&mut state, text);
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

                        agent.spawn(repo_root, text.to_string(), tx);
                        agent_rx = Some(rx);

                        state.ui.spinner_started_at = Some(Instant::now());
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
            diff_active: false,
            diff_snapshot: Vec::new(),

            indexing: false,
            indexed: false,
        },

        logs: crate::state::LogBuffer::new(),
        started_at: Instant::now(),
        repo_root: std::env::current_dir().unwrap(),
    }
}
fn setup_terminal() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    Ok(())
}

fn teardown_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
