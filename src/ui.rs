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

        /* ---------- Header ---------- */

        let header = Paragraph::new("OSMOGREP — AI E2E Testing Agent")
            .style(Style::default().add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));

        /* ---------- Status ---------- */

        let status = Paragraph::new(format!(
            "Phase: {:?}\nBase: {}",
            state.phase,
            state
                .base_branch
                .clone()
                .unwrap_or_else(|| "unknown".into())
        ))
        .block(Block::default().borders(Borders::ALL).title("Status"));

        /* ---------- Logs ---------- */

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

        /* ---------- Command Box ---------- */

        let focused_style = Style::default().fg(Color::Cyan);
        let unfocused_style = Style::default().fg(Color::DarkGray);

        let input_style = if state.input_focused {
            focused_style
        } else {
            unfocused_style
        };

        // Always show `/` to signal command mode
        let input_text = format!("/{}", state.input);

        let input = Paragraph::new(input_text)
            .style(input_style)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Command"),
            );

        input_rect = layout[3];

        /* ---------- Render ---------- */

        f.render_widget(header, layout[0]);
        f.render_widget(status, layout[1]);
        f.render_widget(log_view, layout[2]);
        f.render_widget(input, layout[3]);

        /* ---------- Cursor ---------- */

        if state.input_focused {
            // Cursor after `/` + input text
            let x = input_rect.x + 2 + 1 + state.input.len() as u16;
            let y = input_rect.y + 1;
            f.set_cursor(x, y);
        }
    })?;

    Ok(input_rect)
}
