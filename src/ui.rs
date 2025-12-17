use std::io;

use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

use crate::state::{
    AgentState, LogLevel, Phase, TestDecision, RiskLevel,
};

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

fn decision_color(d: &TestDecision) -> Color {
    match d {
        TestDecision::Yes => Color::Red,
        TestDecision::Conditional => Color::Yellow,
        TestDecision::No => Color::Green,
    }
}

fn risk_color(r: &RiskLevel) -> Color {
    match r {
        RiskLevel::High => Color::Red,
        RiskLevel::Medium => Color::Yellow,
        RiskLevel::Low => Color::Green,
    }
}

fn file_icon(file: &str) -> &'static str {
    if file.ends_with(".rs") {
        "🦀"
    } else if file.ends_with(".py") {
        "🐍"
    } else {
        "📄"
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
                Constraint::Length(8),
                Constraint::Length(9),
                Constraint::Min(8),
                Constraint::Length(3),
            ])
            .split(f.size());

        /* ================= HEADER ================= */

        let header = Paragraph::new(
            [
                "░█████╗░░██████╗███╗░░░███╗░█████╗░░██████╗░██████╗░███████╗██████╗░",
                "██╔══██╗██╔════╝████╗░████║██╔══██╗██╔════╝░██╔══██╗██╔════╝██╔══██╗",
                "██║░░██║╚█████╗░██╔████╔██║██║░░██║██║░░██╗░██████╔╝█████╗░░██████╔╝",
                "██║░░██║░╚═══██╗██║╚██╔╝██║██║░░██║██║░░╚██╗██╔══██╗██╔══╝░░██╔═══╝░",
                "╚█████╔╝██████╔╝██║░╚═╝░██║╚█████╔╝╚██████╔╝██║░░██║███████╗██║░░░░░",
                "░╚════╝░╚═════╝░╚═╝░░░░░╚═╝░╚════╝░░╚═════╝░╚═╝░░╚═╝╚══════╝╚═╝░░░░░",
            ]
            .iter()
            .map(|l| {
                Line::from(Span::styled(
                    *l,
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ))
            })
            .collect::<Vec<_>>(),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));

        f.render_widget(header, layout[0]);

        /* ================= STATUS ================= */

        let (phase_color, phase_label) = phase_style(&state.phase);
        let phase_icon = phase_symbol(&state.phase);

        let status = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Phase: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{} {}", phase_icon, phase_label),
                    Style::default().fg(phase_color).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Current Branch: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    state.current_branch.clone().unwrap_or_else(|| "unknown".into()),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Base Branch: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    state.base_branch.clone().unwrap_or_else(|| "unknown".into()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            Line::from(vec![
                Span::styled("Agent Branch: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    state.agent_branch.clone().unwrap_or_else(|| "none".into()),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
        ])
        .block(Block::default().borders(Borders::ALL).title("STATUS"));

        f.render_widget(status, layout[1]);

        /* ================= EXECUTION ================= */

        let mut exec = Vec::new();

        if !state.logs.is_empty() {
            for l in state.logs.iter().rev().take(20).rev() {
                let color = match l.level {
                    LogLevel::Info => Color::White,
                    LogLevel::Success => Color::Green,
                    LogLevel::Warn => Color::Yellow,
                    LogLevel::Error => Color::Red,
                };
                exec.push(Line::from(Span::styled(&l.text, Style::default().fg(color))));
            }
        }

        if !state.diff_analysis.is_empty() {
            exec.push(Line::from(""));
            exec.push(Line::from(Span::styled(
                "Diff Analysis",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));

            for d in &state.diff_analysis {
                let icon = file_icon(&d.file);

                exec.push(Line::from(vec![
                    Span::raw(format!("{} ", icon)),
                    Span::styled(
                        match &d.symbol {
                            Some(s) => format!("{} :: {}", d.file, s),
                            None => d.file.clone(),
                        },
                        Style::default().fg(Color::White),
                    ),
                    Span::raw("  Δ  "),
                    Span::styled(
                        format!("{:?}", d.test_required),
                        Style::default().fg(decision_color(&d.test_required)),
                    ),
                    Span::raw(" | "),
                    Span::styled(
                        format!("{:?}", d.risk),
                        Style::default().fg(risk_color(&d.risk)),
                    ),
                ]));

                exec.push(Line::from(Span::styled(
                    format!("  ↳ {}", d.reason),
                    Style::default().fg(Color::DarkGray),
                )));

                if let Some(pretty) = &d.pretty {
                    exec.push(Line::from(""));
                    for line in pretty.lines() {
                        exec.push(Line::from(Span::styled(
                            format!("    {}", line),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
            }
        }

        let execution = Paragraph::new(exec)
            .block(Block::default().borders(Borders::ALL).title("EXECUTION"));

        f.render_widget(execution, layout[2]);

        /* ================= COMMAND ================= */

        let mut spans = vec![
            Span::styled("$_ ", Style::default().fg(Color::Cyan)),
            Span::styled(&state.input, Style::default().fg(Color::White)),
        ];

        if let Some(ac) = &state.autocomplete {
            if ac.starts_with(&state.input) {
                let suffix = &ac[state.input.len()..];
                if !suffix.is_empty() {
                    spans.push(Span::styled(
                        suffix,
                        Style::default().fg(Color::DarkGray),
                    ));
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

        let input = Paragraph::new(Line::from(spans))
            .block(Block::default().borders(Borders::ALL).title("COMMAND"));

        input_rect = layout[3];
        f.render_widget(input, input_rect);

        f.set_cursor(
            input_rect.x + 4 + state.input.len() as u16,
            input_rect.y + 1,
        );
    })?;

    Ok(input_rect)
}
