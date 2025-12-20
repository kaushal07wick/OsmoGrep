//! ui/execution.rs
//!
//! Execution / log renderer.
//!
//! Compact, hierarchical diff + semantic view.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use std::time::{Duration, Instant};

use crate::state::{AgentState, LogLevel};
use crate::ui::helpers::risk_color;

pub fn render_execution(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    let height = area.height.saturating_sub(2) as usize;
    let now = Instant::now();
    let fade_after = Duration::from_secs(3);

    let mut lines: Vec<Line> = Vec::new();
    let mut diff_idx = 0;
    let mut counter = 1;

    for log in state.logs.iter() {
        // Fade old warnings/errors
        if matches!(log.level, LogLevel::Warn | LogLevel::Error)
            && now.duration_since(log.at) > fade_after
        {
            continue;
        }

        /* ================= DIFF ANALYSIS ================= */
        if log.text == "__DIFF_ANALYSIS__" {
            lines.push(Line::from(Span::styled(
                "Diff Analysis",
                Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            while diff_idx < state.context.diff_analysis.len() {
                let d = &state.context.diff_analysis[diff_idx];
                diff_idx += 1;

                /* ---------- primary line ---------- */
                let mut header = vec![
                    Span::styled(
                        format!("[{:02}] ", counter),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        &d.file,
                        Style::default().fg(Color::White),
                    ),
                ];

                if let Some(sym) = &d.symbol {
                    header.push(Span::raw(" :: "));
                    header.push(Span::styled(
                        sym,
                        Style::default().fg(Color::White),
                    ));
                }

                header.push(Span::raw(" | "));
                header.push(Span::styled(
                    format!("{:?}", d.surface),
                    Style::default().fg(Color::DarkGray),
                ));

                if let Some(summary) = &d.summary {
                    header.push(Span::raw(" | "));
                    header.push(Span::styled(
                        format!("{:?}", summary.risk),
                        Style::default()
                            .fg(risk_color(&summary.risk))
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                lines.push(Line::from(header));

                /* ---------- semantic (ONE line only) ---------- */
                if let Some(summary) = &d.summary {
                    lines.push(Line::from(vec![
                        Span::raw("     "),
                        Span::styled(
                            format!("↳ {}", summary.behavior),
                            Style::default()
                                .fg(Color::DarkGray)
                                .add_modifier(Modifier::ITALIC),
                        ),
                    ]));
                }

                // spacing between entries
                lines.push(Line::from(""));

                counter += 1;
            }

            continue;
        }

        /* ================= NORMAL LOGS ================= */
        let color = match log.level {
            LogLevel::Info => Color::DarkGray,
            LogLevel::Success => Color::Green,
            LogLevel::Warn => Color::Yellow,
            LogLevel::Error => Color::Red,
        };

        lines.push(Line::from(Span::styled(
            &log.text,
            Style::default().fg(color),
        )));
    }

    /* ---------- scrolling ---------- */
    let max_scroll = lines.len().saturating_sub(height);
    let scroll = state.ui.exec_scroll.min(max_scroll);

    let visible = lines
        .into_iter()
        .skip(scroll)
        .take(height)
        .collect::<Vec<_>>();

    /* ---------- render ---------- */
    f.render_widget(
        Paragraph::new(visible).block(
            Block::default()
                .borders(Borders::ALL)
                .title("EXECUTION")
                .title_alignment(Alignment::Center),
        ),
        area,
    );
}
