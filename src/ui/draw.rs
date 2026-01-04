use std::io;

use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Terminal,
};

use unicode_width::UnicodeWidthStr;

use crate::state::{AgentState, Focus, InputMode, Phase};
use crate::ui::{diff, execution, panels, status};

pub fn draw_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<(Rect, Rect, Rect)> {
    let mut input_rect = Rect::default();
    let mut diff_rect = Rect::default();
    let mut exec_rect = Rect::default();

    terminal.draw(|f| {
        // ───────────────── HEADER + MAIN ─────────────────
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(6), // header
                Constraint::Min(1),    // everything else
            ])
            .split(f.size());

        // Header
        status::render_status(f, layout[0], state);

        // ───────────────── MAIN HORIZONTAL SPLIT ─────────────────
        let main_horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(1),     // left (main + command)
                Constraint::Length(36), // right (agent + context)
            ])
            .split(layout[1]);

        // ───────────────── LEFT SIDE (MAIN + COMMAND) ─────────────────
        let left_side = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // main content
                Constraint::Length(3), // command box
            ])
            .split(main_horizontal[0]);

        let main_area = left_side[0];

        // Render main content
        let mut rendered = false;

        if panels::render_panel(f, main_area, state) {
            rendered = true;
        }

        if !rendered {
            if let Some(idx) = state.ui.selected_diff {
                if let Some(delta) = &state.context.diff_analysis[idx].delta {
                    diff::render_side_by_side(f, main_area, delta, state);
                    diff_rect = main_area;
                    rendered = true;
                }
            }
        }

        if !rendered {
            execution::render_execution(f, main_area, state);
        }

        exec_rect = main_area;

        // ───────────────── RIGHT PANEL (FULL HEIGHT) ─────────────────
        status::render_side_status(f, main_horizontal[1], state);

        // ───────────────── COMMAND BOX ─────────────────
        let in_api_key_mode = matches!(state.ui.input_mode, InputMode::ApiKey { .. });
        let prompt = if in_api_key_mode { "key> " } else { ">_ " };

        let raw_input = if in_api_key_mode {
            let len = state.ui.input.chars().count();
            if len == 0 {
                String::new()
            } else {
                let frame = (state.ui.last_activity.elapsed().as_millis() / 120) as usize;
                let dots = ["•", "••", "•••", "••••"];
                let anim = dots[frame % dots.len()];
                if len == 1 {
                    anim.to_string()
                } else {
                    format!("{}{}", "•".repeat(len - 1), anim)
                }
            }
        } else {
            state.ui.input.clone()
        };

        let max_width = left_side[1]
            .width
            .saturating_sub(prompt.len() as u16 + 2) as usize;

        let visible_input = {
            let width = UnicodeWidthStr::width(raw_input.as_str());
            if width <= max_width {
                raw_input
            } else {
                raw_input
                    .chars()
                    .rev()
                    .scan(0, |w, c| {
                        *w += UnicodeWidthStr::width(c.to_string().as_str());
                        (*w <= max_width).then_some(c)
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            }
        };

        let mut spans = vec![Span::styled(prompt, Style::default().fg(Color::Cyan))];

        if visible_input.is_empty() {
            if let Some(ph) = &state.ui.input_placeholder {
                spans.push(Span::styled(ph, Style::default().fg(Color::DarkGray)));
            }
        } else {
            spans.push(Span::styled(visible_input, Style::default().fg(Color::White)));
        }

        if !in_api_key_mode {
            if let Some(ac) = &state.ui.autocomplete {
                if ac.starts_with(&state.ui.input) {
                    let suffix: String =
                        ac.chars().skip(state.ui.input.chars().count()).collect();
                    if !suffix.is_empty() {
                        spans.push(Span::styled(suffix, Style::default().fg(Color::Gray)));
                    }
                }
            }
        }

        let input = Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title("COMMAND")
                .title_alignment(Alignment::Center),
        );

        input_rect = left_side[1];
        f.render_widget(input, input_rect);

        // Cursor
        if state.ui.focus == Focus::Input {
            let prompt_w = prompt.len();
            let input_w = UnicodeWidthStr::width(state.ui.input.as_str());
            let max_visible =
                input_rect.width.saturating_sub(prompt_w as u16 + 2) as usize;

            f.set_cursor(
                input_rect.x + prompt_w as u16 + input_w.min(max_visible) as u16,
                input_rect.y + 1,
            );
        }
    })?;

    Ok((input_rect, diff_rect, exec_rect))
}
