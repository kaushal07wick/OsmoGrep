//! ui/status.rs
//!
//! Header + status renderer for Osmogrep.

use sysinfo::System;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, BorderType, Paragraph},
};
use crate::state::{AgentState, Phase};
use crate::ui::helpers::{
    framework_badge,
    keyword_style,
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
            Constraint::Length(6), // ASCII header
            Constraint::Length(8), // status area
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
        HEADER.iter().map(|l| {
            Line::from(Span::styled(
                *l,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
        }).collect::<Vec<_>>(),
    )
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    f.render_widget(header, area);
}

fn render_status_block(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    let (sym, label, phase_color) = phase_badge(&state.lifecycle.phase);
    let mut lines: Vec<Line> = Vec::new();

    // Status
    let mut status_spans = vec![
        Span::styled(format!("{:<8}", "Status:"), Style::default().fg(Color::DarkGray)),
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

    // System info
    // ── System info ───────────────────────────────
    let mut sys = System::new();
    sys.refresh_memory();

    let used_gb =
        sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let total_gb =
        sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;

    let os = {
        let raw = std::env::consts::OS;
        let mut c = raw.chars();
        match c.next() {
            Some(f) => format!("{}{}", f.to_ascii_uppercase(), c.as_str()),
            None => raw.to_string(),
        }
    };

    lines.push(Line::from(vec![
        Span::styled(format!("{:<8}", "OS:"), Style::default().fg(Color::DarkGray)),
        Span::styled(os, Style::default().fg(Color::White)),
    ]));

    lines.push(Line::from(vec![
        Span::styled(format!("{:<8}", "RAM:"), Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.1} / {:.1} GB", used_gb, total_gb),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    let model = if state.ollama_model.is_empty() {
        "<default>"
    } else {
        &state.ollama_model
    };

    lines.push(Line::from(vec![
        Span::styled(format!("{:<8}", "Model:"), Style::default().fg(Color::DarkGray)),
        Span::styled(model, Style::default().fg(Color::White)),
    ]));

    // Git state (kept)
    if let Some(cur) = &state.lifecycle.current_branch {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<8}", "Current:"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                cur,
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    if let Some(base) = &state.lifecycle.base_branch {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<8}", "Base:"), Style::default().fg(Color::DarkGray)),
            Span::styled(base, Style::default().fg(Color::Gray)),
        ]));
    }

    if let Some(agent) = &state.lifecycle.agent_branch {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<8}", "Agent:"), Style::default().fg(Color::DarkGray)),
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

    // Top padding
    lines.push(Line::from(""));
    if let Some(snapshot) = &state.context_snapshot {
    lines.push(Line::from(vec![
        Span::styled(format!("{:<10}", "Context:"), Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(" {} file(s) ", snapshot.files.len()),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<10}", "Context:"), Style::default().fg(Color::DarkGray)),
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
            Span::styled(format!("{:<10}", "Language:"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {label} "),
                Style::default().fg(Color::Black).bg(color).add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    if let Some(fw) = &state.lifecycle.framework {
        let (label, color) = framework_badge(&format!("{:?}", fw));
        lines.push(Line::from(vec![
            Span::styled(format!("{:<10}", "Framework:"), Style::default().fg(Color::DarkGray)),
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
