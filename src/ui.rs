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

fn phase_style(phase: &Phase) -> (Color, &'static str) {
    match phase {
        Phase::Idle => (Color::Yellow, "Idle"),
        Phase::ExecuteAgent => (Color::Cyan, "Running"),
        Phase::CreateNewAgent => (Color::Blue, "Creating"),
        Phase::Rollback => (Color::Magenta, "Rollback"),
        Phase::Done => (Color::Green, "Done"),
        _ => (Color::DarkGray, "Unknown"),
    }
}

fn phase_symbol(phase: &Phase) -> &'static str {
    match phase {
        Phase::Idle => "●",
        Phase::ExecuteAgent => "▶",
        Phase::CreateNewAgent => "＋",
        Phase::Rollback => "↩",
        Phase::Done => "✔",
        _ => "○",
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
        _ => ("❓ Unknown", Color::DarkGray),
    }
}

fn framework_badge(fw: &str) -> (&'static str, Color) {
    match fw {
        "CargoTest" => ("🦀🧪 Cargo", Color::Cyan),
        "Pytest" => ("🐍🧪 Pytest", Color::Yellow),
        "GoTest" => ("🐹🧪 Go test", Color::Blue),
        "JUnit" => ("☕🧪 JUnit", Color::Red),
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
                Constraint::Length(8), // header
                Constraint::Length(8), // status
                Constraint::Min(6),    // execution
                Constraint::Length(3), // command
            ])
            .split(f.size());

        /* ================= HEADER ================= */

        let header_lines = [
            "░█████╗░░██████╗███╗░░░███╗░█████╗░░██████╗░██████╗░███████╗██████╗░",
            "██╔══██╗██╔════╝████╗░████║██╔══██╗██╔════╝░██╔══██╗██╔════╝██╔══██╗",
            "██║░░██║╚█████╗░██╔████╔██║██║░░██║██║░░██╗░██████╔╝█████╗░░██████╔╝",
            "██║░░██║░╚═══██╗██║╚██╔╝██║██║░░██║██║░░╚██╗██╔══██╗██╔══╝░░██╔═══╝░",
            "╚█████╔╝██████╔╝██║░╚═╝░██║╚█████╔╝╚██████╔╝██║░░██║███████╗██║░░░░░",
            "░╚════╝░╚═════╝░╚═╝░░░░░╚═╝░╚════╝░░╚═════╝░╚═╝░░╚═╝╚══════╝╚═╝░░░░░",
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

        f.render_widget(header, layout[0]);

        /* ================= STATUS ================= */

        let (phase_color, phase_label) = phase_style(&state.phase);

        let mut status_lines = Vec::new();

        status_lines.push(Line::from(vec![
            Span::styled("Phase: ", Style::default().fg(Color::Gray)),
            Span::styled(
                phase_label,
                Style::default()
                    .fg(phase_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        status_lines.push(Line::from(vec![
            Span::styled("Branch: ", Style::default().fg(Color::Gray)),
            Span::styled(
                state.base_branch.clone().unwrap_or_else(|| "unknown".into()),
                Style::default().fg(Color::White),
            ),
        ]));

        status_lines.push(Line::from(vec![
            Span::styled("Agent: ", Style::default().fg(Color::Gray)),
            Span::styled(phase_label, Style::default().fg(phase_color)),
        ]));

        status_lines.push(Line::from(vec![
            Span::styled("Repo: ", Style::default().fg(Color::Gray)),
            Span::styled("Uncommitted", Style::default().fg(Color::Yellow)),
        ]));

        if let Some(lang) = &state.language {
            let (badge, color) = language_badge(&format!("{:?}", lang));
            status_lines.push(Line::from(vec![
                Span::styled("Language: ", Style::default().fg(Color::Gray)),
                Span::styled(badge, Style::default().fg(color).add_modifier(Modifier::BOLD)),
            ]));
        }

        if let Some(fw) = &state.framework {
            let (badge, color) = framework_badge(&format!("{:?}", fw));
            status_lines.push(Line::from(vec![
                Span::styled("Framework: ", Style::default().fg(Color::Gray)),
                Span::styled(badge, Style::default().fg(color)),
            ]));
        }

        let status = Paragraph::new(status_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title_alignment(Alignment::Center)
                .title("STATUS"),
        );

        f.render_widget(status, layout[1]);

        /* ================= EXECUTION ================= */

        let mut exec_lines = Vec::new();

        exec_lines.push(Line::from(vec![
            Span::styled("Timeline: ", Style::default().fg(Color::Gray)),
            Span::styled(phase_symbol(&state.phase), Style::default().fg(phase_color)),
        ]));

        exec_lines.push(Line::from(""));

        if matches!(state.phase, Phase::Idle) {
            exec_lines.push(Line::from(Span::styled(
                "Agent idle. Awaiting command.",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            exec_lines.extend(
                state.logs.iter().rev().take(25).rev().map(|l| {
                    let color = match l.level {
                        LogLevel::Info => Color::White,
                        LogLevel::Success => Color::Green,
                        LogLevel::Warn => Color::Yellow,
                        LogLevel::Error => Color::Red,
                    };
                    Line::from(Span::styled(&l.text, Style::default().fg(color)))
                }),
            );
        }

        let execution = Paragraph::new(exec_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title_alignment(Alignment::Center)
                .title("EXECUTION"),
        );

        f.render_widget(execution, layout[2]);

        /* ================= COMMAND ================= */

        let input_style = Style::default().fg(Color::White);
        let ghost_style = Style::default().fg(Color::DarkGray);

        let mut spans = Vec::new();
        spans.push(Span::styled("$_ ", Style::default().fg(Color::Cyan)));
        spans.push(Span::styled(&state.input, input_style));

        if let Some(ac) = &state.autocomplete {
            if ac.starts_with(&state.input) {
                let suffix = &ac[state.input.len()..];
                if !suffix.is_empty() {
                    spans.push(Span::styled(suffix, ghost_style));
                }
            }
        }

        if !matches!(state.phase, Phase::Idle) {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                spinner(state.spinner_tick),
                Style::default().fg(Color::Yellow),
            ));
        }

        let input = Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .title_alignment(Alignment::Center)
                .title("COMMAND"),
        );

        input_rect = layout[3];
        f.render_widget(input, input_rect);

        f.set_cursor(
            input_rect.x + 4 + state.input.len() as u16,
            input_rect.y + 1,
        );
    })?;

    Ok(input_rect)
}
