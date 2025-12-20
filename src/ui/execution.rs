//! ui/execution.rs
//!
//! Execution / log renderer.
//!
//! DEFAULT fallback view.
//! Shows logs + raw diff analysis facts (no semantics).

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use std::time::{Duration, Instant};

use crate::state::{AgentState, LogLevel};
use crate::ui::helpers::{
    symbol_style,
    surface_style,
    keyword_style,
};

/* ============================================================
   Public API
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

    for log in state.logs.iter() {
        // fade old warnings/errors
        if matches!(log.level, LogLevel::Warn | LogLevel::Error)
            && now.duration_since(log.at) > fade_after
        {
            continue;
        }

        // Diff analysis marker
        if log.text == "__DIFF_ANALYSIS__" {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Diff Analysis",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            while diff_idx < state.context.diff_analysis.len() {
                let d = &state.context.diff_analysis[diff_idx];
                diff_idx += 1;

                let delta_marker = if d.delta.is_some() {
                    Span::styled("Δ", Style::default().fg(Color::Green))
                } else {
                    Span::styled("—", Style::default().fg(Color::DarkGray))
                };

                let mut spans: Vec<Span> = Vec::new();

                // file
                spans.push(Span::styled(
                    &d.file,
                    Style::default().fg(Color::White),
                ));

                // symbol (if any)
                if let Some(sym) = &d.symbol {
                    spans.push(Span::raw(" :: "));
                    spans.push(Span::styled(sym, symbol_style()));
                }

                spans.push(Span::raw("  "));
                spans.push(delta_marker);
                spans.push(Span::raw("  "));

                // change surface (semantic color)
                spans.push(Span::styled(
                    format!("{:?}", d.surface),
                    surface_style(&d.surface),
                ));

                lines.push(Line::from(spans));
            }

            continue;
        }

        // Normal log rendering
        let color = match log.level {
            LogLevel::Info => Color::White,
            LogLevel::Success => Color::Green,
            LogLevel::Warn => Color::Yellow,
            LogLevel::Error => Color::Red,
        };

        let style = if matches!(log.level, LogLevel::Warn | LogLevel::Error)
            && now.duration_since(log.at).as_secs_f32() > 2.0
        {
            Style::default().fg(color).add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(color)
        };

        lines.push(Line::from(Span::styled(&log.text, style)));
    }

    let total = lines.len();
    let max_scroll = total.saturating_sub(height);
    let scroll = state.ui.exec_scroll.min(max_scroll);

    let visible = lines
        .into_iter()
        .skip(scroll)
        .take(height)
        .collect::<Vec<_>>();

    let block = Block::default()
        .borders(Borders::ALL)
        .title("EXECUTION")
        .title_alignment(Alignment::Center);

    f.render_widget(Paragraph::new(visible).block(block), area);
}
