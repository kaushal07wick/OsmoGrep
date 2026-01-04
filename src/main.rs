// src/main.rs

mod state;
mod logger;
mod git;
mod ui;
mod commands;
mod machine;
mod detectors;
mod testgen;
mod llm;
mod context;

use std::{ error::Error, io, sync::{Arc, atomic::AtomicBool}, time::{Duration, Instant} };
use std::sync::mpsc::channel;
use crate::{llm::{backend::LlmBackend, client::{LlmClient, Provider}}, logger::log, state::InputMode};
use crossterm::{
    event::{self, Event, KeyCode, MouseEventKind},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::{backend::CrosstermBackend, Terminal};

use state::{
    AgentContext, AgentEvent, AgentState, LifecycleState, LogBuffer,
    Phase, Focus, UiState, LogLevel, TestResult, SinglePanelView,
};
use commands::{handle_command, update_command_hints};
use machine::step;

fn main() -> Result<(), Box<dyn Error>> {
    setup_terminal()?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = init_state();

    loop {
        let (input_rect, diff_rect, exec_rect) =
            ui::draw_ui(&mut terminal, &state)?;

        if event::poll(Duration::from_millis(120))? {
            let ev = event::read()?;
            handle_event(&mut state, ev, input_rect, diff_rect, exec_rect);
        }

        // advance lifecycle 
        step(&mut state);

        // drain background agent events
        drain_agent_events(&mut state);

        if matches!(state.lifecycle.phase, Phase::Done) {
            break;
        }
    }

    teardown_terminal(&mut terminal)?;
    Ok(())
}

fn drain_agent_events(state: &mut AgentState) {
    while let Ok(ev) = state.agent_rx.try_recv() {
        match ev {
            AgentEvent::Log(level, msg) => {
                state.push_log(level, msg);
            }

            AgentEvent::SpinnerStart(text) => {
                state.start_spinner(text);
            }

            AgentEvent::SpinnerStop => {
                state.stop_spinner();
            }

            AgentEvent::GeneratedTest(code) => {
                state.context.last_generated_test = Some(code);
                state.context.generated_tests_ready = true;
                state.push_log(LogLevel::Success, " Test generated");
            }

            AgentEvent::TestStarted => {
                state.push_log(LogLevel::Info, "🧪 Test execution started");
            }

            AgentEvent::TestFinished(result) => {
                state.context.last_test_result = Some(result.clone());

                match &result {
                    TestResult::Passed => {
                        state.push_log(LogLevel::Success, "🧪 Test passed");
                        state.ui.panel_view = Some(SinglePanelView::TestResult {
                            output: String::new(),
                            passed: true,
                        });
                    }

                    TestResult::Failed { output } => {
                        state.push_log(LogLevel::Error, "🧪 Test failed");
                        state.ui.panel_view = Some(SinglePanelView::TestResult {
                            output: output.clone(),
                            passed: false,
                        });
                    }
                }

                state.ui.focus = Focus::Execution;
                state.ui.panel_scroll = 0;
            }

            AgentEvent::Finished => {
                state.push_log(LogLevel::Success, " Agent finished");
                state.lifecycle.phase = Phase::Idle;
            }

            AgentEvent::Failed(err) => {
                state.push_log(LogLevel::Error, format!(" {}", err));
                state.lifecycle.phase = Phase::Idle;
            }
        }
    }
}

fn init_state() -> AgentState {
    let (agent_tx, agent_rx) = channel::<AgentEvent>();

    AgentState {
        lifecycle: LifecycleState {
            phase: Phase::Init,
            base_branch: None,
            original_branch: None,
            current_branch: None,
            agent_branch: None,
            language: None,
            framework: None,
        },

        context: AgentContext {
            diff_analysis: Vec::new(),
            test_candidates: Vec::new(),
            last_generated_test: None,
            generated_tests_ready: false,
            last_test_result: None,
        },

        ui: UiState {
            input: String::new(),
            history: Vec::new(),
            history_index: None,
            hint: None,
            autocomplete: None,
            last_activity: Instant::now(),

            selected_diff: None,
            diff_scroll: 0,
            diff_scroll_x: 0,
            in_diff_view: false,
            diff_side_by_side: false,
            focus: Focus::Input,

            exec_scroll: 0,
            active_spinner: None,
            spinner_started_at: None,
            spinner_elapsed: None,

            panel_view: None,
            panel_scroll: 0,
            panel_scroll_x: 0,

            cached_log_lines: Vec::new(),
            last_log_len: 0,
            dirty: true,
            last_draw: Instant::now(),
            input_mode: InputMode::Command,
            input_masked: false,
            input_placeholder: None,
        },
        logs: LogBuffer::new(),
        llm_backend: LlmBackend::Remote {client: LlmClient::new(),},
        agent_tx,
        agent_rx,
        cancel_requested: Arc::new(AtomicBool::new(false)),
        full_context_snapshot: None,
        force_reload: false,
    }
}

fn handle_event(
    state: &mut AgentState,
    event: impl Into<Event>,
    input_rect: ratatui::layout::Rect,
    diff_rect: ratatui::layout::Rect,
    exec_rect: ratatui::layout::Rect,
) {
    match event.into() {
        Event::Key(k) => handle_key(state, k),
        Event::Mouse(m) => handle_mouse(state, m, input_rect, diff_rect, exec_rect),
        _ => {}
    }
}

fn handle_key(state: &mut AgentState, k: crossterm::event::KeyEvent) {
    match state.ui.focus {
        Focus::Input => handle_input_keys(state, k),
        Focus::Diff => handle_diff_keys(state, k),
        Focus::Execution => handle_exec_keys(state, k),
    }

    if k.code == KeyCode::Esc {
        state.lifecycle.phase = Phase::Done;
    }
}


fn handle_input_keys(state: &mut AgentState, k: crossterm::event::KeyEvent) {
    match k.code {
        KeyCode::Char(c) => {
            state.push_char(c);
            update_command_hints(state);
        }
        KeyCode::Backspace => {
            state.backspace();
            update_command_hints(state);
        }
        KeyCode::Enter => {
            match &state.ui.input_mode {
                InputMode::ApiKey { provider, model } => {
                    let api_key = state.ui.input.clone();

                    let client = LlmClient::new();
                    let provider_str = match provider {
                        Provider::OpenAI => "openai",
                        Provider::Anthropic => "anthropic",
                    };

                    match client.configure(
                        provider_str,
                        model.clone(),
                        api_key,
                        None,
                    ) {
                        Ok(_) => {
                            state.llm_backend = LlmBackend::remote(client);
                            log(
                                state,
                                LogLevel::Success,
                                format!("API key set. Using {} ({})", provider_str, model),
                            );
                        }
                        Err(e) => {
                            log(state, LogLevel::Error, e);
                        }
                    }

                    // reset input state
                    state.ui.input.clear();
                    state.ui.input_mode = InputMode::Command;
                    state.ui.input_masked = false;
                    state.ui.input_placeholder = None;
                    state.ui.focus = Focus::Input;
                }

                InputMode::Command => {
                    let cmd = state.commit_input();
                    handle_command(state, &cmd);
                    update_command_hints(state);
                }
            }
        }
        KeyCode::Up => {
            state.history_prev();
            update_command_hints(state);
        }
        KeyCode::Down => {
            state.history_next();
            update_command_hints(state);
        }
        KeyCode::Tab => {
            if let Some(ac) = state.ui.autocomplete.clone() {
                state.ui.input = ac;
                update_command_hints(state);
            }
        }
        KeyCode::Esc => {
            state.ui.input.clear();
            state.clear_hint();
            state.clear_autocomplete();
        }
        _ => {}
    }
}


fn handle_diff_keys(state: &mut AgentState, k: crossterm::event::KeyEvent) {
    match k.code {
        KeyCode::Up => state.ui.diff_scroll = state.ui.diff_scroll.saturating_sub(1),
        KeyCode::Down => state.ui.diff_scroll = state.ui.diff_scroll.saturating_add(1),
        KeyCode::Left => state.ui.diff_scroll_x = state.ui.diff_scroll_x.saturating_sub(4),
        KeyCode::Right => state.ui.diff_scroll_x = state.ui.diff_scroll_x.saturating_add(4),
        KeyCode::PageUp => state.ui.diff_scroll = state.ui.diff_scroll.saturating_sub(10),
        KeyCode::PageDown => state.ui.diff_scroll = state.ui.diff_scroll.saturating_add(10),
        KeyCode::Char('s') => {
            state.ui.diff_side_by_side = !state.ui.diff_side_by_side;
            state.ui.diff_scroll = 0;
        }
        KeyCode::Esc => {
            state.ui.in_diff_view = false;
            state.ui.focus = Focus::Input;
        }
        _ => {}
    }
}


fn handle_exec_keys(state: &mut AgentState, k: crossterm::event::KeyEvent) {
    // Panel open → panel scroll only
    if state.ui.panel_view.is_some() {
        match k.code {
            KeyCode::Up => state.ui.panel_scroll = state.ui.panel_scroll.saturating_sub(1),
            KeyCode::Down => state.ui.panel_scroll = state.ui.panel_scroll.saturating_add(1),
            KeyCode::PageUp => state.ui.panel_scroll = state.ui.panel_scroll.saturating_sub(10),
            KeyCode::PageDown => state.ui.panel_scroll = state.ui.panel_scroll.saturating_add(10),
            KeyCode::Left => {
                state.ui.panel_scroll_x = state.ui.panel_scroll_x.saturating_sub(4);
            }
            KeyCode::Right => {
                state.ui.panel_scroll_x = state.ui.panel_scroll_x.saturating_add(4);
            }

            KeyCode::Esc => {
                state.ui.panel_view = None;
                state.ui.focus = Focus::Input;
            }
            _ => {}
        }
        return;
    }

    match k.code {
        KeyCode::Up => {
            state.ui.exec_scroll = state.ui.exec_scroll.saturating_sub(1);
        }

        KeyCode::PageUp => {
            state.ui.exec_scroll = state.ui.exec_scroll.saturating_sub(10);
        }

        KeyCode::Down => {
            state.ui.exec_scroll = state.ui.exec_scroll.saturating_add(1);

            // if user hits bottom → re-enable follow
            if state.ui.exec_scroll >= state.logs.len().saturating_sub(1) {
                state.ui.exec_scroll = usize::MAX;
            }
        }

        KeyCode::PageDown => {
            state.ui.exec_scroll = state.ui.exec_scroll.saturating_add(10);

            if state.ui.exec_scroll >= state.logs.len().saturating_sub(1) {
                state.ui.exec_scroll = usize::MAX;
            }
        }

        KeyCode::End => {
            state.ui.exec_scroll = usize::MAX;
        }

        _ => {}
    }
}



fn handle_mouse(
    state: &mut AgentState,
    m: crossterm::event::MouseEvent,
    input_rect: ratatui::layout::Rect,
    diff_rect: ratatui::layout::Rect,
    exec_rect: ratatui::layout::Rect,
) {
    let pos = (m.column, m.row).into();

    match m.kind {
        MouseEventKind::Down(_) => {
            if diff_rect.contains(pos) {
                state.ui.focus = Focus::Diff;
                state.ui.in_diff_view = true;
            } else if exec_rect.contains(pos) {
                state.ui.focus = Focus::Execution;
            } else if input_rect.contains(pos) {
                state.ui.focus = Focus::Input;
            }
        }


        MouseEventKind::ScrollUp if diff_rect.contains(pos) => {
            state.ui.diff_scroll = state.ui.diff_scroll.saturating_sub(2);
        }

        MouseEventKind::ScrollDown if diff_rect.contains(pos) => {
            state.ui.diff_scroll = state.ui.diff_scroll.saturating_add(2);
        }

        MouseEventKind::ScrollUp if exec_rect.contains(pos) => {
            if state.ui.panel_view.is_some() {
                state.ui.panel_scroll = state.ui.panel_scroll.saturating_sub(2);
            } else {
                state.ui.exec_scroll = state.ui.exec_scroll.saturating_sub(2);
            }
        }

        MouseEventKind::ScrollDown if exec_rect.contains(pos) => {
            if state.ui.panel_view.is_some() {
                state.ui.panel_scroll = state.ui.panel_scroll.saturating_add(2);
            } else {
                state.ui.exec_scroll = state.ui.exec_scroll.saturating_add(2);
            }
        }

        _ => {}
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
