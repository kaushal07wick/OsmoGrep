// src/main.rs
mod state;
mod logger;
mod git;
mod ui;
mod commands;
mod machine;
mod detectors;
use std::collections::VecDeque;

use std::{error::Error, io, time::Duration};

use crossterm::{
    event::{self, Event, KeyCode, MouseEventKind},
    execute,
    terminal::{
        enable_raw_mode,
        disable_raw_mode,
        EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use crossterm::event::{EnableMouseCapture, DisableMouseCapture};
use ratatui::{backend::CrosstermBackend, Terminal};

use state::{AgentState, Phase, Focus};
use commands::{handle_command, update_command_hints};
use machine::step;

fn main() -> Result<(), Box<dyn Error>> {
    /* ---------- terminal setup ---------- */

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    execute!(io::stdout(), EnableMouseCapture)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    /* ---------- state init ---------- */

    let mut state = AgentState {
        /* lifecycle */
        phase: Phase::Init,

        base_branch: None,
        original_branch: None,
        current_branch: None,
        agent_branch: None,

        /* input */
        input: String::new(),
        input_focused: true,

        /* command UX */
        history: Vec::new(),
        history_index: None,
        hint: None,
        autocomplete: None,

        /* logs (ring buffer) */
        logs: VecDeque::new(),

        /* analysis */
        diff_analysis: Vec::new(),

        /* status */
        language: None,
        framework: None,

        /* ui */
        spinner_tick: 0,

        /* diff viewer */
        selected_diff: None,
        diff_scroll: 0,
        diff_scroll_x: 0,
        in_diff_view: false,
        diff_side_by_side: false,
        focus: Focus::Input,

        /* execution panel */
        exec_scroll: 0,
        exec_paused: false,
    };


    /* ---------- event loop ---------- */

    loop {
        state.spinner_tick = state.spinner_tick.wrapping_add(1);

        let (input_rect, diff_rect, exec_rect) = ui::draw_ui(&mut terminal, &state)?;

        if event::poll(Duration::from_millis(120))? {
            match event::read()? {

                /* ================= DIFF (FOCUSED) ================= */
                Event::Key(k) if state.in_diff_view && state.focus == Focus::Diff => {
                    match k.code {
                        KeyCode::Up => state.diff_scroll = state.diff_scroll.saturating_sub(1),
                        KeyCode::Down => state.diff_scroll = state.diff_scroll.saturating_add(1),
                        KeyCode::Left => state.diff_scroll_x = state.diff_scroll_x.saturating_sub(4),
                        KeyCode::Right => state.diff_scroll_x = state.diff_scroll_x.saturating_add(4),
                        KeyCode::PageUp => state.diff_scroll = state.diff_scroll.saturating_sub(10),
                        KeyCode::PageDown => state.diff_scroll = state.diff_scroll.saturating_add(10),
                        KeyCode::Char('s') => {
                            state.diff_side_by_side = !state.diff_side_by_side;
                            state.diff_scroll = 0;
                        }
                        KeyCode::Esc => {
                            state.in_diff_view = false;
                            state.focus = Focus::Input;
                            state.diff_scroll = 0;
                        }
                        _ => {}
                    }
                }

                /* ================= EXECUTION (FOCUSED) ================= */
                Event::Key(k) if state.focus == Focus::Execution => {
                    match k.code {
                        KeyCode::Up => state.exec_scroll = state.exec_scroll.saturating_sub(1),
                        KeyCode::Down => state.exec_scroll = state.exec_scroll.saturating_add(1),
                        KeyCode::PageUp => state.exec_scroll = state.exec_scroll.saturating_sub(10),
                        KeyCode::PageDown => state.exec_scroll = state.exec_scroll.saturating_add(10),
                        _ => {}
                    }
                }

                /* ================= INPUT (FOCUSED) ================= */
                Event::Key(k) if state.focus == Focus::Input && state.input_focused => {
                    match k.code {
                        KeyCode::Char(c) => {
                            state.push_char(c);
                            update_command_hints(&mut state);
                        }
                        KeyCode::Backspace => {
                            state.backspace();
                            update_command_hints(&mut state);
                        }
                        KeyCode::Enter => {
                            let cmd = state.commit_input();
                            handle_command(&mut state, &cmd);
                            update_command_hints(&mut state);
                        }
                        KeyCode::Up => {
                            state.history_prev();
                            update_command_hints(&mut state);
                        }
                        KeyCode::Down => {
                            state.history_next();
                            update_command_hints(&mut state);
                        }
                        KeyCode::Tab => {
                            if let Some(ac) = state.autocomplete.clone() {
                                state.input = ac;
                                update_command_hints(&mut state);
                            }
                        }
                        KeyCode::Esc => {
                            state.input.clear();
                            state.clear_hint();
                            state.clear_autocomplete();
                        }
                        _ => {}
                    }
                }

                /* ================= GLOBAL ================= */
                Event::Key(k) => {
                    match k.code {
                        KeyCode::PageUp => {
                            if !state.diff_analysis.is_empty() {
                                let next = state.selected_diff.unwrap_or(0).saturating_sub(1);
                                state.selected_diff = Some(next);
                            }
                        }
                        KeyCode::PageDown => {
                            if !state.diff_analysis.is_empty() {
                                let next = state.selected_diff.unwrap_or(0) + 1;
                                if next < state.diff_analysis.len() {
                                    state.selected_diff = Some(next);
                                }
                            }
                        }
                        KeyCode::Enter => {
                            if state.selected_diff.is_some() {
                                state.in_diff_view = true;
                                state.focus = Focus::Diff;
                                state.diff_scroll = 0;
                            }
                        }
                        KeyCode::Esc => state.phase = Phase::Done,
                        _ => {}
                    }
                }

                /* ================= MOUSE ================= */
                Event::Mouse(m) => {
                    let pos = (m.column, m.row).into();

                    match m.kind {
                        MouseEventKind::Down(_) => {
                            if diff_rect.contains(pos) {
                                state.in_diff_view = true;
                                state.focus = Focus::Diff;
                            } else if exec_rect.contains(pos) {
                                state.focus = Focus::Execution; // ✅ only when inside exec
                            } else if input_rect.contains(pos) {
                                state.focus = Focus::Input;
                            }
                        }

                        MouseEventKind::ScrollUp if state.focus == Focus::Diff => {
                            state.diff_scroll = state.diff_scroll.saturating_sub(3);
                        }
                        MouseEventKind::ScrollDown if state.focus == Focus::Diff => {
                            state.diff_scroll = state.diff_scroll.saturating_add(3);
                        }

                        MouseEventKind::ScrollLeft if state.focus == Focus::Diff => {
                            state.diff_scroll_x = state.diff_scroll_x.saturating_sub(4);
                        }
                        MouseEventKind::ScrollRight if state.focus == Focus::Diff => {
                            state.diff_scroll_x = state.diff_scroll_x.saturating_add(4);
                        }

                        MouseEventKind::ScrollUp if state.focus == Focus::Execution => {
                            state.exec_scroll = state.exec_scroll.saturating_sub(3);
                        }
                        MouseEventKind::ScrollDown if state.focus == Focus::Execution => {
                            state.exec_scroll = state.exec_scroll.saturating_add(3);
                        }

                        _ => {}
                    }
                }


                _ => {}
            }

        }

        /* ---------- state machine ---------- */

        step(&mut state);

        if let Phase::Done = state.phase {
            break;
        }
    }

    /* ---------- teardown ---------- */

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    execute!(terminal.backend_mut(), DisableMouseCapture)?;
    terminal.show_cursor()?;

    Ok(())
}
