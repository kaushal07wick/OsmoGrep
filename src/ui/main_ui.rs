use crate::state::{AgentState, InputMode};
use ratatui::layout::Rect;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers,
    MouseEvent, MouseEventKind,
};
const SCROLL_LINE_STEP: usize = 3;
const SCROLL_PAGE_STEP: usize = 12;
const SCROLL_WHEEL_STEP: usize = 6;

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
    let palette_active = !state.ui.command_items.is_empty();

    match k.code {
        /* ---------- Palette navigation ---------- */

        KeyCode::Up if palette_active => {
            state.ui.command_selected =
                state.ui.command_selected.saturating_sub(1);
        }

        KeyCode::Down if palette_active => {
            let max = state.ui.command_items.len().saturating_sub(1);
            state.ui.command_selected =
                (state.ui.command_selected + 1).min(max);
        }

        KeyCode::Enter if palette_active => {
            if let Some(item) =
                state.ui.command_items.get(state.ui.command_selected)
            {
                state.ui.input = item.cmd.to_string();
                state.ui.input_mode = InputMode::Command; // ← CRITICAL
                state.ui.command_items.clear();
                state.ui.command_selected = 0;
                state.ui.execution_pending = true;
            }
        }


        KeyCode::Esc if palette_active => {
            state.ui.command_items.clear();
            state.ui.command_selected = 0;
        }

        /* ---------- Text input ---------- */

        KeyCode::Char(c)
            if !k.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            state.push_char(c);
        }

        KeyCode::Backspace => {
            state.backspace();
        }

        KeyCode::Enter if k.modifiers.contains(KeyModifiers::SHIFT) => {
            state.push_char('\n');
        }

        /* ---------- Normal Enter (no palette) ---------- */

        KeyCode::Enter => {
            if state.ui.execution_pending {
                return;
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

            state.ui.command_items.clear();
            state.ui.command_selected = 0;

            state.ui.autocomplete = None;
            state.ui.hint = None;

            state.ui.execution_pending = true;
        }

        /* ---------- History (disabled during palette) ---------- */

        KeyCode::Up if !palette_active => {
            state.history_prev();
        }

        KeyCode::Down if !palette_active => {
            state.history_next();
        }

        /* ---------- Autocomplete ---------- */

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

        /* ---------- Execution scrolling ---------- */

        KeyCode::PageUp => {
            state.ui.exec_scroll = match state.ui.exec_scroll {
                usize::MAX => SCROLL_PAGE_STEP,
                v => v.saturating_add(SCROLL_PAGE_STEP),
            };
            state.ui.follow_tail = false;
        }

        KeyCode::PageDown => {
            state.ui.exec_scroll =
                state.ui.exec_scroll.saturating_sub(SCROLL_PAGE_STEP);
            if state.ui.exec_scroll == 0 {
                state.ui.exec_scroll = usize::MAX;
                state.ui.follow_tail = true;
            }
        }

        KeyCode::Up if k.modifiers.contains(KeyModifiers::CONTROL) => {
            state.ui.exec_scroll = match state.ui.exec_scroll {
                usize::MAX => SCROLL_LINE_STEP,
                v => v.saturating_add(SCROLL_LINE_STEP),
            };
            state.ui.follow_tail = false;
        }

        KeyCode::Down if k.modifiers.contains(KeyModifiers::CONTROL) => {
            state.ui.exec_scroll =
                state.ui.exec_scroll.saturating_sub(SCROLL_LINE_STEP);
            if state.ui.exec_scroll == 0 {
                state.ui.exec_scroll = usize::MAX;
                state.ui.follow_tail = true;
            }
        }

        KeyCode::End => {
            state.ui.exec_scroll = usize::MAX;
            state.ui.follow_tail = true;
        }

        /* ---------- Exit ---------- */

        KeyCode::Esc => {
            state.ui.should_exit = true;
        }

        _ => {}
    }
}


fn handle_mouse(state: &mut AgentState, m: MouseEvent) {
    match m.kind {
        MouseEventKind::ScrollUp => {
            state.ui.exec_scroll = match state.ui.exec_scroll {
                usize::MAX => SCROLL_WHEEL_STEP,
                v => v.saturating_add(SCROLL_WHEEL_STEP),
            };
            state.ui.follow_tail = false;
        }

        MouseEventKind::ScrollDown => {
            state.ui.exec_scroll = state.ui.exec_scroll.saturating_sub(SCROLL_WHEEL_STEP);
            if state.ui.exec_scroll == 0 {
                state.ui.exec_scroll = usize::MAX;
                state.ui.follow_tail = true;
            }
        }

        _ => {}
    }
}
