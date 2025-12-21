//! ui/status.rs
//!
//! Header + status renderer for Osmogrep.

use std::time::{Duration, Instant};

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
            Constraint::Length(8), // status block
        ])
        .split(area);

    render_header(f, chunks[0]);
    render_status_block(f, chunks[1], state);
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
       Branch context (always show current)
       ===================================================== */

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
       Language / framework (tertiary metadata)
       ===================================================== */

    let mut meta: Vec<Span> = Vec::new();

    if let Some(lang) = &state.lifecycle.language {
        let (badge, color) = language_badge(&format!("{:?}", lang));
        meta.push(Span::styled(badge, Style::default().fg(color)));
    }

    if let Some(fw) = &state.lifecycle.framework {
        if !meta.is_empty() {
            meta.push(Span::raw("   "));
        }
        let (badge, color) = framework_badge(&format!("{:?}", fw));
        meta.push(Span::styled(badge, Style::default().fg(color)));
    }

    if !meta.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(meta));
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
