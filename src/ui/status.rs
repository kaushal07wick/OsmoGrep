use sysinfo::System;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::context::types::TestFramework;
use crate::state::{AgentState, SinglePanelView};
use crate::ui::helpers::{
    framework_badge,
    language_badge,
    phase_badge,
    spinner,
};

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
        HEADER.iter().map(|l| {
            Line::from(Span::styled(
                *l,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
        }).collect::<Vec<_>>(),
    )
    .alignment(Alignment::Center);

    f.render_widget(header, area);
}

pub fn render_side_status(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
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
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let mut lines: Vec<Line> = Vec::new();

    // ───────── Status / Phase ─────────
    let (sym, label, color) = phase_badge(&state.lifecycle.phase);
    let mut phase = vec![
        Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{sym} {label}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ];

    if let Some(start) = state.ui.spinner_started_at {
        let frame = (start.elapsed().as_millis() / 120) as usize;
        phase.push(Span::raw(" "));
        phase.push(Span::styled(spinner(frame), Style::default().fg(color)));
    }

    lines.push(Line::from(phase));

    // ───────── Branches ─────────
    if let Some(cur) = &state.lifecycle.current_branch {
        lines.push(Line::from(vec![
            Span::styled("Current: ", Style::default().fg(Color::DarkGray)),
            Span::styled(cur, Style::default().fg(Color::Green)),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("Agent:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            state.lifecycle.agent_branch.as_deref().unwrap_or("none"),
            Style::default().fg(if state.lifecycle.agent_branch.is_some() {
                Color::Yellow
            } else {
                Color::DarkGray
            }),
        ),
    ]));

    // ───────── System ─────────
    let mut sys = System::new();
    sys.refresh_memory();

    lines.push(Line::from(vec![
        Span::styled("RAM:     ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(
                "{:.1} / {:.1} GB",
                sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0,
                sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0,
            ),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("OS:      ", Style::default().fg(Color::DarkGray)),
        Span::styled(std::env::consts::OS, Style::default().fg(Color::Magenta)),
    ]));

    // ───────── Active Inspector (Diff/Test) ─────────
    if let Some(diff) = state.ui.selected_diff
        .and_then(|i| state.context.diff_analysis.get(i)) {

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("View: ", Style::default().fg(Color::DarkGray)),
            Span::styled("Diff", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("📄 File: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&diff.file, Style::default().fg(Color::White)),
        ]));
    }

    if let Some(SinglePanelView::TestGenPreview { candidate, .. }) = &state.ui.panel_view {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("View: ", Style::default().fg(Color::DarkGray)),
            Span::styled("Test", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("📄 File: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&candidate.diff.file, Style::default().fg(Color::White)),
        ]));
    }

    // ───────── CONTEXT (ALWAYS WHEN PRESENT) ─────────
    lines.push(Line::from(""));

    if let Some(snapshot) = &state.full_context_snapshot {
        lines.push(Line::from(vec![
            Span::styled("Code ctx: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {} file(s) ", snapshot.code.files.len()),
                Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD),
            ),
        ]));

        let tests = &snapshot.tests;
        lines.push(Line::from(vec![
            Span::styled("🧪 Tests: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if tests.exists { " present " } else { " none " },
                Style::default()
                    .fg(Color::Black)
                    .bg(if tests.exists { Color::Green } else { Color::DarkGray })
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        if tests.exists {
            let (fw, c) = framework_badge(&format!("{:?}", tests.framework.unwrap_or(TestFramework::Unknown)));
            lines.push(Line::from(vec![
                Span::styled("⚙ Framework: ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!(" {fw} "), Style::default().fg(Color::Black).bg(c).add_modifier(Modifier::BOLD)),
            ]));

            if let Some(style) = tests.style {
                lines.push(Line::from(vec![
                    Span::styled("Style: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(format!(" {:?} ", style), Style::default().fg(Color::Black).bg(Color::Blue).add_modifier(Modifier::BOLD)),
                ]));
            }

            if !tests.helpers.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("📦 Helpers: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(format!(" {} ", tests.helpers.len()), Style::default().fg(Color::Black).bg(Color::Magenta).add_modifier(Modifier::BOLD)),
                ]));
            }
        }
    }

    // ───────── Language ─────────
    if let Some(lang) = &state.lifecycle.language {
        let (label, c) = language_badge(&format!("{:?}", lang));
        lines.push(Line::from(vec![
            Span::styled("Language: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!(" {label} "), Style::default().fg(Color::Black).bg(c).add_modifier(Modifier::BOLD)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), chunks[0]);

    // ───────── Model ─────────
    let model = match &state.llm_backend {
        crate::llm::backend::LlmBackend::Remote { client } => {
            format!("{:?}", client.current_config().provider).to_uppercase()
        }
        crate::llm::backend::LlmBackend::Ollama { .. } => "OLLAMA".to_string(),
    };

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            model,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center),
        chunks[1],
    );
}
