use sysinfo::System;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::state::AgentState;
use crate::ui::helpers::{
    framework_badge,
    language_badge,
    phase_badge,
    running_pulse,
    format_uptime,
};

fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/* ==========================================================
   TOP STATUS BAR — FLAT BACKGROUND, NO BORDERS
========================================================== */

pub fn render_status(f: &mut ratatui::Frame, area: Rect, state: &AgentState) {
    // Flat background
    f.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(25,25,25))),
        area,
    );

    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(20),
            Constraint::Length(12),
        ])
        .split(area);

    /* -------- LEFT (Repo + Branch) -------- */
    let repo = state.repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("<repo>");

    let branch = state.lifecycle.current_branch.as_deref().unwrap_or("main");

    let left = Paragraph::new(
        Line::from(vec![
            Span::styled("Repo ",   Style::default().fg(Color::Rgb(140,140,140))),
            Span::styled(repo,      Style::default().fg(Color::White)),
            Span::raw("   "),
            Span::styled("Branch ", Style::default().fg(Color::Rgb(140,140,140))),
            Span::styled(branch,    Style::default().fg(Color::White)),
        ])
    );

    f.render_widget(left, layout[0]);

    /* -------- CENTER (Phase) -------- */
    let (_, phase_label, _) = phase_badge(&state.lifecycle.phase);

    let mid = Paragraph::new(
        Line::from(vec![
            Span::styled("Status ", Style::default().fg(Color::Rgb(140,140,140))),
            Span::styled(
                phase_label,
                Style::default()
                    .fg(Color::Rgb(0,220,140))
                    .add_modifier(Modifier::BOLD)
            ),
        ])
    ).alignment(Alignment::Center);

    f.render_widget(mid, layout[1]);

    /* -------- RIGHT (Version) -------- */
    let ver = Paragraph::new(
        Line::from(vec![
            Span::styled("v",              Style::default().fg(Color::Rgb(140,140,140))),
            Span::styled(app_version(),    Style::default().fg(Color::White)),
        ])
    ).alignment(Alignment::Right);

    f.render_widget(ver, layout[2]);
}


/* ==========================================================
   RIGHT SIDEBAR — BORDERLESS + INTERNAL PADDING
========================================================== */

pub fn render_side_status(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    /* -------- Outer padding (shifts panel right) -------- */
    let padded = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(2),
        height: area.height,
    };

    // Sidebar background color
    f.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(32,32,32))),
        padded,
    );

    /* -------- Inner padding (content spacing) -------- */
    let inner_padded = Rect {
        x: padded.x + 2,
        y: padded.y + 1,
        width: padded.width.saturating_sub(3),
        height: padded.height.saturating_sub(2),
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),   // Large scrolling section
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner_padded);

    let mut lines: Vec<Line> = Vec::new();

    /* STATUS */
    let (_, label, _) = phase_badge(&state.lifecycle.phase);
    lines.push(Line::from(vec![
        Span::styled("Status", Style::default().fg(Color::Rgb(140,140,140))),
        Span::raw(": "),
        Span::styled(label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]));

    /* CURRENT BRANCH */
    if let Some(cur) = &state.lifecycle.current_branch {
        lines.push(Line::from(vec![
            Span::styled("Current", Style::default().fg(Color::Rgb(140,140,140))),
            Span::raw(": "),
            Span::styled(cur, Style::default().fg(Color::White)),
        ]));
    }

    /* AGENT BRANCH */
    lines.push(Line::from(vec![
        Span::styled("Agent", Style::default().fg(Color::Rgb(140,140,140))),
        Span::raw(": "),
        Span::styled(
            state.lifecycle.agent_branch.as_deref().unwrap_or("None"),
            Style::default().fg(Color::White),
        ),
    ]));

    /* MEMORY */
    let mut sys = System::new();
    sys.refresh_memory();

    lines.push(Line::from(vec![
        Span::styled("RAM", Style::default().fg(Color::Rgb(140,140,140))),
        Span::raw(": "),
        Span::styled(
            format!(
                "{:.1}/{:.1} GB",
                sys.used_memory() as f64 / 1_073_741_824.0,
                sys.total_memory() as f64 / 1_073_741_824.0,
            ),
            Style::default().fg(Color::White),
        ),
    ]));

    /* UPTIME */
    lines.push(Line::from(vec![
        Span::styled("Uptime", Style::default().fg(Color::Rgb(140,140,140))),
        Span::raw(": "),
        Span::styled(format_uptime(state.started_at), Style::default().fg(Color::Gray)),
    ]));

    /* ACTIVE DIFF */
    if let Some(diff) = state.ui.selected_diff.and_then(|i| state.context.diff_analysis.get(i)) {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled("View", Style::default().fg(Color::Rgb(140,140,140))),
            Span::raw(": "),
            Span::styled("Diff", Style::default().fg(Color::Rgb(0,220,140))),
        ]));
        lines.push(Line::from(vec![
            Span::styled("File", Style::default().fg(Color::Rgb(140,140,140))),
            Span::raw(": "),
            Span::styled(&diff.file, Style::default().fg(Color::White)),
        ]));
    }

    /* CONTEXT SNAPSHOT */
    if let Some(snapshot) = &state.full_context_snapshot {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "Context",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )));

        lines.push(Line::from(vec![
            Span::styled("Code", Style::default().fg(Color::Rgb(140,140,140))),
            Span::raw(": "),
            Span::styled(
                format!("{} files", snapshot.code.files.len()),
                Style::default().fg(Color::White),
            ),
        ]));

        let tests = &snapshot.tests;

        lines.push(Line::from(vec![
            Span::styled("Tests", Style::default().fg(Color::Rgb(140,140,140))),
            Span::raw(": "),
            Span::styled(
                if tests.exists { "Present" } else { "None" },
                Style::default().fg(Color::White),
            ),
        ]));

        if tests.exists {
            if let Some(fw) = tests.framework {
                let (label, _) = framework_badge(&fw);
                lines.push(Line::from(vec![
                    Span::styled("Framework", Style::default().fg(Color::Rgb(140,140,140))),
                    Span::raw(": "),
                    Span::styled(label, Style::default().fg(Color::Rgb(0,220,140))),
                ]));
            }

            for h in tests.helpers.iter().take(5) {
                lines.push(Line::from(vec![
                    Span::raw("  • "),
                    Span::styled(h, Style::default().fg(Color::Gray)),
                ]));
            }
        }
    }

    /* LANGUAGE */
    if let Some(lang) = &state.lifecycle.language {
        let (label, _) = language_badge(&format!("{:?}", lang));
        lines.push(Line::from(vec![
            Span::styled("Language", Style::default().fg(Color::Rgb(140,140,140))),
            Span::raw(": "),
            Span::styled(label, Style::default().fg(Color::Rgb(0,220,140))),
        ]));
    }

    /* TEST SUITE */
    if state.context.last_suite_report.is_some() {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled("Test Suite", Style::default().fg(Color::Rgb(140,140,140))),
            Span::raw(": "),
            Span::styled("Report generated", Style::default().fg(Color::Rgb(0,220,140))),
        ]));
    }

    f.render_widget(Paragraph::new(lines), chunks[0]);

    /* FOOTER INFO */

    let repo_name = state.repo_root.file_name().and_then(|s| s.to_str()).unwrap_or("<unknown>");

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Repo", Style::default().fg(Color::Rgb(140,140,140))),
            Span::raw(": "),
            Span::styled(format!("~/{}", repo_name), Style::default().fg(Color::White)),
        ])),
        chunks[1],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Version", Style::default().fg(Color::Rgb(140,140,140))),
            Span::raw(": "),
            Span::styled(
                format!("osmogrep {}", app_version()),
                Style::default().fg(Color::Rgb(0,220,140)),
            ),
        ])),
        chunks[2],
    );

    /* MODEL */
    let model = match &state.llm_backend {
        crate::llm::backend::LlmBackend::Remote { client } =>
            format!("{:?}", client.current_config().provider).to_uppercase(),
        crate::llm::backend::LlmBackend::Ollama { .. } =>
            "OLLAMA".to_string(),
    };

    let mut spans = vec![
        Span::styled("Model", Style::default().fg(Color::Rgb(140,140,140))),
        Span::raw(": "),
        Span::styled(model, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ];

    if state.lifecycle.phase == crate::state::Phase::Running {
        if let Some(pulse) = running_pulse(state.ui.spinner_started_at) {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(pulse, Style::default().fg(Color::Rgb(0,220,140))));
        }
    }

    f.render_widget(Paragraph::new(Line::from(spans)), chunks[3]);
}
