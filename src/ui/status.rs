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

    let is_active =
        state.ui.selected_diff.is_some()
            || state.ui.panel_view.is_some()
            || !matches!(state.lifecycle.phase, Phase::Idle)
            || Instant::now()
                .duration_since(state.ui.last_activity)
                < Duration::from_secs(10);

    let (activity_label, activity_color) = if is_active {
        ("Active", Color::Green)
    } else {
        ("Idle", Color::Yellow)
    };

    let mut lines: Vec<Line> = Vec::with_capacity(9);

    /* ---------- Phase ---------- */
    lines.push(Line::from(vec![
        Span::styled("Phase: ", Style::default().fg(Color::Gray)),
        Span::styled(
            format!("{sym} {label}"),
            Style::default()
                .fg(phase_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    /* ---------- Activity ---------- */
    let mut activity = vec![
        Span::styled("Status: ", Style::default().fg(Color::Gray)),
        Span::styled(
            format!("● {}", activity_label),
            Style::default()
                .fg(activity_color)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if is_active {
        activity.push(Span::raw(" "));
        activity.push(Span::styled(
            spinner(
                (Instant::now()
                    .duration_since(state.ui.last_activity)
                    .as_millis()
                    / 120) as usize,
            ),
            Style::default().fg(Color::Green),
        ));
    }

    lines.push(Line::from(activity));

    /* ---------- Diff context (NEW) ---------- */
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
        }
    }

    /* ---------- Branches ---------- */
    lines.push(Line::from(vec![
        Span::styled("Current Branch: ", Style::default().fg(Color::Gray)),
        Span::styled(
            state
                .lifecycle
                .current_branch
                .clone()
                .unwrap_or_else(|| "unknown".into()),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Base Branch: ", Style::default().fg(Color::Gray)),
        Span::styled(
            state
                .lifecycle
                .base_branch
                .clone()
                .unwrap_or_else(|| "unknown".into()),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Agent Branch: ", Style::default().fg(Color::Gray)),
        Span::styled(
            state
                .lifecycle
                .agent_branch
                .clone()
                .unwrap_or_else(|| "none".into()),
            Style::default().fg(Color::Yellow),
        ),
    ]));

    /* ---------- Language ---------- */
    if let Some(lang) = &state.lifecycle.language {
        let (badge, color) = language_badge(&format!("{:?}", lang));
        lines.push(Line::from(vec![
            Span::styled("Language: ", Style::default().fg(Color::Gray)),
            Span::styled(badge, Style::default().fg(color)),
        ]));
    }

    /* ---------- Framework ---------- */
    if let Some(fw) = &state.lifecycle.framework {
        let (badge, color) = framework_badge(&format!("{:?}", fw));
        lines.push(Line::from(vec![
            Span::styled("Framework: ", Style::default().fg(Color::Gray)),
            Span::styled(badge, Style::default().fg(color)),
        ]));
    }

    let status = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("STATUS")
            .title_alignment(Alignment::Center),
    );

    f.render_widget(status, area);
}
