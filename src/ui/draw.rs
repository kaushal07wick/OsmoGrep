//! ui/draw.rs
//!
//! Root UI compositor.

use std::io;

use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, BorderType},
    Terminal,
};

use crate::state::{AgentState, Focus};
use crate::ui::{diff, panels, status, execution};

pub fn draw_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<(Rect, Rect, Rect)> {
    let mut input_rect = Rect::default();
    let mut diff_rect = Rect::default();
    let mut exec_rect = Rect::default();

    terminal.draw(|f| {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(14), // header + status
                Constraint::Min(6),     // main content
                Constraint::Length(3),  // FIXED: command box needs space
            ])
            .split(f.size());

        /* ================= STATUS ================= */
        status::render_status(f, layout[0], state);

        /* ================= MAIN PANEL ================= */
        let mut rendered = false;

        if panels::render_panel(f, layout[1], state) {
            rendered = true;
        }

        if !rendered {
            if let Some(idx) = state.ui.selected_diff {
                if let Some(delta) = &state.context.diff_analysis[idx].delta {
                    diff::render_side_by_side(f, layout[1], delta, state);
                    diff_rect = layout[1];
                    rendered = true;
                }
            }
        }

        if !rendered {
            execution::render_execution(f, layout[1], state);
        }

        exec_rect = layout[1];

        /* ================= COMMAND BOX ================= */
        let prompt = "$_ ";

        let mut spans = vec![
            Span::styled(prompt, Style::default().fg(Color::Cyan)),
            Span::styled(&state.ui.input, Style::default().fg(Color::White)),
        ];

        if let Some(ac) = &state.ui.autocomplete {
            if ac.starts_with(&state.ui.input) {
                let suffix = &ac[state.ui.input.len()..];
                if !suffix.is_empty() {
                    spans.push(Span::styled(
                        suffix,
                        Style::default().fg(Color::Gray), // FIXED visibility
                    ));
                }
            }
        }

        let input = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title("COMMAND")
            .title_alignment(ratatui::layout::Alignment::Center),
    );


        input_rect = layout[2];
        f.render_widget(input, input_rect);

        /* ================= CURSOR ================= */
        if state.ui.focus == Focus::Input {
            let cursor_x =
                input_rect.x + prompt.len() as u16 + state.ui.input.len() as u16;
            let cursor_y = input_rect.y + 1;
            f.set_cursor(cursor_x, cursor_y);
        }
    })?;

    Ok((input_rect, diff_rect, exec_rect))
}
