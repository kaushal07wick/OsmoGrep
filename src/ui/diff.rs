//! ui/diff.rs
//!
//! Side-by-side diff renderer

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use similar::{ChangeTag, TextDiff};

use crate::state::{AgentState, Focus, SymbolDelta};
use crate::ui::helpers::{hclip, ln};

fn bar(color: Color) -> Span<'static> {
    Span::styled(" ▌ ", Style::default().fg(color))
}

fn empty_pad() -> Span<'static> {
    Span::raw("   ")
}

pub fn render_side_by_side(
    f: &mut ratatui::Frame,
    area: Rect,
    delta: &SymbolDelta,
    state: &AgentState,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let gutter = 5;
    let width = chunks[0]
        .width
        .saturating_sub(gutter + 1) as usize;
    let height = area.height.saturating_sub(2) as usize;

    let diff = TextDiff::from_lines(
        &delta.old_source,
        &delta.new_source,
    );

    let changes: Vec<_> = diff.iter_all_changes().collect();
    let total = changes.len();

    let max_scroll = total.saturating_sub(height);
    let start = state.ui.diff_scroll.min(max_scroll);
    let end = (start + height).min(total);

    let mut old_ln = 1usize;
    let mut new_ln = 1usize;

    for change in &changes[..start] {
        match change.tag() {
            ChangeTag::Equal => {
                old_ln += 1;
                new_ln += 1;
            }
            ChangeTag::Delete => old_ln += 1,
            ChangeTag::Insert => new_ln += 1,
        }
    }

    let mut left: Vec<Line> = Vec::with_capacity(end - start);
    let mut right: Vec<Line> = Vec::with_capacity(end - start);

    for change in &changes[start..end] {
        match change.tag() {
            ChangeTag::Equal => {
                let text = hclip(
                    change.value(),
                    state.ui.diff_scroll_x,
                    width,
                );

                let dim = Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM);

                left.push(Line::from(vec![
                    bar(Color::DarkGray),
                    ln(old_ln, Color::DarkGray),
                    Span::styled(text.clone(), dim),
                ]));

                right.push(Line::from(vec![
                    bar(Color::DarkGray),
                    ln(new_ln, Color::DarkGray),
                    Span::styled(text, dim),
                ]));

                old_ln += 1;
                new_ln += 1;
            }

            ChangeTag::Delete => {
                let text = hclip(
                    change.value(),
                    state.ui.diff_scroll_x,
                    width,
                );

                left.push(Line::from(vec![
                    bar(Color::Red),
                    ln(old_ln, Color::Red),
                    Span::styled(
                        text,
                        Style::default().fg(Color::Red),
                    ),
                ]));

                right.push(Line::from(vec![empty_pad()]));

                old_ln += 1;
            }

            ChangeTag::Insert => {
                let text = hclip(
                    change.value(),
                    state.ui.diff_scroll_x,
                    width,
                );

                left.push(Line::from(vec![empty_pad()]));

                right.push(Line::from(vec![
                    bar(Color::Green),
                    ln(new_ln, Color::Green),
                    Span::styled(
                        text,
                        Style::default().fg(Color::Green),
                    ),
                ]));

                new_ln += 1;
            }
        }
    }

    let title_style = if state.ui.focus == Focus::Diff {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    f.render_widget(
        Paragraph::new(left).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled("OLD", title_style))
                .title_alignment(Alignment::Center),
        ),
        chunks[0],
    );

    f.render_widget(
        Paragraph::new(right).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled("NEW", title_style))
                .title_alignment(Alignment::Center),
        ),
        chunks[1],
    );
}
