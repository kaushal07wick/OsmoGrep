use sysinfo::System;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, BorderType, Paragraph},
};

use crate::{context::types::TestFramework, llm::backend::LlmBackend};
use crate::state::{AgentState, Phase, SinglePanelView};
use crate::ui::helpers::{
    framework_badge,
    language_badge,
    phase_badge,
    spinner,
};

pub fn render_status(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(8),
        ])
        .split(area);

    render_header(f, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70),
            Constraint::Percentage(30),
        ])
        .split(chunks[1]);

    render_status_block(f, body[0], state);
    render_context_block(f, body[1], state);
}

fn render_header(f: &mut ratatui::Frame, area: Rect) {
    const HEADER: [&str; 6] = [
        "░█████╗░░██████╗███╗░░░███╗░█████╗░░██████╗░██████╗░███████╗██████╗░",
        "██╔══██╗██╔════╝████╗░████║██╔══██╗██╔════╝░██╔══██╗██╔════╝██╔══██╗",
        "██║░░██║╚█████╗░██╔████╔██║██║░░██║██║░░██╗░██████╔╝█████╗░░██████╔╝",
        "██║░░██║░╚═══██╗██║╚██╔╝██║██║░░██║██║░░╚██╗██╔══██╗██╔══╝░░██╔═══╝░",
        "╚█████╔╝██████╔╝██║░╚═╝░██║╚█████╔╝╚██████╔╝██║░░██║███████╗██║░░░░░",
        "░╚════╝░╚═════╝░╚═╝░░░░░╚═╝░╚════╝░░╚═════╝░╚═╝░░╚═╝╚══════╝╚═╝░░░░░",
    ];

    let header = Paragraph::new(
        HEADER.iter()
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
    .block(Block::default().border_style(Style::default().fg(Color::DarkGray)));

    f.render_widget(header, area);
}

fn render_status_block(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    let (sym, label, phase_color) = phase_badge(&state.lifecycle.phase);
    let mut lines: Vec<Line> = Vec::new();

    let mut status_spans = vec![
        Span::styled("Status:  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{sym} {label}"),
            Style::default().fg(phase_color).add_modifier(Modifier::BOLD),
        ),
    ];

    if matches!(state.lifecycle.phase, Phase::Running) {
        if let Some(start) = state.ui.spinner_started_at {
            let frame = (start.elapsed().as_millis() / 120) as usize;
            status_spans.push(Span::raw(" "));
            status_spans.push(Span::styled(spinner(frame), Style::default().fg(phase_color)));
        }
    }

    lines.push(Line::from(status_spans));

    let mut sys = System::new();
    sys.refresh_memory();

    let used_gb = sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let total_gb = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;

    lines.push(Line::from(vec![
        Span::styled("RAM:     ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.1} / {:.1} GB", used_gb, total_gb),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    let model = match &state.llm_backend {
        LlmBackend::Ollama { model } => format!("Ollama ({})", model),
        LlmBackend::Remote { client } => {
            let cfg = client.current_config();
            if cfg.api_key.trim().is_empty() {
                "Ollama (default)".to_string()
            } else {
                format!("Remote ({:?} / {})", cfg.provider, cfg.model)
            }
        }
    };

    lines.push(Line::from(vec![
        Span::styled("Model:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(model, Style::default().fg(Color::White)),
    ]));

    if let Some(cur) = &state.lifecycle.current_branch {
        lines.push(Line::from(vec![
            Span::styled("Current: ", Style::default().fg(Color::DarkGray)),
            Span::styled(cur, Style::default().fg(Color::Green)),
        ]));
    }

    if let Some(agent) = &state.lifecycle.agent_branch {
        lines.push(Line::from(vec![
            Span::styled("Agent:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(agent, Style::default().fg(Color::Yellow)),
        ]));
    }

    let block = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title("SYSTEM")
            .title_alignment(Alignment::Center),
    );

    f.render_widget(block, area);
}

fn render_context_block(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    let mut lines: Vec<Line> = Vec::new();

    let active_diff = state
        .ui
        .selected_diff
        .and_then(|i| state.context.diff_analysis.get(i));

    let active_test = match &state.ui.panel_view {
        Some(SinglePanelView::TestGenPreview { candidate, .. }) => Some(candidate),
        _ => None,
    };

    let (file, symbol, mode) = if let Some(d) = active_diff {
        (Some(&d.file), d.symbol.as_deref(), "Diff")
    } else if let Some(c) = active_test {
        (Some(&c.diff.file), c.diff.symbol.as_deref(), "Test")
    } else {
        (None, None, "")
    };

    if let Some(file) = file {
        lines.push(Line::from(vec![
            Span::styled("View:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(mode, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]));

        lines.push(Line::from(vec![
            Span::styled("File:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(file, Style::default().fg(Color::White)),
        ]));

        if let Some(sym) = symbol {
            lines.push(Line::from(vec![
                Span::styled("Symbol:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(sym, Style::default().fg(Color::Yellow)),
            ]));
        }

        lines.push(Line::from(""));
    }

    // ---- FULL CONTEXT SNAPSHOT ----
    if let Some(snapshot) = &state.full_context_snapshot {
        // Code context
        lines.push(Line::from(vec![
            Span::styled("Code ctx: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {} file(s) ", snapshot.code.files.len()),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // Test context
        let tests = &snapshot.tests;

        lines.push(Line::from(vec![
            Span::styled("Tests:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if tests.exists { " present " } else { " none " },
                Style::default()
                    .fg(Color::Black)
                    .bg(if tests.exists { Color::Green } else { Color::DarkGray })
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        if tests.exists {
            if let Some(fw) = tests.framework {
                let (label, color) = match fw {
                    TestFramework::Pytest => ("pytest", Color::Green),
                    TestFramework::Unittest => ("unittest", Color::Blue),
                    TestFramework::Rust => ("cargo test", Color::Red),
                    _ => ("unknown", Color::DarkGray),
                };

                lines.push(Line::from(vec![
                    Span::styled("Test fw:  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!(" {label} "),
                        Style::default()
                            .fg(Color::Black)
                            .bg(color)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
        
            if let Some(style) = tests.style {
                lines.push(Line::from(vec![
                    Span::styled("Style:    ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!(" {:?} ", style),
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Blue)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            }

            if !tests.helpers.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Helpers:  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!(" {} ", tests.helpers.len()),
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
        }
    } else {
        lines.push(Line::from(vec![
            Span::styled("Context:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                " not built ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    if let Some(lang) = &state.lifecycle.language {
        let (label, color) = language_badge(&format!("{:?}", lang));
        lines.push(Line::from(vec![
            Span::styled("Language: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {label} "),
                Style::default().fg(Color::Black).bg(color).add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    let block = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title("CONTEXT")
            .title_alignment(Alignment::Center),
    );

    f.render_widget(block, area);
}
