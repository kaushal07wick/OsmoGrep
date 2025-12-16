// src/ui.rs
use std::io;

use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

use crate::state::{AgentState, LogLevel};

pub fn draw_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<Rect> {
    let mut input_rect = Rect::default();

    terminal.draw(|f| {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),  // header
                Constraint::Length(3),  // status
                Constraint::Min(8),     // execution log
                Constraint::Length(3),  // command input
            ])
            .split(f.size());

        /* ================= Header ================= */

        let header = Paragraph::new("OSMOGREP — AI E2E Testing Agent")
            .style(Style::default().add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));

        /* ================= Status ================= */

        let status = Paragraph::new(format!(
            "Phase: {:?}\nBase: {}",
            state.phase,
            state
                .base_branch
                .clone()
                .unwrap_or_else(|| "unknown".into())
        ))
        .block(Block::default().borders(Borders::ALL).title("Status"));

        /* ================= Logs ================= */

        let logs: Vec<Line> = state
            .logs
            .iter()
            .rev()
            .take(30)
            .rev()
            .map(|l| {
                let color = match l.level {
                    LogLevel::Info => Color::White,
                    LogLevel::Success => Color::Green,
                    LogLevel::Warn => Color::Yellow,
                    LogLevel::Error => Color::Red,
                };

                Line::from(Span::styled(&l.text, Style::default().fg(color)))
            })
            .collect();

        let log_view = Paragraph::new(logs)
            .block(Block::default().borders(Borders::ALL).title("Execution"));

        /* ================= Command Box ================= */

        let focused_style = Style::default().fg(Color::Cyan);
        let unfocused_style = Style::default().fg(Color::DarkGray);
        let ghost_style = Style::default().fg(Color::DarkGray);

        let input_style = if state.input_focused {
            focused_style
        } else {
            unfocused_style
        };

        // Build command line spans
        let mut spans: Vec<Span> = Vec::new();

        // Leading slash (command mode)
        spans.push(Span::styled("/", input_style));

        // Actual input
        spans.push(Span::styled(&state.input, input_style));

        // Ghost autocomplete (fish/zsh style)
        if let Some(ac) = &state.autocomplete {
            if ac.starts_with(&state.input) {
                let ghost = &ac[state.input.len()..];
                if !ghost.is_empty() {
                    spans.push(Span::styled(ghost, ghost_style));
                }
            }
        }

        let input = Paragraph::new(Line::from(spans))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Command"),
            );

        input_rect = layout[3];

        /* ================= Render ================= */

        f.render_widget(header, layout[0]);
        f.render_widget(status, layout[1]);
        f.render_widget(log_view, layout[2]);
        f.render_widget(input, layout[3]);

        /* ================= Cursor ================= */

        if state.input_focused {
            // Cursor after: border + "/" + input text
            let x = input_rect.x + 2 + 1 + state.input.len() as u16;
            let y = input_rect.y + 1;
            f.set_cursor(x, y);
        }
    })?;

    Ok(input_rect)
}
