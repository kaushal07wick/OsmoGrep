//! ui/diff.rs — clean, spaced side-by-side diff view

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use similar::{ChangeTag, TextDiff};

use crate::state::{AgentState, Focus, SymbolDelta};
use crate::ui::helpers::{hclip, ln};

fn empty_pad() -> Span<'static> {
    Span::raw("   ")
}

pub fn render_side_by_side(
    f: &mut ratatui::Frame,
    area: Rect,
    delta: &SymbolDelta,
    state: &AgentState,
) {
    /* Background */
    f.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(28, 28, 28))),
        area,
    );

    /* Outer padding: top + bottom + left + right */
    let padded = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(3), // ↓ extra bottom gap
    };

    /* Titles (OLD / NEW) */
    let title_row = Rect {
        x: padded.x,
        y: padded.y,
        width: padded.width,
        height: 1,
    };

    let title_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(title_row);

    let title_style = if state.ui.focus == Focus::Diff {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Rgb(150, 150, 150))
    };

    f.render_widget(
        Paragraph::new("OLD").alignment(Alignment::Center).style(title_style),
        title_split[0],
    );

    f.render_widget(
        Paragraph::new("NEW").alignment(Alignment::Center).style(title_style),
        title_split[1],
    );

    /* Content region under titles */
    let body = Rect {
        x: padded.x,
        y: padded.y + 1,
        width: padded.width,
        height: padded.height.saturating_sub(1),
    };

    /* Natural column spacing */
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(body);

    let left_col = Rect {
        x: cols[0].x,
        y: cols[0].y,
        width: cols[0].width.saturating_sub(1),
        height: cols[0].height,
    };

    let right_col = Rect {
        x: cols[1].x + 1,
        y: cols[1].y,
        width: cols[1].width.saturating_sub(1),
        height: cols[1].height,
    };

    /* Prepare diff */
    let diff = TextDiff::from_lines(&delta.old_source, &delta.new_source);
    let entries: Vec<_> = diff.iter_all_changes().collect();

    let max_scroll = entries.len().saturating_sub(left_col.height as usize);
    let start = state.ui.diff_scroll.min(max_scroll);
    let end = (start + left_col.height as usize).min(entries.len());

    let mut old_ln = 1usize;
    let mut new_ln = 1usize;

    for change in &entries[..start] {
        match change.tag() {
            ChangeTag::Equal => { old_ln += 1; new_ln += 1; }
            ChangeTag::Delete => old_ln += 1,
            ChangeTag::Insert => new_ln += 1,
        }
    }

    let mut out_left = Vec::new();
    let mut out_right = Vec::new();

    /* Build visible lines */
    for change in &entries[start..end] {
        match change.tag() {
            ChangeTag::Equal => {
                let t_left = hclip(change.value(), state.ui.diff_scroll_x, left_col.width as usize);
                let t_right = hclip(change.value(), state.ui.diff_scroll_x, right_col.width as usize);

                let style = Style::default().fg(Color::Rgb(220, 220, 220));

                out_left.push(Line::from(vec![
                    ln(old_ln, Color::Rgb(130,130,130)),
                    Span::raw("  "),
                    Span::styled(t_left, style),
                ]));

                out_right.push(Line::from(vec![
                    ln(new_ln, Color::Rgb(130,130,130)),
                    Span::raw("  "),
                    Span::styled(t_right, style),
                ]));

                old_ln += 1;
                new_ln += 1;
            }

            ChangeTag::Delete => {
                let t = hclip(change.value(), state.ui.diff_scroll_x, left_col.width as usize);

                out_left.push(Line::from(vec![
                    ln(old_ln, Color::Red),
                    Span::raw("  "),
                    Span::styled(t, Style::default().fg(Color::Red)),
                ]));

                out_right.push(Line::from(empty_pad()));

                old_ln += 1;
            }

            ChangeTag::Insert => {
                let t = hclip(change.value(), state.ui.diff_scroll_x, right_col.width as usize);

                out_left.push(Line::from(empty_pad()));

                out_right.push(Line::from(vec![
                    ln(new_ln, Color::Green),
                    Span::raw("  "),
                    Span::styled(t, Style::default().fg(Color::Green)),
                ]));

                new_ln += 1;
            }
        }
    }

    /* Render panels */
    f.render_widget(
        Paragraph::new(out_left).style(Style::default().bg(Color::Rgb(28,28,28))),
        left_col,
    );

    f.render_widget(
        Paragraph::new(out_right).style(Style::default().bg(Color::Rgb(28,28,28))),
        right_col,
    );
}
