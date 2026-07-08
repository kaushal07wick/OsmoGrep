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
        Event::Paste(text) => handle_paste(state, &text),
        Event::Mouse(m) => handle_mouse(state, m),
        _ => {}
    }
}

fn handle_paste(state: &mut AgentState, text: &str) {
    let text = normalize_paste_text(text);
    state.insert_text(&text);
}

fn normalize_paste_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn handle_key(state: &mut AgentState, k: KeyEvent) {
    if matches!(k.code, KeyCode::Esc) && state.ui.agent_running {
        request_agent_cancel(state);
        return;
    }

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
                state.ui.input_cursor = state.ui.input.len();
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
            state.delete_forward();
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
            if !state.move_cursor_up() {
                state.history_prev();
            }
        }

        KeyCode::Down if !palette_active => {
            if !state.move_cursor_down() {
                state.history_next();
            }
        }

        /* ---------- Autocomplete ---------- */
        KeyCode::Tab => {
            if state.ui.agent_running {
                let raw = state.ui.input.trim();
                if !raw.is_empty() {
                    state.ui.queued_agent_prompt = Some(raw.to_string());
                    state.ui.input.clear();
                    state.ui.input_cursor = 0;
                    state.ui.input_all_selected = false;
                    crate::logger::log_status(state, "Queued follow-up prompt (tab).");
                    return;
                }
            }
            if let Some(ac) = &state.ui.autocomplete {
                if ac.starts_with(&state.ui.input) {
                    let suffix = &ac[state.ui.input.len()..];
                    state.ui.input.push_str(suffix);
                    state.ui.input_cursor = state.ui.input.len();
                } else {
                    state.ui.input = ac.clone();
                    state.ui.input_cursor = state.ui.input.len();
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
            if state.ui.input.is_empty() {
                state.ui.exec_scroll = usize::MAX;
                state.ui.follow_tail = true;
            } else {
                state.move_cursor_line_end();
            }
        }

        KeyCode::Home => {
            state.move_cursor_line_start();
        }

        KeyCode::Left => {
            state.move_cursor_left();
        }

        KeyCode::Right => {
            state.move_cursor_right();
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

fn request_agent_cancel(state: &mut AgentState) {
    state.ui.cancel_requested = true;
    state.ui.command_items.clear();
    state.ui.command_selected = 0;

    if let Some(pending) = state.ui.pending_permission.take() {
        let _ = pending.reply_tx.send(false);
        crate::logger::log(
            state,
            crate::state::LogLevel::Warn,
            format!(
                "Cancelled pending {} {}",
                pending.tool_name, pending.args_summary
            ),
        );
    }

    crate::logger::log_status(state, "Cancel requested.");
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
    CopyOutput,
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
        'o' => Some(InputControlAction::CopyOutput),
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
        InputControlAction::CopyOutput => {
            crate::commands::copy_latest_output(state);
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
        handle_event, handle_key, input_control_action, scroll_back_offset,
        scroll_toward_tail_offset, update_prompt_action, InputControlAction, UpdatePromptAction,
    };
    use crate::state::{
        AgentState, ConversationHistory, LogBuffer, PermissionProfile, UiAccent, UiDensity,
        UiState, UiTheme, UsageStats, VoiceState,
    };
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Rect;
    use std::{path::PathBuf, sync::mpsc, time::Instant};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn agent_state() -> AgentState {
        AgentState {
            ui: UiState::default(),
            logs: LogBuffer::new(),
            session_changes: Vec::new(),
            reviewed_change_count: 0,
            undo_stack: Vec::new(),
            usage: UsageStats::default(),
            steer: None,
            auto_eval: false,
            permission_profile: PermissionProfile::WorkspaceAuto,
            jobs: Vec::new(),
            job_queue: Vec::new(),
            next_job_id: 1,
            plan_items: Vec::new(),
            session_name: None,
            theme: UiTheme::default(),
            accent: UiAccent::default(),
            density: UiDensity::default(),
            plan_mode: false,
            started_at: Instant::now(),
            repo_root: PathBuf::from("."),
            voice: VoiceState::default(),
            conversation: ConversationHistory::new(),
        }
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
            input_control_action(&ctrl('o')),
            Some(InputControlAction::CopyOutput)
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
    fn paste_event_bulk_inserts_normalized_text_at_cursor() {
        let mut state = agent_state();
        state.ui.input = "ab".to_string();
        state.ui.input_cursor = 1;

        handle_event(
            &mut state,
            Event::Paste("x\r\ny\rz".to_string()),
            Rect::default(),
            Rect::default(),
            Rect::default(),
        );

        assert_eq!(state.ui.input, "ax\ny\nzb");
        assert_eq!(state.ui.input_cursor, "ax\ny\nz".len());
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

    #[test]
    fn esc_cancels_running_agent_instead_of_exiting() {
        let mut state = agent_state();
        state.ui.agent_running = true;

        handle_key(&mut state, key(KeyCode::Esc));

        assert!(state.ui.cancel_requested);
        assert!(!state.ui.should_exit);
    }

    #[test]
    fn esc_denies_pending_permission_while_cancelling_agent() {
        let mut state = agent_state();
        state.ui.agent_running = true;
        let (tx, rx) = mpsc::channel();
        state.ui.pending_permission = Some(crate::state::PendingPermission {
            tool_name: "patch".to_string(),
            args_summary: "README.md".to_string(),
            reply_tx: tx,
        });

        handle_key(&mut state, key(KeyCode::Esc));

        assert!(state.ui.cancel_requested);
        assert!(state.ui.pending_permission.is_none());
        assert_eq!(rx.try_recv(), Ok(false));
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
