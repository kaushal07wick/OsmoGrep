use sysinfo::System;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::state::AgentState;
use crate::ui::helpers::{language_badge, phase_badge, spinner};

pub fn render_status(
    f: &mut ratatui::Frame,
    area: Rect,
    _state: &AgentState,
) {
    render_header(f, area);
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
        HEADER
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
    .alignment(Alignment::Center);

    f.render_widget(header, area);
}

pub fn render_side_status(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    // Split the right panel into AGENT and CONTEXT sections
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),     // AGENT section (takes remaining space)
            Constraint::Length(5),  // CONTEXT section (fixed height)
        ])
        .split(area);

    // ───────── AGENT SECTION ─────────
    let mut agent_lines = Vec::new();
    let (sym, label, color) = phase_badge(&state.lifecycle.phase);

    agent_lines.push(Line::from(vec![
        Span::styled("Phase: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{sym} {label}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ]));

    if let Some(cur) = &state.lifecycle.current_branch {
        agent_lines.push(Line::from(vec![
            Span::styled("Current: ", Style::default().fg(Color::DarkGray)),
            Span::styled(cur, Style::default().fg(Color::Green)),
        ]));
    }

    if let Some(agent) = &state.lifecycle.agent_branch {
        agent_lines.push(Line::from(vec![
            Span::styled("Agent:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(agent, Style::default().fg(Color::Yellow)),
        ]));
    }

    let mut sys = System::new();
    sys.refresh_memory();

    let used = sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let total = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;

    agent_lines.push(Line::from(vec![
        Span::styled("RAM:     ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.1} / {:.1} GB", used, total),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    agent_lines.push(Line::from(vec![
        Span::styled("OS:      ", Style::default().fg(Color::DarkGray)),
        Span::styled(std::env::consts::OS, Style::default().fg(Color::Magenta)),
    ]));

    if let Some(start) = state.ui.spinner_started_at {
        let frame = (start.elapsed().as_millis() / 120) as usize;
        agent_lines.push(Line::from(vec![
            Span::styled("Task:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                spinner(frame),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        let frame = (state.ui.last_activity.elapsed().as_millis() / 400) % 3;
        let dots = match frame {
            0 => "·",
            1 => "··",
            _ => "···",
        };

        agent_lines.push(Line::from(vec![
            Span::styled("Task:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("idle{}", dots), Style::default().fg(Color::DarkGray)),
        ]));
    }

    f.render_widget(
        Paragraph::new(agent_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title("AGENT")
                .title_alignment(Alignment::Center),
        ),
        chunks[0],
    );

    // ───────── CONTEXT SECTION ─────────
    let mut ctx_lines = Vec::new();

    if let Some(snapshot) = &state.full_context_snapshot {
        ctx_lines.push(Line::from(vec![
            Span::styled("Files: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                snapshot.code.files.len().to_string(),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
        ]));

        if let Some(lang) = &state.lifecycle.language {
            let (label, color) = language_badge(&format!("{:?}", lang));
            ctx_lines.push(Line::from(vec![
                Span::styled("Lang:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(label, Style::default().fg(color)),
            ]));
        }
    } else {
        ctx_lines.push(Line::from(vec![
            Span::styled("Context: ", Style::default().fg(Color::DarkGray)),
            Span::styled("not built", Style::default().fg(Color::DarkGray)),
        ]));
    }

    f.render_widget(
        Paragraph::new(ctx_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title("CONTEXT")
                .title_alignment(Alignment::Center),
        ),
        chunks[1],
    );
}