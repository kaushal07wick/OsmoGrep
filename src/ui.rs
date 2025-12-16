use std::io;

use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

use crate::state::{AgentState, LogLevel, Phase};

/* ================= Helpers ================= */

fn spinner(frame: usize) -> &'static str {
    const FRAMES: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];
    FRAMES[frame % FRAMES.len()]
}

fn phase_glyph(phase: &Phase) -> (&'static str, Color) {
    match phase {
        Phase::Idle => ("●", Color::Yellow),
        Phase::ExecuteAgent => ("▶", Color::Cyan),
        Phase::CreateNewAgent => ("＋", Color::Cyan),
        Phase::Rollback => ("↩", Color::Magenta),
        Phase::Done => ("✔", Color::Green),
        _ => ("●", Color::DarkGray),
    }
}

fn language_badge(lang: &str) -> (&'static str, Color) {
    match lang {
        "Rust" => ("🦀 Rust", Color::Cyan),
        "Python" => ("🐍 Python", Color::Yellow),
        "Go" => ("🐹 Go", Color::Blue),
        "TypeScript" => ("📘 TypeScript", Color::Blue),
        "JavaScript" => ("📗 JavaScript", Color::Yellow),
        "Java" => ("☕ Java", Color::Red),
        "Ruby" => ("💎 Ruby", Color::Magenta),
        "Rails" => ("🚄 Rails", Color::Red),
        "CSharp" => ("♯ C#", Color::Green),
        "Cpp" => ("🧠 C++", Color::Blue),
        "C" => ("🧱 C", Color::Blue),
        _ => ("❓ Unknown", Color::DarkGray),
    }
}

fn framework_badge(fw: &str) -> (&'static str, Color) {
    match fw {
        "CargoTest" => ("🦀🧪 Cargo", Color::Cyan),
        "Pytest" => ("🐍🧪 Pytest", Color::Yellow),
        "Unittest" => ("🐍📦 Unittest", Color::Yellow),
        "Jest" => ("🃏 Jest", Color::Magenta),
        "Vitest" => ("⚡ Vitest", Color::Cyan),
        "GoTest" => ("🐹🧪 Go test", Color::Blue),
        "JUnit" => ("☕🧪 JUnit", Color::Red),
        "TestNG" => ("☕⚙ TestNG", Color::Red),
        "None" => ("⚪ No tests", Color::DarkGray),
        _ => ("❓ Unknown", Color::DarkGray),
    }
}

/* ================= UI ================= */

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
                Constraint::Length(7), // ASCII header
                Constraint::Length(6), // status
                Constraint::Min(8),    // execution log
                Constraint::Length(3), // command
            ])
            .split(f.size());

        /* ================= HEADER ================= */

        let header_lines = [
            " ██████╗  ██████╗ ███╗   ███╗  ██████╗  ██████╗ ██████╗ ███████╗██████╗ ",
            "██╔═══██╗██╔════╝ ████╗ ████║ ██╔═══██╗██╔════╝ ██╔══██╗██╔════╝██╔══██╗",
            "██║   ██║███████╗ ██╔████╔██║ ██║   ██║██║  ███╗██████╔╝█████╗  ██████╔╝",
            "██║   ██║╚════██║ ██║╚██╔╝██║ ██║   ██║██║   ██║██╔══██╗██╔══╝  ██╔═══╝ ",
            "╚██████╔╝███████║ ██║ ╚═╝ ██║ ╚██████╔╝╚██████╔╝██║  ██║███████╗██║     ",
            " ╚═════╝ ╚══════╝ ╚═╝     ╚═╝  ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝     ",
        ];

        let header = Paragraph::new(
            header_lines
                .iter()
                .map(|l| {
                    Line::from(Span::styled(
                        *l,
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ))
                })
                .collect::<Vec<_>>(),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));

        /* ================= STATUS ================= */

        let (glyph, glyph_color) = phase_glyph(&state.phase);

        let spinner_part = if matches!(state.phase, Phase::Idle) {
            format!(" {}", spinner(state.spinner_tick))
        } else {
            String::new()
        };

        let mut status_lines = Vec::new();

        status_lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", glyph),
                Style::default()
                    .fg(glyph_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("Phase: {:?}", state.phase),
                Style::default().fg(Color::White),
            ),
            Span::styled(spinner_part, Style::default().fg(Color::Yellow)),
        ]));

        if let Some(lang) = &state.language {
            let (label, color) = language_badge(&format!("{:?}", lang));
            status_lines.push(Line::from(Span::styled(
                label,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )));
        }

        if let Some(fw) = &state.framework {
            let (label, color) = framework_badge(&format!("{:?}", fw));
            status_lines.push(Line::from(Span::styled(
                label,
                Style::default().fg(color),
            )));
        }

        status_lines.push(Line::from(Span::styled(
            format!(
                "Base: {}",
                state.base_branch.clone().unwrap_or_else(|| "unknown".into())
            ),
            Style::default().fg(Color::Gray),
        )));

        let status = Paragraph::new(status_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title("Status"),
        );

        /* ================= LOGS ================= */

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

        let log_view = Paragraph::new(logs).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title("Execution"),
        );

        /* ================= COMMAND ================= */
        let input_style = if state.input_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let ghost_style = Style::default().fg(Color::DarkGray);

        let mut spans: Vec<Span> = Vec::new();

        // Command prefix
        spans.push(Span::styled("/ ", input_style));

        // User input
        spans.push(Span::styled(&state.input, input_style));

        // Ghost autocomplete (ONLY the suffix)
        if let Some(ac) = &state.autocomplete {
            if ac.starts_with(&state.input) {
                let suffix = &ac[state.input.len()..];
                if !suffix.is_empty() {
                    spans.push(Span::styled(suffix, ghost_style));
                }
            }
        }

        let input = Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title("Command"),
        );

        input_rect = layout[3];

        f.render_widget(header, layout[0]);
        f.render_widget(status, layout[1]);
        f.render_widget(log_view, layout[2]);
        f.render_widget(input, layout[3]);

        if state.input_focused {
            let x = input_rect.x + 4 + state.input.len() as u16;
            let y = input_rect.y + 1;
            f.set_cursor(x, y);
        }
    })?;

    Ok(input_rect)
}
