// src/ui/main_ui.rs

use crate::state::{AgentState, InputMode};
use ratatui::layout::Rect;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers,
    MouseEvent, MouseEventKind,
};


fn parse_input(raw: &str) -> InputMode {
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();

    if first.starts_with('!') {
        InputMode::Shell
    } else if first.starts_with('/') {
        InputMode::Command
    } else {
        InputMode::AgentText
    }
}

pub fn handle_event(
    state: &mut AgentState,
    event: impl Into<Event>,
    _input_rect: Rect,
    _diff_rect: Rect,
    _exec_rect: Rect,
) {
    match event.into() {
        Event::Key(k) => handle_key(state, k),
        Event::Mouse(m) => handle_mouse(state, m),
        _ => {}
    }
}

fn handle_key(state: &mut AgentState, k: KeyEvent) {
    match k.code {

        KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
            state.push_char(c);
        }

        KeyCode::Backspace => {
            state.backspace();
        }

        // multiline input
        KeyCode::Enter if k.modifiers.contains(KeyModifiers::SHIFT) => {
            state.push_char('\n');
        }

        KeyCode::Enter => {
            if state.ui.execution_pending {
                return; // prevent double-trigger
            }

            let raw = state.ui.input.trim();
            if raw.is_empty() {
                return;
            }

            if state.ui.input_mode != InputMode::ApiKey {
                let mode = parse_input(raw);
                state.ui.input_mode = mode;

                if mode == InputMode::Shell {
                    state.ui.input = raw
                        .strip_prefix('!')
                        .unwrap_or(raw)
                        .to_string();
                }
            }

            state.ui.execution_pending = true;
        }

        KeyCode::Up if !k.modifiers.contains(KeyModifiers::CONTROL) => {
            state.history_prev();
        }

        KeyCode::Down if !k.modifiers.contains(KeyModifiers::CONTROL) => {
            state.history_next();
        }


        KeyCode::Tab => {
            if let Some(ac) = &state.ui.autocomplete {
                if ac.starts_with(&state.ui.input) {
                    let suffix = &ac[state.ui.input.len()..];
                    state.ui.input.push_str(suffix);
                } else {
                    state.ui.input = ac.clone();
                }
            }
        }


        KeyCode::PageUp => {
            state.ui.exec_scroll = state.ui.exec_scroll.saturating_add(5);
        }

        KeyCode::PageDown => {
            state.ui.exec_scroll = state.ui.exec_scroll.saturating_sub(5);
        }

        KeyCode::Up if k.modifiers.contains(KeyModifiers::CONTROL) => {
            state.ui.exec_scroll = state.ui.exec_scroll.saturating_add(1);
        }

        KeyCode::Down if k.modifiers.contains(KeyModifiers::CONTROL) => {
            state.ui.exec_scroll = state.ui.exec_scroll.saturating_sub(1);
        }


        KeyCode::Esc => {
            state.ui.should_exit = true;
        }

        _ => {}
    }
}

fn handle_mouse(state: &mut AgentState, m: MouseEvent) {
    match m.kind {
        MouseEventKind::ScrollUp => {
            state.ui.exec_scroll = state.ui.exec_scroll.saturating_add(3);
        }

        MouseEventKind::ScrollDown => {
            state.ui.exec_scroll = state.ui.exec_scroll.saturating_sub(3);
        }

        _ => {}
    }
}
