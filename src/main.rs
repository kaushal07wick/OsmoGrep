// src/main.rs
mod state;
mod logger;
mod git;
mod ui;
mod commands;
mod machine;

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

use ratatui::{backend::CrosstermBackend, Terminal};

use state::{AgentState, Phase};
use commands::{handle_command, update_command_hints};
use machine::step;

fn main() -> Result<(), Box<dyn Error>> {
    /* ---------- terminal setup ---------- */

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    /* ---------- state init ---------- */

    let mut state = AgentState {
        phase: Phase::Init,
        base_branch: None,
        original_branch: None,
        agent_branch: None,

        input: String::new(),
        input_focused: true,

        history: Vec::new(),
        history_index: None,
        hint: None,
        autocomplete: None,

        logs: Vec::new(),
    };

    /* ---------- event loop ---------- */

    loop {
        let input_rect = ui::draw_ui(&mut terminal, &state)?;

        if event::poll(Duration::from_millis(120))? {
            match event::read()? {
                /* ================= Keyboard ================= */

                Event::Key(k) if state.input_focused => match k.code {
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
                },

                /* ================= Mouse ================= */

                Event::Mouse(m) if matches!(m.kind, MouseEventKind::Down(_)) => {
                    state.input_focused =
                        input_rect.contains((m.column, m.row).into());
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
    terminal.show_cursor()?;

    Ok(())
}
