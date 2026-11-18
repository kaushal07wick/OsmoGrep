use crate::state::{AgentState, InputMode};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
const SCROLL_LINE_STEP: usize = 3;
const SCROLL_PAGE_STEP: usize = 12;
const SCROLL_WHEEL_STEP: usize = 1;

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
    if state.ui.pending_update.is_some() {
        return;
    }
    if state.ui.experience.is_some() {
        crate::ui::experience::paste(state, text);
        return;
    }
    if state.ui.panel.is_some() {
        crate::ui::workbench::paste(state, text);
        return;
    }
    let text = normalize_paste_text(text);
    if state.ui.input_mode != InputMode::ApiKey {
        if let Some(path) = crate::attachments::dropped_image(&text, &state.repo_root) {
            if let Err(error) = crate::attachments::attach(state, &path) {
                state.logs.push(crate::state::LogLevel::Warn, error);
            }
            return;
        }
    }
    state.insert_text(&text);
}

fn normalize_paste_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn handle_key(state: &mut AgentState, k: KeyEvent) {
    if matches!(k.code, KeyCode::Esc) && state.ui.agent_running && state.ui.experience.is_none() {
        request_agent_cancel(state);
        return;
    }

    if let Some(pending) = state.ui.pending_permission.take() {
        if matches!(k.code, KeyCode::Up | KeyCode::Down) {
            state.ui.permission_selected = if k.code == KeyCode::Up {
                state.ui.permission_selected.saturating_sub(1)
            } else {
                (state.ui.permission_selected + 1).min(2)
            };
            state.ui.pending_permission = Some(pending);
            return;
        }
        let code = if k.code == KeyCode::Enter {
            KeyCode::Char(['y', 'n', 'a'][state.ui.permission_selected.min(2)])
        } else {
            k.code
        };
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let _ = pending.reply_tx.send(true);
                crate::logger::log(
                    state,
                    crate::state::LogLevel::Info,
                    format!("Approved {} {}", pending.tool_name, pending.args_summary),
                );
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                state.discard_diff_preview();
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

    if let Some(update) = state.ui.pending_update.as_mut() {
        if update.installing {
            return;
        }
        if update.progress.stage == "ready" {
            if matches!(
                k.code,
                KeyCode::Enter | KeyCode::Char('r' | 'R' | 'u' | 'U')
            ) {
                state.ui.update_install_requested = true;
            }
            return;
        }
        match k.code {
            KeyCode::Up => {
                update.selected = update.selected.saturating_sub(1);
                return;
            }
            KeyCode::Down => {
                update.selected = (update.selected + 1).min(2);
                return;
            }
            _ => {}
        }
        let action =
            if matches!(k.code, KeyCode::Char('r' | 'R')) && update.progress.stage == "failed" {
                UpdatePromptAction::Install
            } else if k.code == KeyCode::Enter {
                [
                    UpdatePromptAction::Install,
                    UpdatePromptAction::Later,
                    UpdatePromptAction::Skip,
                ][update.selected.min(2)]
            } else {
                update_prompt_action(&k)
            };
        match action {
            UpdatePromptAction::Install => {
                state.ui.update_install_requested = true;
            }
            UpdatePromptAction::Skip => {
                state.ui.update_skip_requested = true;
            }
            UpdatePromptAction::Later => {
                state.ui.update_later_requested = true;
            }
            UpdatePromptAction::Ignore => {}
        }
        return;
    }

    if crate::ui::experience::handle_key(state, k) {
        return;
    }
    if crate::ui::workbench::handle_key(state, k) {
        return;
    }
    let editing = crate::ui::experience::editing(state);
    let k = if editing || state.ui.input_mode == InputMode::ApiKey {
        k
    } else {
        match crate::ui::keymap::resolve(&state.preferences, k) {
            crate::ui::keymap::Resolved::Sessions => {
                let model = state.ui.presentation_model.clone();
                crate::ui::workbench::open(state, 10, model.as_ref());
                return;
            }
            crate::ui::keymap::Resolved::Command(cmd) => {
                state.ui.requested_command = Some(cmd.into());
                return;
            }
            crate::ui::keymap::Resolved::Key(key) => key,
            crate::ui::keymap::Resolved::Ignore => return,
            crate::ui::keymap::Resolved::Unchanged => k,
        }
    };
    if !editing && k.code == KeyCode::Char('t') && k.modifiers.contains(KeyModifiers::CONTROL) {
        state.ui.expand_tools = !state.ui.expand_tools;
        return;
    }
    let palette_active = !state.ui.command_items.is_empty();
    let navigation = matches!(
        k.code,
        KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End
    );
    if navigation {
        if k.modifiers.contains(KeyModifiers::SHIFT) {
            state
                .ui
                .input_selection_anchor
                .get_or_insert(state.ui.input_cursor);
        } else {
            if let Some(range) = state.input_selection() {
                if matches!(k.code, KeyCode::Left | KeyCode::Right) {
                    state.ui.input_cursor = if k.code == KeyCode::Left {
                        range.start
                    } else {
                        range.end
                    };
                    state.ui.input_all_selected = false;
                    state.ui.input_selection_anchor = None;
                    return;
                }
            }
            state.ui.input_selection_anchor = None;
        }
    }

    if let Some(action) = input_control_action(&k) {
        apply_input_control_action(state, action);
        return;
    }

    match k.code {
        KeyCode::Left
            if k.modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
        {
            state.move_cursor_word(-1)
        }
        KeyCode::Right
            if k.modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
        {
            state.move_cursor_word(1)
        }
        KeyCode::Char('b') if k.modifiers.contains(KeyModifiers::ALT) => state.move_cursor_word(-1),
        KeyCode::Char('f') if k.modifiers.contains(KeyModifiers::ALT) => state.move_cursor_word(1),
        KeyCode::Up if k.modifiers.contains(KeyModifiers::CONTROL) => {
            scroll_execution_back(state, SCROLL_LINE_STEP)
        }
        KeyCode::Down if k.modifiers.contains(KeyModifiers::CONTROL) => {
            scroll_execution_toward_tail(state, SCROLL_LINE_STEP)
        }
        KeyCode::Home if k.modifiers.contains(KeyModifiers::CONTROL) => state.ui.input_cursor = 0,
        KeyCode::End if k.modifiers.contains(KeyModifiers::CONTROL) => {
            state.ui.input_cursor = state.ui.input.len()
        }
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
            if !editing
                && state.ui.input_mode != InputMode::ApiKey
                && c == '@'
                && !k
                    .modifiers
                    .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
                && (state.ui.input_cursor == 0
                    || state.ui.input[..state.ui.input_cursor]
                        .chars()
                        .last()
                        .is_some_and(char::is_whitespace))
            {
                state.push_char(c);
                crate::ui::experience::open_context(state, true);
                return;
            }

            state.push_char(c);
            if !editing
                && c == '$'
                && state.ui.input_mode != InputMode::ApiKey
                && state.ui.input_mode != InputMode::Shell
            {
                let prefix = &state.ui.input[..state.ui.input_cursor.saturating_sub(1)];
                if prefix.is_empty() || prefix.chars().last().is_some_and(char::is_whitespace) {
                    crate::ui::workbench::open(state, 6, None);
                }
            }
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
                if !state.ui.agent_running && !state.ui.queued_agent_prompts.is_empty() {
                    state.ui.queue_paused = false;
                }
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
            if !state.move_cursor_up() && !editing && !k.modifiers.contains(KeyModifiers::SHIFT) {
                state.history_prev();
            }
        }

        KeyCode::Down if !palette_active => {
            if !state.move_cursor_down() && !editing && !k.modifiers.contains(KeyModifiers::SHIFT) {
                state.history_next();
            }
        }

        /* ---------- Autocomplete ---------- */
        KeyCode::Tab => {
            if state.ui.agent_running && state.ui.input_mode != InputMode::ApiKey {
                let raw = state.ui.input.clone();
                if !raw.trim().is_empty() {
                    let raw = state.commit_input();
                    state.ui.queued_agent_prompts.push_back(raw);
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
                state.ui.transcript_selection = None;
                state.ui.input_all_selected = false;
                state.ui.input_selection_anchor = None;
                if state.ui.input_mode == InputMode::ApiKey {
                    state.clear_input();
                    state.ui.input_mode = InputMode::AgentText;
                    state.ui.input_masked = false;
                    state.ui.input_placeholder = None;
                }
            }
        }

        _ => {}
    }
}

fn request_agent_cancel(state: &mut AgentState) {
    state.ui.queue_paused = true;
    state.discard_diff_preview();
    if state.ui.cancelling {
        return;
    }
    state.ui.cancelling = true;
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
    let (offset, _) = scroll_back_offset(state.ui.exec_scroll, step);
    let offset = offset.min(state.ui.exec_scroll_max);
    state.ui.exec_scroll = offset;
    state.ui.follow_tail = false;
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
    LineEnd,
    KillToLineEnd,
    Yank,
    DeletePreviousWord,
    DeleteForward,
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
        'd' => Some(InputControlAction::DeleteForward),
        'e' => Some(InputControlAction::LineEnd),
        'k' => Some(InputControlAction::KillToLineEnd),
        'o' => Some(InputControlAction::CopyOutput),
        'x' => Some(InputControlAction::CutAll),
        'v' => Some(InputControlAction::Paste),
        'u' => Some(InputControlAction::ClearAll),
        'w' => Some(InputControlAction::DeletePreviousWord),
        'y' => Some(InputControlAction::Yank),
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
                match crate::clipboard::copy_text_to_clipboard(&state.ui.input_clipboard) {
                    Ok(_) => {
                        state.ui.copy_notice = Some((
                            state.ui.input_clipboard.chars().count(),
                            std::time::Instant::now(),
                        ))
                    }
                    Err(error) => crate::logger::log_status(state, format!("Copy failed: {error}")),
                }
            } else if state.ui.agent_running {
                request_agent_cancel(state);
            }
        }
        InputControlAction::CopyOutput => {
            crate::commands::copy_latest_output(state);
        }
        InputControlAction::CutAll => {
            if state.cut_input() {
                match crate::clipboard::copy_text_to_clipboard(&state.ui.input_clipboard) {
                    Ok(_) => {
                        state.ui.copy_notice = Some((
                            state.ui.input_clipboard.chars().count(),
                            std::time::Instant::now(),
                        ))
                    }
                    Err(error) => crate::logger::log_status(state, format!("Copy failed: {error}")),
                }
            }
        }
        InputControlAction::Paste => {
            crate::attachments::paste(state);
        }
        InputControlAction::ClearAll => {
            if state.clear_input() {
                crate::logger::log_status(state, "Cleared input.");
            }
        }
        InputControlAction::LineEnd => {
            state.move_cursor_line_end();
        }
        InputControlAction::KillToLineEnd => {
            if state.kill_to_line_end() {
                crate::logger::log_status(state, "Killed input.");
            }
        }
        InputControlAction::Yank => {
            if state.paste_input() {
                crate::logger::log_status(state, "Yanked input.");
            }
        }
        InputControlAction::DeletePreviousWord => {
            if state.delete_previous_word() {
                crate::logger::log_status(state, "Deleted previous word.");
            }
        }
        InputControlAction::DeleteForward => {
            state.delete_forward();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdatePromptAction {
    Install,
    Later,
    Skip,
    Ignore,
}

fn update_prompt_action(k: &KeyEvent) -> UpdatePromptAction {
    match k.code {
        KeyCode::Char('y' | 'Y' | 'u' | 'U') => UpdatePromptAction::Install,
        KeyCode::Char('n' | 'N' | 's' | 'S') => UpdatePromptAction::Skip,
        KeyCode::Char('l' | 'L') | KeyCode::Esc => UpdatePromptAction::Later,
        _ => UpdatePromptAction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        handle_event, handle_key, handle_paste, input_control_action, scroll_back_offset,
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

    #[test]
    fn shift_right_selects_text_and_typing_replaces_it() {
        let mut state = agent_state();
        state.insert_text("one 界two");
        state.ui.input_cursor = "one ".len();
        for _ in 0..4 {
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT),
            );
        }
        assert_eq!(&state.ui.input[state.input_selection().unwrap()], "界two");
        handle_key(&mut state, key(KeyCode::Char('x')));
        assert_eq!(state.ui.input, "one x");
    }

    #[test]
    fn command_left_opens_sessions_directly_and_preserves_draft() {
        let mut state = agent_state();
        state.insert_text("keep this 界 draft");
        let cursor = state.ui.input_cursor;
        handle_key(&mut state, key(KeyCode::F(6)));
        assert!(state.ui.panel.is_none());
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Left, KeyModifiers::SUPER),
        );
        assert_eq!(state.ui.panel.as_ref().unwrap().section, 10);
        assert_eq!(state.ui.input, "keep this 界 draft");
        assert_eq!(state.ui.input_cursor, cursor);
        assert!(state.ui.requested_command.is_none());
        assert!(!state.ui.execution_pending);
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.ui.panel.is_none());
        assert!(!state.ui.should_exit);
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn agent_state() -> AgentState {
        AgentState {
            session: Default::default(),
            ui: UiState::default(),
            logs: LogBuffer::new(),
            session_changes: Vec::new(),
            reviewed_change_count: 0,
            undo_stack: Vec::new(),
            preferences: crate::preferences::Preferences::default(),
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
            input_control_action(&ctrl('d')),
            Some(InputControlAction::DeleteForward)
        );
        assert_eq!(
            input_control_action(&ctrl('e')),
            Some(InputControlAction::LineEnd)
        );
        assert_eq!(
            input_control_action(&ctrl('k')),
            Some(InputControlAction::KillToLineEnd)
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
        assert_eq!(
            input_control_action(&ctrl('w')),
            Some(InputControlAction::DeletePreviousWord)
        );
        assert_eq!(
            input_control_action(&ctrl('y')),
            Some(InputControlAction::Yank)
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
    fn queued_inputs_are_fifo_and_escape_pauses_without_dropping_them() {
        let mut state = crate::init_state();
        state.ui.agent_running = true;
        for text in ["first", "second"] {
            state.ui.input = text.into();
            state.ui.input_cursor = text.len();
            handle_key(&mut state, key(KeyCode::Tab));
        }
        assert_eq!(
            state
                .ui
                .queued_agent_prompts
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.ui.queue_paused);
        assert_eq!(state.ui.queued_agent_prompts.len(), 2);
        state.ui.agent_running = false;
        handle_key(&mut state, key(KeyCode::Enter));
        assert!(!state.ui.queue_paused);
        assert_eq!(
            state.ui.queued_agent_prompts.pop_front().as_deref(),
            Some("first")
        );
    }

    #[test]
    fn tab_during_api_key_entry_never_sends_the_key_to_the_agent() {
        let mut state = crate::init_state();
        state.ui.agent_running = true;
        state.ui.input_mode = crate::state::InputMode::ApiKey;
        state.ui.input = "example-key".into();
        handle_key(&mut state, key(KeyCode::Tab));
        assert!(state.ui.queued_agent_prompts.is_empty());
        assert_eq!(state.ui.input, "example-key");
    }

    #[test]
    fn escape_never_exits_an_idle_session_or_discards_a_draft() {
        let mut state = crate::init_state();
        state.ui.input = "keep draft".into();
        for _ in 0..3 {
            handle_key(&mut state, key(KeyCode::Esc));
        }
        assert!(!state.ui.should_exit);
        assert_eq!(state.ui.input, "keep draft");
        state.ui.input_mode = crate::state::InputMode::ApiKey;
        state.ui.input = "secret".into();
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(!state.ui.should_exit);
        assert!(state.ui.input.is_empty());
        assert_eq!(state.ui.input_mode, crate::state::InputMode::AgentText);
    }

    #[test]
    fn transcript_scroll_cannot_accumulate_past_the_oldest_visible_line() {
        let mut state = crate::init_state();
        state.ui.exec_scroll_max = 20;
        super::scroll_execution_back(&mut state, 100);
        assert_eq!(state.ui.exec_scroll, 20);
        super::scroll_execution_toward_tail(&mut state, 12);
        assert_eq!(state.ui.exec_scroll, 8);
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
    fn scrolling_back_pauses_future_output_even_before_the_viewport_is_full() {
        let mut state = crate::init_state();
        state.ui.exec_scroll_max = 0;
        super::scroll_execution_back(&mut state, 1);
        assert!(!state.ui.follow_tail);
        assert_eq!(state.ui.exec_scroll, 0);
        handle_key(&mut state, key(KeyCode::Char('x')));
        assert_eq!(state.ui.input, "x");
        assert!(!state.ui.follow_tail);
    }

    #[test]
    fn update_menu_defaults_to_later_and_never_installs_without_a_choice() {
        let mut state = agent_state();
        state.insert_text("preserved draft");
        state.ui.pending_update = Some(crate::state::PendingUpdate {
            current_version: "0.3.4".into(),
            latest_version: "0.4.0".into(),
            asset_url: String::new(),
            asset_name: String::new(),
            checksum_url: String::new(),
            notes: vec!["New sessions".into()],
            selected: 1,
            installing: false,
            progress: Default::default(),
        });
        handle_key(&mut state, key(KeyCode::Enter));
        assert!(state.ui.update_later_requested);
        assert!(!state.ui.update_install_requested);
        assert!(!state.ui.update_skip_requested);
        state.ui.update_later_requested = false;
        handle_key(&mut state, key(KeyCode::Down));
        handle_key(&mut state, key(KeyCode::Enter));
        assert!(state.ui.update_skip_requested);
        state.ui.update_skip_requested = false;
        handle_key(&mut state, key(KeyCode::Up));
        handle_key(&mut state, key(KeyCode::Up));
        handle_key(&mut state, key(KeyCode::Enter));
        assert!(state.ui.update_install_requested);
        state.ui.update_install_requested = false;
        state.ui.pending_update.as_mut().unwrap().installing = true;
        handle_paste(&mut state, "must not reach chat");
        handle_key(&mut state, key(KeyCode::Char('u')));
        assert!(!state.ui.update_install_requested);
        assert_eq!(state.ui.input, "preserved draft");
        let update = state.ui.pending_update.as_mut().unwrap();
        update.installing = false;
        update.progress.stage = "ready";
        handle_key(&mut state, key(KeyCode::Enter));
        assert!(state.ui.update_install_requested);
        assert_eq!(state.ui.input, "preserved draft");
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
            UpdatePromptAction::Later
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
    if state.ui.pending_update.is_some() {
        return;
    }
    if state.ui.experience.is_some() && state.ui.pending_permission.is_none() {
        let code = match m.kind {
            MouseEventKind::ScrollUp => Some(KeyCode::PageUp),
            MouseEventKind::ScrollDown => Some(KeyCode::PageDown),
            _ => None,
        };
        if let Some(code) = code {
            crate::ui::experience::handle_key(state, KeyEvent::new(code, KeyModifiers::NONE));
            return;
        }
    }
    if state.ui.panel.is_some()
        && state.ui.pending_permission.is_none()
        && state.ui.pending_update.is_none()
    {
        let code = match m.kind {
            MouseEventKind::ScrollUp => Some(KeyCode::Up),
            MouseEventKind::ScrollDown => Some(KeyCode::Down),
            _ => None,
        };
        if let Some(code) = code {
            crate::ui::workbench::handle_key(state, KeyEvent::new(code, KeyModifiers::NONE));
            return;
        }
    }
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
