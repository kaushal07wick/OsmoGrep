//! ui/execution.rs
//! Clean, flat execution log renderer.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

use std::time::{Duration, Instant};

use crate::state::{AgentState, LogLevel};
use crate::ui::helpers::risk_color;


fn parse_change_line(s: &str) -> Option<(String, String, Option<String>)> {
    let s = s.trim();
    if !s.starts_with('[') {
        return None;
    }
    let (idx, rest) = s.split_once("] ")?;
    
    let index = format!("[{}]", idx.trim_start_matches('['));
    let (file, symbol) = match rest.split_once(" :: ") {
        Some((f, sym)) if sym != "<file>" => (f.to_string(), Some(sym.to_string())),
        Some((f, _)) => (f.to_string(), None),
        None => (rest.to_string(), None),
    };
    Some((index, file, symbol))
}


pub fn render_execution(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {

    f.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(22,22,22))),
        area,
    );

    let padded = Rect {
        x: area.x + 3, 
        y: area.y + 1,
        width: area.width.saturating_sub(6), 
        height: area.height.saturating_sub(3),
    };

    let height = padded.height.max(1) as usize;

    let mut lines: Vec<Line> = Vec::new();

    let now = Instant::now();
    let fade_after = Duration::from_secs(3);

    const INDENT: &str = "  ";
    const BULLET: &str = "› "; // modern bullet

    let mut changes_heading_shown = false;


    /* ------------------------------------------------------
       BUILD LOG LINES
    ------------------------------------------------------ */
    for log in state.logs.iter() {
        let faded = matches!(log.level, LogLevel::Warn | LogLevel::Error)
            && now.duration_since(log.at) > fade_after;


        /* ---------------- DIFF ANALYSIS ---------------- */
        if log.text == "__DIFF_ANALYSIS__" {
            lines.push(Line::default());
            lines.push(Line::from(vec![
                Span::raw(INDENT),
                Span::styled("Diff Analysis", Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::default());

            for (idx, d) in state.context.diff_analysis.iter().enumerate() {
                let mut s = vec![
                    Span::styled(format!("[{}] ", idx), Style::default().fg(Color::Gray)),
                    Span::styled(&d.file, Style::default().fg(Color::Gray)),
                ];

                if let Some(sym) = &d.symbol {
                    s.push(Span::raw(" :: "));
                    s.push(Span::styled(sym, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)));
                }

                s.push(Span::raw(" | "));
                s.push(Span::styled(format!("{:?}", d.surface), Style::default().fg(Color::Gray)));

                if let Some(summary) = &d.summary {
                    s.push(Span::raw(" | "));
                    s.push(Span::styled(format!("{:?}", summary.risk), Style::default().fg(risk_color(&summary.risk)).add_modifier(Modifier::BOLD)));
                }

                lines.push(Line::from(
                    std::iter::once(Span::raw(INDENT)).chain(s).collect::<Vec<_>>()
                ));

                if let Some(summary) = &d.summary {
                    lines.push(Line::from(vec![
                        Span::raw("     ↳ "),
                        Span::styled(summary.behavior.clone(), Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
                    ]));
                }

                lines.push(Line::default());
            }

            continue;
        }


        /* ---------------- CHANGE LIST ---------------- */
        if let Some((idx, file, symbol)) = parse_change_line(&log.text) {
            if !changes_heading_shown {
                lines.push(Line::default());
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled("Changes", Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)),
                ]));
                lines.push(Line::default());
                changes_heading_shown = true;
            }

            let mut s = vec![
                Span::styled(idx + " ", Style::default().fg(Color::Gray)),
                Span::styled(file, Style::default().fg(Color::Gray)),
            ];

            if let Some(sym) = symbol {
                s.push(Span::raw(" :: "));
                s.push(Span::styled(sym, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)));
            }

            lines.push(Line::from(
                std::iter::once(Span::raw(INDENT)).chain(s).collect::<Vec<_>>()
            ));
            lines.push(Line::default());
            continue;
        }


        /* ---------------- NORMAL LOGS ---------------- */
        let color = match log.level {
            LogLevel::Info => Color::Gray,
            LogLevel::Success => Color::Green,
            LogLevel::Warn => Color::Yellow,
            LogLevel::Error => Color::Red,
        };

        let faded_color = if faded { Color::DarkGray } else { color };


        lines.push(Line::from(vec![
            Span::raw(INDENT),
            Span::styled(BULLET, Style::default().fg(faded_color)),
            Span::styled(&log.text, Style::default().fg(faded_color)),
        ]));

        lines.push(Line::default());
    }


    /* ------------------------------------------------------
       SCROLLING
    ------------------------------------------------------ */
    let content_len = lines.len();
    let max_scroll = content_len.saturating_sub(height);

    let scroll = if state.ui.exec_scroll == usize::MAX {
        max_scroll
    } else {
        state.ui.exec_scroll.min(max_scroll)
    };


    /* ------------------------------------------------------
       FINAL RENDER
    ------------------------------------------------------ */
    f.render_widget(
        Paragraph::new(lines)
            .scroll((scroll as u16, 0))
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(Color::Rgb(22,22,22))) // SAME AS ROOT
            .alignment(Alignment::Left),
        padded,
    );
}
