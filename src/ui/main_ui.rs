// src/ui/main_ui.rs

use crate::state::{AgentState, Phase, Focus, InputMode};
use crate::commands::{handle_command, update_command_hints};
use crate::state::{LogLevel};
use crate::logger::log;
use crate::{LlmClient, LlmBackend};
use crate::llm::client::Provider;

use ratatui::layout::Rect;
use crossterm::event::{Event, KeyCode, KeyEvent, MouseEvent, MouseEventKind};

pub fn handle_event(
    state: &mut AgentState,
    event: impl Into<Event>,
    input_rect: Rect,
    diff_rect: Rect,
    exec_rect: Rect,
) {
    match event.into() {
        Event::Key(k) => handle_key(state, k),
        Event::Mouse(m) => handle_mouse(state, m, input_rect, diff_rect, exec_rect),
        _ => {}
    }
}

pub fn handle_key(state: &mut AgentState, k: KeyEvent) {
    match state.ui.focus {
        Focus::Input => handle_input_keys(state, k),
        Focus::Diff => handle_diff_keys(state, k),
        Focus::Execution => handle_exec_keys(state, k),
    }

    if k.code == KeyCode::Esc {
        state.lifecycle.phase = Phase::Done;
    }
}

pub fn handle_input_keys(state: &mut AgentState, k: KeyEvent) {
    match k.code {
        KeyCode::Char(c) => {
            state.push_char(c);
            update_command_hints(state);
        }
        KeyCode::Backspace => {
            state.backspace();
            update_command_hints(state);
        }
        KeyCode::Enter => match &state.ui.input_mode {
            InputMode::ApiKey { provider, model } => {
                let api_key = state.ui.input.clone();
                let client = LlmClient::new();

                let provider_str = match provider {
                    Provider::OpenAI => "openai",
                    Provider::Anthropic => "anthropic",
                };

                match client.configure(provider_str, model.clone(), api_key, None) {
                    Ok(_) => {
                        state.llm_backend = LlmBackend::Remote { client };
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

                state.ui.input.clear();
                state.ui.input_mode = InputMode::Command;
                state.ui.input_masked = false;
                state.ui.input_placeholder = None;
                state.ui.focus = Focus::Input;
            }

            InputMode::Command => {
                if !state.ui.first_command_done && !state.ui.input.trim().is_empty() {
                    state.ui.first_command_done = true;
                }

                let cmd = state.commit_input();
                handle_command(state, &cmd);
                update_command_hints(state);
                state.ui.input.clear();
            }
        },

        KeyCode::Up => {
            state.history_prev();
            update_command_hints(state);
        }
        KeyCode::Down => {
            state.history_next();
            update_command_hints(state);
        }
        KeyCode::Tab => {
            if let Some(ac) = &state.ui.autocomplete {
                if ac.starts_with(&state.ui.input) {
                    let suffix = &ac[state.ui.input.len()..];
                    state.ui.input.push_str(suffix);
                } else {
                    state.ui.input = ac.clone();
                }
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

pub fn handle_diff_keys(state: &mut AgentState, k: KeyEvent) {
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

pub fn handle_exec_keys(state: &mut AgentState, k: KeyEvent) {
    if state.ui.panel_view.is_some() {
        match k.code {
            KeyCode::Up => state.ui.panel_scroll = state.ui.panel_scroll.saturating_sub(1),
            KeyCode::Down => state.ui.panel_scroll = state.ui.panel_scroll.saturating_add(1),
            KeyCode::PageUp => state.ui.panel_scroll = state.ui.panel_scroll.saturating_sub(10),
            KeyCode::PageDown => state.ui.panel_scroll = state.ui.panel_scroll.saturating_add(10),
            KeyCode::Left => state.ui.panel_scroll_x = state.ui.panel_scroll_x.saturating_sub(4),
            KeyCode::Right => state.ui.panel_scroll_x = state.ui.panel_scroll_x.saturating_add(4),
            KeyCode::Esc => {
                state.ui.panel_view = None;
                state.ui.focus = Focus::Input;
            }
            _ => {}
        }
        return;
    }

    match k.code {
        KeyCode::Up => state.ui.exec_scroll = state.ui.exec_scroll.saturating_sub(1),
        KeyCode::PageUp => state.ui.exec_scroll = state.ui.exec_scroll.saturating_sub(10),
        KeyCode::Down => {
            state.ui.exec_scroll = state.ui.exec_scroll.saturating_add(1);
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
        KeyCode::End => state.ui.exec_scroll = usize::MAX,
        _ => {}
    }
}

pub fn handle_mouse(
    state: &mut AgentState,
    m: MouseEvent,
    input_rect: Rect,
    diff_rect: Rect,
    exec_rect: Rect,
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
