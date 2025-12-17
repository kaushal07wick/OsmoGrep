use std::io;

use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

use crate::state::{AgentState, LogLevel, Phase, TestDecision, RiskLevel};

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
                Style::default().fg(phase_color).add_modifier(Modifier::BOLD),
            ),
        ]));

        status_lines.push(Line::from(vec![
            Span::styled("Current Branch: ", Style::default().fg(Color::Gray)),
            Span::styled(
                state.current_branch
                    .clone()
                    .unwrap_or_else(|| "unknown".into()),
                Style::default().fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        status_lines.push(Line::from(vec![
            Span::styled("Base Branch: ", Style::default().fg(Color::Gray)),
            Span::styled(
                state.base_branch.clone().unwrap_or_else(|| "unknown".into()),
                Style::default().fg(Color::DarkGray),
            ),
        ]));


        status_lines.push(Line::from(vec![
            Span::styled("Agent Branch: ", Style::default().fg(Color::Gray)),
            Span::styled(
                state.agent_branch.clone().unwrap_or_else(|| "none".into()),
                Style::default().fg(Color::Yellow),
            ),
        ]));

        status_lines.push(Line::from(vec![
            Span::styled("Agent Status: ", Style::default().fg(Color::Gray)),
            Span::styled(phase_label, Style::default().fg(phase_color)),
        ]));

        if let Some(lang) = &state.language {
            let (badge, color) = language_badge(&format!("{:?}", lang));
            status_lines.push(Line::from(vec![
                Span::styled("Language: ", Style::default().fg(Color::Gray)),
                Span::styled(badge, Style::default().fg(color)),
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
                .title("STATUS")
                .title_alignment(Alignment::Center),
        );

        f.render_widget(status, layout[1]);

        /* ================= EXECUTION ================= */

        let mut exec_lines = Vec::new();

        exec_lines.push(Line::from(""));

        if !state.logs.is_empty() {
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

        // ✅ ALWAYS SHOW PER-FILE DIFF ANALYSIS (AFTER LOGS)
        if !state.diff_analysis.is_empty() {
            exec_lines.push(Line::from(""));
            exec_lines.push(Line::from(Span::styled(
                "Diff Analysis",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));
            exec_lines.push(Line::from(""));

            for d in &state.diff_analysis {
                exec_lines.push(Line::from(vec![
                    Span::styled(
                        match &d.symbol {
                            Some(sym) => format!("{} :: {}", d.file, sym),
                            None => d.file.clone(),
                        },
                        Style::default().fg(Color::White),
                    ),
                    Span::raw(" | "),
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

                exec_lines.push(Line::from(Span::styled(
                    format!("  ↳ {}", d.reason),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        // idle fallback
        if state.logs.is_empty() && state.diff_analysis.is_empty() {
            exec_lines.push(Line::from(Span::styled(
                "Agent idle. Awaiting command.",
                Style::default().fg(Color::DarkGray),
            )));
        }


        let execution = Paragraph::new(exec_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("EXECUTION")
                .title_alignment(Alignment::Center),
        );

        f.render_widget(execution, layout[2]);

        /* ================= COMMAND ================= */

        let mut spans = Vec::new();
        spans.push(Span::styled("$_ ", Style::default().fg(Color::Cyan)));
        spans.push(Span::styled(&state.input, Style::default().fg(Color::White)));

        if let Some(ac) = &state.autocomplete {
            if ac.starts_with(&state.input) {
                let suffix = &ac[state.input.len()..];
                if !suffix.is_empty() {
                    spans.push(Span::styled(suffix, Style::default().fg(Color::DarkGray)));
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
                .title("COMMAND")
                .title_alignment(Alignment::Center),
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