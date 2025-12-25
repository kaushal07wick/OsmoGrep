//! ui/panels.rs
//!
//! Single-panel renderers (LLM output, test results).

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::state::{AgentState, SinglePanelView};
use crate::ui::helpers::{keyword_style, symbol_style};

pub fn render_panel(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) -> bool {
    match &state.ui.panel_view {
        Some(SinglePanelView::TestGenPreview {
            candidate,
            generated_test,
        }) => {
            render_testgen_panel(f, area, state, candidate, generated_test);
            true
        }

        Some(SinglePanelView::TestResult { output, passed }) => {
            render_test_result_panel(f, area, state, output, *passed);
            true
        }

        None => false,
    }
}


fn render_testgen_panel(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
    candidate: &crate::testgen::candidate::TestCandidate,
    generated_test: &Option<String>,
) {
    let mut lines: Vec<Line> = Vec::new();

    /* ---------- HEADER ---------- */
    lines.push(Line::from(Span::styled(
        "🤖 GENERATED TEST",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    /* ---------- META ---------- */
    lines.push(Line::from(vec![
        Span::styled("File: ", keyword_style()),
        Span::raw(&candidate.file),
    ]));

    if let Some(sym) = &candidate.symbol {
        lines.push(Line::from(vec![
            Span::styled("Symbol: ", keyword_style()),
            Span::styled(sym, symbol_style()),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "TEST CODE",
        Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    /* ---------- BODY ---------- */
    match generated_test {
        Some(code) => {
            for l in code.lines() {
                lines.push(Line::from(Span::styled(
                    l,
                    Style::default().fg(Color::White),
                )));
            }
        }
        None => {
            lines.push(Line::from(Span::styled(
                "Generating test…",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )));
        }
    }

    render_scrollable_block(
        f,
        area,
        state.ui.panel_scroll,
        lines,
        "LLM OUTPUT",
    );
}


fn render_test_result_panel(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
    output: &str,
    passed: bool,
) {
    let mut lines: Vec<Line> = Vec::new();

    /* ---------- HEADER ---------- */
    lines.push(Line::from(Span::styled(
        if passed { "✔ TEST PASSED" } else { "✖ TEST FAILED" },
        Style::default()
            .fg(if passed { Color::Green } else { Color::Red })
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    /* ---------- OUTPUT ---------- */
    if output.is_empty() {
        lines.push(Line::from(Span::styled(
            "No test output.",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));
    } else {
        for l in output.lines() {
            lines.push(Line::from(Span::styled(
                l,
                Style::default().fg(Color::White),
            )));
        }
    }
    render_scrollable_block(
        f,
        area,
        state.ui.panel_scroll,
        lines,
        "TEST RESULT",
    );
}


fn render_scrollable_block(
    f: &mut ratatui::Frame,
    area: Rect,
    scroll: usize,
    lines: Vec<Line>,
    title: &str,
) {
    let height = area.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(height);
    let scroll = scroll.min(max_scroll);

    let visible = lines
        .into_iter()
        .skip(scroll)
        .take(height)
        .collect::<Vec<_>>();

    f.render_widget(
        Paragraph::new(visible).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_alignment(Alignment::Center),
        ),
        area,
    );
}
