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

/* ============================================================
   Helpers
   ============================================================ */

/// Parses lines like:
/// "[2] src/foo.rs :: bar_fn"
fn parse_change_line(s: &str) -> Option<(String, String, Option<String>)> {
    let s = s.trim();
    if !s.starts_with('[') {
        return None;
    }

    let (idx, rest) = s.split_once("] ")?;
    let index = format!("{}]", idx.trim_start_matches('['));

    let (file, symbol) = match rest.split_once(" :: ") {
        Some((f, sym)) if sym != "<file>" => (f.to_string(), Some(sym.to_string())),
        Some((f, _)) => (f.to_string(), None),
        None => (rest.to_string(), None),
    };

    Some((index, file, symbol))
}

/* ============================================================
   Renderer
   ============================================================ */

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
    let mut diff_counter = 0;
    let mut changes_heading_shown = false;

    for log in state.logs.iter() {
        // fade old warnings/errors
        if matches!(log.level, LogLevel::Warn | LogLevel::Error)
            && now.duration_since(log.at) > fade_after
        {
            continue;
        }

        /* ================= DIFF ANALYSIS ================= */
        if log.text == "__DIFF_ANALYSIS__" {
            lines.push(Line::from(""));
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

                let mut header = vec![
                    Span::styled(
                        format!("[{}] ", diff_counter),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        &d.file,
                        Style::default().fg(Color::Gray),
                    ),
                ];

                if let Some(sym) = &d.symbol {
                    header.push(Span::raw(" :: "));
                    header.push(Span::styled(
                        sym,
                        Style::default()
                            .fg(Color::Cyan) // 🔵 Rust-style function color
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
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

                lines.push(Line::from(""));
                diff_counter += 1;
            }

            continue;
        }

        /* ================= CHANGES LIST ================= */
        if let Some((idx, file, symbol)) = parse_change_line(&log.text) {
            if !changes_heading_shown {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Changes",
                    Style::default()
                        .fg(Color::Gray)
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                changes_heading_shown = true;
            }

            let mut spans = vec![
                Span::styled(
                    idx + " ",
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    file,
                    Style::default().fg(Color::Gray),
                ),
            ];

            if let Some(sym) = symbol {
                spans.push(Span::raw(" :: "));
                spans.push(Span::styled(
                    sym,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ));
            }

            lines.push(Line::from(spans));
            lines.push(Line::from(""));
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
        lines.push(Line::from(""));
    }

    /* ================= SCROLL ================= */
    let max_scroll = lines.len().saturating_sub(height);
    let scroll = state.ui.exec_scroll.min(max_scroll);

    let visible = lines
        .into_iter()
        .skip(scroll)
        .take(height)
        .collect::<Vec<_>>();

    /* ================= RENDER ================= */
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
