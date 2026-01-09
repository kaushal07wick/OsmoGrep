use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
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

pub const PRIMARY_GREEN: Color = Color::Rgb(0, 220, 140);

pub fn render_side_status(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    // Outer padded background
    let padded = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(2),
        height: area.height,
    };

    f.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(32, 32, 32))),
        padded,
    );

    // Internal padding
    let inner = Rect {
        x: padded.x + 2,
        y: padded.y + 1,
        width: padded.width.saturating_sub(4),
        height: padded.height.saturating_sub(2),
    };

    // Vertical layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),  // SYSTEM section
            Constraint::Min(6),  // CONTEXT section
            Constraint::Length(1), // Repo
            Constraint::Length(1), // Version + Model + Pulse
        ])
        .split(inner);

    let mut sys_lines = Vec::<Line>::new();
    let mut ctx_lines = Vec::<Line>::new();

    sys_lines.push(Line::from(
        Span::styled(
            "SYSTEM //",
            Style::default().fg(PRIMARY_GREEN).add_modifier(Modifier::BOLD),
        ),
    ));
    sys_lines.push(Line::default());

    let (_, phase_label, _) = phase_badge(&state.lifecycle.phase);

    sys_lines.push(Line::from(vec![
        Span::styled("Status", Style::default().fg(Color::Rgb(140, 140, 140))),
        Span::raw(": "),
        Span::styled(
            phase_label,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
    ]));

    if let Some(branch) = &state.lifecycle.current_branch {
        sys_lines.push(Line::from(vec![
            Span::styled("Branch", Style::default().fg(Color::Rgb(140, 140, 140))),
            Span::raw(": "),
            Span::styled(branch, Style::default().fg(Color::White)),
        ]));
    }

    sys_lines.push(Line::from(vec![
        Span::styled("Agent", Style::default().fg(Color::Rgb(140, 140, 140))),
        Span::raw(": "),
        Span::styled(
            state.lifecycle.agent_branch.as_deref().unwrap_or("None"),
            Style::default().fg(Color::White),
        ),
    ]));

    // Live memory info
    let mut sysinfo = sysinfo::System::new();
    sysinfo.refresh_memory();

    sys_lines.push(Line::from(vec![
        Span::styled("RAM", Style::default().fg(Color::Rgb(140, 140, 140))),
        Span::raw(": "),
        Span::styled(
            format!(
                "{:.1}/{:.1} GB",
                sysinfo.used_memory() as f64 / 1_073_741_824.0,
                sysinfo.total_memory() as f64 / 1_073_741_824.0,
            ),
            Style::default().fg(Color::White),
        ),
    ]));

    sys_lines.push(Line::from(vec![
        Span::styled("Uptime", Style::default().fg(Color::Rgb(140, 140, 140))),
        Span::raw(": "),
        Span::styled(
            format_uptime(state.started_at),
            Style::default().fg(Color::Gray),
        ),
    ]));

    f.render_widget(Paragraph::new(sys_lines), chunks[0]);

    ctx_lines.push(Line::from(
        Span::styled(
            "CONTEXT //",
            Style::default().fg(PRIMARY_GREEN).add_modifier(Modifier::BOLD),
        ),
    ));
    ctx_lines.push(Line::default());

    if let Some(snapshot) = &state.full_context_snapshot {
        ctx_lines.push(Line::from(vec![
            Span::styled("Files", Style::default().fg(Color::Rgb(140, 140, 140))),
            Span::raw(": "),
            Span::styled(
                format!("{}", snapshot.code.files.len()),
                Style::default().fg(Color::White),
            ),
        ]));

        let tests = &snapshot.tests;

        ctx_lines.push(Line::from(vec![
            Span::styled("Tests", Style::default().fg(Color::Rgb(140, 140, 140))),
            Span::raw(": "),
            Span::styled(
                if tests.exists { "Present" } else { "None" },
                Style::default().fg(Color::White),
            ),
        ]));

        if tests.exists {
            if let Some(fw) = tests.framework {
                let (label, _) = framework_badge(&fw);
                ctx_lines.push(Line::from(vec![
                    Span::styled("Framework", Style::default().fg(Color::Rgb(140, 140, 140))),
                    Span::raw(": "),
                    Span::styled(label, Style::default().fg(PRIMARY_GREEN).add_modifier(Modifier::BOLD)),
                ]));
            }

            ctx_lines.push(Line::from(Span::styled(
                "Helpers",
                Style::default().fg(Color::Rgb(140, 140, 140)),
            )));

            for (i, h) in tests.helpers.iter().enumerate() {
                ctx_lines.push(Line::from(vec![
                    Span::styled(
                        format!("[{}] ", i + 1),
                        Style::default()
                            .fg(PRIMARY_GREEN)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(h, Style::default().fg(Color::White)),
                ]));
            }
        }
    }

    if let Some(lang) = &state.lifecycle.language {
        let (label, _) = language_badge(&format!("{:?}", lang));
        ctx_lines.push(Line::default());
        ctx_lines.push(Line::from(vec![
            Span::styled("Language", Style::default().fg(Color::Rgb(140, 140, 140))),
            Span::raw(": "),
            Span::styled(label, Style::default().fg(PRIMARY_GREEN)),
        ]));
    }

    if state.context.last_suite_report.is_some() {
        ctx_lines.push(Line::default());
        ctx_lines.push(Line::from(vec![
            Span::styled("Test Suite", Style::default().fg(Color::Rgb(140, 140, 140))),
            Span::raw(": "),
            Span::styled("Report generated", Style::default().fg(PRIMARY_GREEN)),
        ]));
    }

    f.render_widget(Paragraph::new(ctx_lines), chunks[1]);


    /* ==========================================================
   FOOTER (Repo)
========================================================== */

let repo_name = state
    .repo_root
    .file_name()
    .and_then(|s| s.to_str())
    .unwrap_or("<unknown>");

f.render_widget(
    Paragraph::new(Line::from(vec![
        Span::styled("Repo", Style::default().fg(Color::Rgb(140,140,140))),
        Span::raw(": "),
        Span::styled(format!("~/{}", repo_name), Style::default().fg(Color::White)),
    ])),
    chunks[2],
);
    let version_line = Line::from(vec![
        Span::styled("Version", Style::default().fg(Color::Rgb(140,140,140))),
        Span::raw(": "),
        Span::styled(format!("v{}", app_version()), Style::default().fg(PRIMARY_GREEN)),
    ]);

    let version_rect = Rect {
        x: chunks[3].x,
        y: chunks[3].y,
        width: chunks[3].width,
        height: 1,
    };

    f.render_widget(Paragraph::new(version_line), version_rect);

    let model_name = match &state.llm_backend {
        crate::llm::backend::LlmBackend::Remote { client } =>
            format!("{:?}", client.current_config().provider).to_uppercase(),
        crate::llm::backend::LlmBackend::Ollama { .. } =>
            "OLLAMA".into(),
    };

    let mut model_line = vec![
        Span::styled("Model", Style::default().fg(Color::Rgb(140,140,140))),
        Span::raw(": "),
        Span::styled(
            model_name,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
    ];

    if state.lifecycle.phase == crate::state::Phase::Running {
        if let Some(anim) = running_pulse(state.ui.spinner_started_at) {
            model_line.push(Span::raw("   "));
            model_line.push(Span::styled(anim, Style::default().fg(PRIMARY_GREEN)));
        }
    }

    let model_rect = Rect {
        x: chunks[3].x,
        y: chunks[3].y + 1,
        width: chunks[3].width,
        height: 1,
    };

    f.render_widget(Paragraph::new(Line::from(model_line)), model_rect);

}
