use crate::state::{AgentState, InputMode};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
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
    } else if is_command_prefix(first) {
        InputMode::Command
    } else {
        InputMode::AgentText
    }
}

fn is_command_prefix(s: &str) -> bool {
    s.starts_with('/') || s.starts_with('／') || s.starts_with('÷')
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
    if let Some(pending) = state.ui.pending_permission.take() {
        match k.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let _ = pending.reply_tx.send(true);
                crate::logger::log(
                    state,
                    crate::state::LogLevel::Info,
                    format!("Approved {} {}", pending.tool_name, pending.args_summary),
                );
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                let _ = pending.reply_tx.send(false);
                crate::logger::log(
                    state,
                    crate::state::LogLevel::Warn,
                    format!("Denied {} {}", pending.tool_name, pending.args_summary),
                );
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                let _ = pending.reply_tx.send(true);
                state.ui.auto_approve = true;
                crate::logger::log(
                    state,
                    crate::state::LogLevel::Info,
                    "Auto-approve enabled for dangerous tools.",
                );
            }
            _ => {
                state.ui.pending_permission = Some(pending);
            }
        }
        return;
    }

    if let Some(update) = state.ui.pending_update.as_ref() {
        if update.installing {
            return;
        }
        match update_prompt_action(&k) {
            UpdatePromptAction::Install => {
                state.ui.update_install_requested = true;
            }
            UpdatePromptAction::Skip => {
                state.ui.update_skip_requested = true;
            }
            UpdatePromptAction::Ignore => {}
        }
        return;
    }

    let palette_active = !state.ui.command_items.is_empty();

    if let Some(action) = input_control_action(&k) {
        apply_input_control_action(state, action);
        return;
    }

    match k.code {
        /* ---------- Palette navigation ---------- */
        KeyCode::Up if palette_active => {
            state.ui.command_selected = state.ui.command_selected.saturating_sub(1);
        }

        KeyCode::Down if palette_active => {
            let max = state.ui.command_items.len().saturating_sub(1);
            state.ui.command_selected = (state.ui.command_selected + 1).min(max);
        }

        KeyCode::Enter if palette_active => {
            if let Some(item) = state.ui.command_items.get(state.ui.command_selected) {
                state.ui.input = item.cmd.to_string();
                state.ui.input_all_selected = false;
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
        KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
            state.push_char(c);
        }

        KeyCode::Backspace => {
            state.backspace();
        }

        KeyCode::Delete => {
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
                    state.ui.input = raw.strip_prefix('!').unwrap_or(raw).to_string();
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
            if state.ui.agent_running {
                let raw = state.ui.input.trim();
                if !raw.is_empty() {
                    state.ui.queued_agent_prompt = Some(raw.to_string());
                    state.ui.input.clear();
                    state.ui.input_all_selected = false;
                    crate::logger::log_status(state, "Queued follow-up prompt (tab).");
                    return;
                }
            }
            if let Some(ac) = &state.ui.autocomplete {
                if ac.starts_with(&state.ui.input) {
                    let suffix = &ac[state.ui.input.len()..];
                    state.ui.input.push_str(suffix);
                } else {
                    state.ui.input = ac.clone();
                }
                state.ui.input_all_selected = false;
            }
        }

        /* ---------- Execution scrolling ---------- */
        KeyCode::PageUp => {
            scroll_execution_back(state, SCROLL_PAGE_STEP);
        }

        KeyCode::PageDown => {
            scroll_execution_toward_tail(state, SCROLL_PAGE_STEP);
        }

        KeyCode::Up if k.modifiers.contains(KeyModifiers::CONTROL) => {
            scroll_execution_back(state, SCROLL_LINE_STEP);
        }

        KeyCode::Down if k.modifiers.contains(KeyModifiers::CONTROL) => {
            scroll_execution_toward_tail(state, SCROLL_LINE_STEP);
        }

        KeyCode::End => {
            state.ui.exec_scroll = usize::MAX;
            state.ui.follow_tail = true;
        }

        /* ---------- Exit ---------- */
        KeyCode::Esc => {
            if state.ui.agent_running {
                state.ui.cancel_requested = true;
            } else {
                state.ui.should_exit = true;
            }
        }

        _ => {}
    }
}

fn scroll_execution_back(state: &mut AgentState, step: usize) {
    let (offset, follow_tail) = scroll_back_offset(state.ui.exec_scroll, step);
    state.ui.exec_scroll = offset;
    state.ui.follow_tail = follow_tail;
}

fn scroll_execution_toward_tail(state: &mut AgentState, step: usize) {
    let (offset, follow_tail) = scroll_toward_tail_offset(state.ui.exec_scroll, step);
    state.ui.exec_scroll = offset;
    state.ui.follow_tail = follow_tail;
}

fn scroll_back_offset(current: usize, step: usize) -> (usize, bool) {
    let next = match current {
        usize::MAX => step,
        value => value.saturating_add(step),
    };
    (next, false)
}

fn scroll_toward_tail_offset(current: usize, step: usize) -> (usize, bool) {
    let value = match current {
        usize::MAX | 0 => return (usize::MAX, true),
        value => value,
    };

    let next = value.saturating_sub(step);
    if next == 0 {
        (usize::MAX, true)
    } else {
        (next, false)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputControlAction {
    SelectAll,
    CopyAll,
    CutAll,
    Paste,
    ClearAll,
}

fn input_control_action(k: &KeyEvent) -> Option<InputControlAction> {
    if !k.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }

    let KeyCode::Char(c) = k.code else {
        return None;
    };

    match c.to_ascii_lowercase() {
        'a' => Some(InputControlAction::SelectAll),
        'c' => Some(InputControlAction::CopyAll),
        'x' => Some(InputControlAction::CutAll),
        'v' => Some(InputControlAction::Paste),
        'u' => Some(InputControlAction::ClearAll),
        _ => None,
    }
}

fn apply_input_control_action(state: &mut AgentState, action: InputControlAction) {
    match action {
        InputControlAction::SelectAll => {
            if state.select_all_input() {
                crate::logger::log_status(state, "Selected input.");
            }
        }
        InputControlAction::CopyAll => {
            if state.copy_input() {
                crate::logger::log_status(state, "Copied input.");
            } else if state.ui.agent_running {
                state.ui.cancel_requested = true;
                crate::logger::log_status(state, "Cancel requested.");
            }
        }
        InputControlAction::CutAll => {
            if state.cut_input() {
                crate::logger::log_status(state, "Cut input.");
            }
        }
        InputControlAction::Paste => {
            if state.paste_input() {
                crate::logger::log_status(state, "Pasted input.");
            }
        }
        InputControlAction::ClearAll => {
            if state.clear_input() {
                crate::logger::log_status(state, "Cleared input.");
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdatePromptAction {
    Install,
    Skip,
    Ignore,
}

fn update_prompt_action(k: &KeyEvent) -> UpdatePromptAction {
    match k.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => UpdatePromptAction::Install,
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => UpdatePromptAction::Skip,
        _ => UpdatePromptAction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        input_control_action, scroll_back_offset, scroll_toward_tail_offset, update_prompt_action,
        InputControlAction, UpdatePromptAction,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn input_control_maps_common_line_editing_shortcuts() {
        assert_eq!(
            input_control_action(&ctrl('a')),
            Some(InputControlAction::SelectAll)
        );
        assert_eq!(
            input_control_action(&ctrl('c')),
            Some(InputControlAction::CopyAll)
        );
        assert_eq!(
            input_control_action(&ctrl('x')),
            Some(InputControlAction::CutAll)
        );
        assert_eq!(
            input_control_action(&ctrl('v')),
            Some(InputControlAction::Paste)
        );
        assert_eq!(
            input_control_action(&ctrl('u')),
            Some(InputControlAction::ClearAll)
        );
    }

    #[test]
    fn input_control_ignores_plain_text_keys() {
        assert_eq!(input_control_action(&key(KeyCode::Char('a'))), None);
        assert_eq!(input_control_action(&key(KeyCode::Enter)), None);
    }

    #[test]
    fn scroll_toward_tail_recovers_follow_mode_at_bottom() {
        assert_eq!(scroll_toward_tail_offset(3, 6), (usize::MAX, true));
        assert_eq!(scroll_toward_tail_offset(0, 6), (usize::MAX, true));
        assert_eq!(scroll_toward_tail_offset(usize::MAX, 6), (usize::MAX, true));
    }

    #[test]
    fn scroll_toward_tail_keeps_offset_when_not_at_bottom() {
        assert_eq!(scroll_toward_tail_offset(20, 6), (14, false));
    }

    #[test]
    fn scroll_back_leaves_follow_mode() {
        assert_eq!(scroll_back_offset(usize::MAX, 6), (6, false));
        assert_eq!(scroll_back_offset(12, 6), (18, false));
    }

    #[test]
    fn update_prompt_accepts_yes_keys() {
        assert_eq!(
            update_prompt_action(&key(KeyCode::Char('y'))),
            UpdatePromptAction::Install
        );
        assert_eq!(
            update_prompt_action(&key(KeyCode::Char('Y'))),
            UpdatePromptAction::Install
        );
    }

    #[test]
    fn update_prompt_accepts_no_keys() {
        assert_eq!(
            update_prompt_action(&key(KeyCode::Char('n'))),
            UpdatePromptAction::Skip
        );
        assert_eq!(
            update_prompt_action(&key(KeyCode::Char('N'))),
            UpdatePromptAction::Skip
        );
        assert_eq!(
            update_prompt_action(&key(KeyCode::Esc)),
            UpdatePromptAction::Skip
        );
    }

    #[test]
    fn update_prompt_ignores_unrelated_keys() {
        assert_eq!(
            update_prompt_action(&key(KeyCode::Enter)),
            UpdatePromptAction::Ignore
        );
    }
}

fn handle_mouse(state: &mut AgentState, m: MouseEvent) {
    match m.kind {
        MouseEventKind::ScrollUp => {
            scroll_execution_back(state, SCROLL_WHEEL_STEP);
        }

        MouseEventKind::ScrollDown => {
            scroll_execution_toward_tail(state, SCROLL_WHEEL_STEP);
        }

        _ => {}
    }
}
