//! ui/draw.rs
//!
//! Root UI compositor.

use std::io;

use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

use crate::state::{AgentState, Focus};
use crate::ui::{diff, panels, status, execution};

/* ============================================================
   Public API
   ============================================================ */

pub fn draw_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<(Rect, Rect, Rect)> {
    let mut input_rect = Rect::default();
    let mut diff_rect = Rect::default();
    let mut exec_rect = Rect::default();

    terminal.draw(|f| {
        /* =====================================================
         * Root Layout
         * =================================================== */

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(14), // header + status (expanded)
                Constraint::Min(6),     // execution / diff / panel
                Constraint::Length(3),  // command input
            ])
            .split(f.size());

        /* =====================================================
         * Header + Status
         * =================================================== */

        status::render_status(f, layout[0], state);

        /* =====================================================
         * Main Execution Area
         * =================================================== */

        let mut rendered = false;

        // 1) Single-panel views (LLM output, test result)
        if panels::render_panel(f, layout[1], state) {
            rendered = true;
        }

        // 2) Diff view
        if !rendered {
            if let Some(idx) = state.ui.selected_diff {
                if let Some(delta) = &state.context.diff_analysis[idx].delta {
                    diff::render_side_by_side(f, layout[1], delta, state);
                    diff_rect = layout[1];
                    rendered = true;
                }
            }
        }

        // 3) Execution logs (DEFAULT)
        if !rendered {
            execution::render_execution(f, layout[1], state);
        }

        exec_rect = layout[1];

        /* =====================================================
         * Command Input (RESTORED)
         * =================================================== */

        let mut spans = vec![
            Span::styled("$_ ", Style::default().fg(Color::Cyan)),
            Span::styled(&state.ui.input, Style::default().fg(Color::White)),
        ];

        if let Some(ac) = &state.ui.autocomplete {
            if ac.starts_with(&state.ui.input) {
                let suffix = &ac[state.ui.input.len()..];
                if !suffix.is_empty() {
                    spans.push(Span::styled(
                        suffix,
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
        }

        let input = Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .title("COMMAND"),
        );

        input_rect = layout[2];
        f.render_widget(input, input_rect);

        if state.ui.focus == Focus::Input {
            f.set_cursor(
                input_rect.x + 4 + state.ui.input.len() as u16,
                input_rect.y + 1,
            );
        }
    })?;

    Ok((input_rect, diff_rect, exec_rect))
}
