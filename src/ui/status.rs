//! ui/status.rs
//!
//! Header + status renderer for Osmogrep.

use std::time::{Duration, Instant};
use crate::context::types::IndexStatus;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::state::{AgentState, Phase};
use crate::ui::helpers::{
    framework_badge,
    language_badge,
    phase_badge,
    spinner,
    symbol_style,
    keyword_style,
};

/* ============================================================
   Public API
   ============================================================ */

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

    // ⬇️ split the STATUS area horizontally
    let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70), // main status
            Constraint::Percentage(30), // context index
        ])
        .split(chunks[1]);

    render_status_block(f, status_chunks[0], state);
    render_context_block(f, status_chunks[1], state);
}


/* ============================================================
   Header
   ============================================================ */

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
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::BOTTOM));

    f.render_widget(header, area);
}


/* ============================================================
   Status block
   ============================================================ */

fn render_status_block(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    let (sym, label, phase_color) = phase_badge(&state.lifecycle.phase);

    let mut lines: Vec<Line> = Vec::with_capacity(10);

    /* =====================================================
       Primary state (single source of truth)
       ===================================================== */

    let mut state_spans = vec![
        Span::styled(
            format!("{sym} {label}"),
            Style::default()
                .fg(phase_color)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if matches!(state.lifecycle.phase, Phase::Running) {
        if let Some(start) = state.ui.spinner_started_at {
            let frame = (start.elapsed().as_millis() / 120) as usize;
            state_spans.push(Span::raw("  "));
            state_spans.push(Span::styled(
                spinner(frame),
                Style::default().fg(phase_color),
            ));
        }
    }

    lines.push(Line::from(state_spans));
    lines.push(Line::from(""));

    /* =====================================================
       Diff context (ONLY when in diff view)
       ===================================================== */

    if state.ui.in_diff_view {
        if let Some(idx) = state.ui.selected_diff {
            if let Some(d) = state.context.diff_analysis.get(idx) {
                let mut spans = vec![
                    Span::styled("Diff: ", keyword_style()),
                    Span::styled(&d.file, Style::default()),
                ];

                if let Some(sym) = &d.symbol {
                    spans.push(Span::raw(" :: "));
                    spans.push(Span::styled(sym, symbol_style()));
                }

                lines.push(Line::from(spans));
                lines.push(Line::from(""));
            }
        }
    }

    /* =====================================================
   Branch context (explicit + informational)
   ===================================================== */

    if let Some(cur) = &state.lifecycle.current_branch {
        lines.push(Line::from(vec![
            Span::styled("Current: ", keyword_style()),
            Span::styled(
                cur,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    if let Some(base) = &state.lifecycle.base_branch {
        lines.push(Line::from(vec![
            Span::styled("Base:    ", keyword_style()),
            Span::styled(
                base,
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    if let Some(agent) = &state.lifecycle.agent_branch {
        lines.push(Line::from(vec![
            Span::styled("Agent:   ", keyword_style()),
            Span::styled(
                agent,
                Style::default().fg(Color::Yellow),
            ),
        ]));
    }
    /* =====================================================
       Render
       ===================================================== */

    let status = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("STATE")
            .title_alignment(Alignment::Center),
    );

    f.render_widget(status, area);
}

fn render_context_block(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled(
            "Repo Context",
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(""));

    /* ---------- index status ---------- */

    if let Some(index) = &state.context_index {
        let status = index.status.read().unwrap();

        let (label, color) = match &*status {
            IndexStatus::Indexing => ("Indexing…", Color::Yellow),
            IndexStatus::Ready => ("Ready", Color::Green),
            IndexStatus::Failed(_) => ("Failed", Color::Red),
        };

        lines.push(Line::from(vec![
            Span::styled("Status: ", keyword_style()),
            Span::styled(label, Style::default().fg(color)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Status: ", keyword_style()),
            Span::styled("Not started", Style::default().fg(Color::DarkGray)),
        ]));
    }

    /* ---------- language ---------- */

    if let Some(lang) = &state.lifecycle.language {
        lines.push(Line::from(vec![
            Span::styled("Language: ", keyword_style()),
            Span::styled(
                format!("{:?}", lang),
                Style::default().fg(Color::Cyan),
            ),
        ]));
    }

    /* ---------- framework ---------- */

    if let Some(fw) = &state.lifecycle.framework {
        lines.push(Line::from(vec![
            Span::styled("Framework: ", keyword_style()),
            Span::styled(
                format!("{:?}", fw),
                Style::default().fg(Color::Magenta),
            ),
        ]));
    }

    let block = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("CONTEXT")
            .title_alignment(Alignment::Center),
    );

    f.render_widget(block, area);
}
