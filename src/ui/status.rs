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
    // ─────────────────────────────────────────────────────────────
    // VIEW-AWARE SWITCH
    // ─────────────────────────────────────────────────────────────
    let in_diff_view = state.ui.selected_diff.is_some();
    let in_test_view = matches!(
        state.ui.panel_view,
        Some(crate::state::SinglePanelView::TestGenPreview { .. })
            | Some(crate::state::SinglePanelView::TestResult { .. })
    );

    if in_diff_view || in_test_view {
        render_context_inspector(f, area, state);
        return;
    }

    // ─────────────────────────────────────────────────────────────
    // AGENT PANEL (DEFAULT)
    // ─────────────────────────────────────────────────────────────
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title("AGENT")
        .title_alignment(Alignment::Center);

    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),     // content
            Constraint::Length(1),  // model (bottom)
        ])
        .split(inner);

    let mut lines = Vec::new();
    let (sym, label, color) = phase_badge(&state.lifecycle.phase);

    lines.push(Line::from(vec![
        Span::styled("Phase: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{sym} {label}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ]));

    if let Some(cur) = &state.lifecycle.current_branch {
        lines.push(Line::from(vec![
            Span::styled("Current: ", Style::default().fg(Color::DarkGray)),
            Span::styled(cur, Style::default().fg(Color::Green)),
        ]));
    }

    let agent_display = state
        .lifecycle
        .agent_branch
        .as_deref()
        .unwrap_or("none");

    lines.push(Line::from(vec![
        Span::styled("Agent:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            agent_display,
            Style::default().fg(if state.lifecycle.agent_branch.is_some() {
                Color::Yellow
            } else {
                Color::DarkGray
            }),
        ),
    ]));

    let mut sys = System::new();
    sys.refresh_memory();

    let used = sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let total = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;

    lines.push(Line::from(vec![
        Span::styled("RAM:     ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.1} / {:.1} GB", used, total),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("OS:      ", Style::default().fg(Color::DarkGray)),
        Span::styled(std::env::consts::OS, Style::default().fg(Color::Magenta)),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Context: ", Style::default().fg(Color::DarkGray)),
        Span::styled("not built", Style::default().fg(Color::DarkGray)),
    ]));

    f.render_widget(Paragraph::new(lines), chunks[0]);

    // ───────── MODEL (BOTTOM, INSIDE) ─────────
    let model_label = match &state.llm_backend {
        crate::llm::backend::LlmBackend::Remote { client } => {
            let cfg = client.current_config();
            format!("{:?}", cfg.provider).to_uppercase()
        }
        crate::llm::backend::LlmBackend::Ollama { .. } => "OLLAMA".to_string(),
    };

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            model_label,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center),
        chunks[1],
    );
}

fn render_context_inspector(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    let mut lines = Vec::new();

    let active_diff = state
        .ui
        .selected_diff
        .and_then(|i| state.context.diff_analysis.get(i));

    let active_test = match &state.ui.panel_view {
        Some(crate::state::SinglePanelView::TestGenPreview { candidate, .. }) => {
            Some(candidate)
        }
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
            Span::styled("View: ", Style::default().fg(Color::DarkGray)),
            Span::styled(mode, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]));

        lines.push(Line::from(vec![
            Span::styled("File: ", Style::default().fg(Color::DarkGray)),
            Span::styled(file, Style::default().fg(Color::White)),
        ]));

        if let Some(sym) = symbol {
            lines.push(Line::from(vec![
                Span::styled("Symbol: ", Style::default().fg(Color::DarkGray)),
                Span::styled(sym, Style::default().fg(Color::Yellow)),
            ]));
        }

        lines.push(Line::from(""));
    }

    if let Some(snapshot) = &state.full_context_snapshot {
        lines.push(Line::from(vec![
            Span::styled("Code ctx: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {} files ", snapshot.code.files.len()),
                Style::default().fg(Color::Black).bg(Color::Green),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Context: ", Style::default().fg(Color::DarkGray)),
            Span::styled(" not built ", Style::default().fg(Color::Black).bg(Color::DarkGray)),
        ]));
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title("CONTEXT")
                .title_alignment(Alignment::Center),
        ),
        area,
    );
}
