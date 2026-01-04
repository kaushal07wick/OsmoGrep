use std::io;

use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, BorderType},
    Terminal,
};

use unicode_width::UnicodeWidthStr;

use crate::state::{AgentState, Focus, InputMode};
use crate::ui::{diff, panels, status, execution};

pub fn draw_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<(Rect, Rect, Rect)> {
    let mut input_rect = Rect::default();
    let mut diff_rect = Rect::default();
    let mut exec_rect = Rect::default();

    terminal.draw(|f| {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(14),
                Constraint::Min(6),
                Constraint::Length(3),
            ])
            .split(f.size());

        status::render_status(f, layout[0], state);

        let mut rendered = false;

        if panels::render_panel(f, layout[1], state) {
            rendered = true;
        }

        if !rendered {
            if let Some(idx) = state.ui.selected_diff {
                if let Some(delta) = &state.context.diff_analysis[idx].delta {
                    diff::render_side_by_side(f, layout[1], delta, state);
                    diff_rect = layout[1];
                    rendered = true;
                }
            }
        }

        if !rendered {
            execution::render_execution(f, layout[1], state);
        }

        exec_rect = layout[1];

        let in_api_key_mode = matches!(state.ui.input_mode, InputMode::ApiKey { .. });
        let prompt = if in_api_key_mode { "key> " } else { ">_ " };

        // -------- Masked / normal input (UTF-8 safe) --------
        let raw_input: String = if in_api_key_mode {
            let len = state.ui.input.chars().count();
            if len == 0 {
                String::new()
            } else {
                let frame =
                    (state.ui.last_activity.elapsed().as_millis() / 120) as usize;
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

        // -------- Horizontal clipping (char-safe) --------
        let max_width = layout[2]
            .width
            .saturating_sub(prompt.len() as u16 + 2) as usize;

        let visible_input: String = {
            let width = UnicodeWidthStr::width(raw_input.as_str());
            if width <= max_width {
                raw_input.clone()
            } else {
                raw_input
                    .chars()
                    .rev()
                    .scan(0, |w, c| {
                        *w += UnicodeWidthStr::width(c.to_string().as_str());
                        if *w <= max_width {
                            Some(c)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            }
        };

        let mut spans = vec![
            Span::styled(prompt, Style::default().fg(Color::Cyan)),
        ];

        if visible_input.is_empty() {
            if let Some(ph) = &state.ui.input_placeholder {
                spans.push(Span::styled(
                    ph,
                    Style::default().fg(Color::DarkGray),
                ));
            }
        } else {
            spans.push(Span::styled(
                visible_input.clone(),
                Style::default().fg(Color::White),
            ));
        }

        // -------- Autocomplete (UTF-8 safe) --------
        if !in_api_key_mode {
            if let Some(ac) = &state.ui.autocomplete {
                if ac.starts_with(&state.ui.input) {
                    let input_chars = state.ui.input.chars().count();
                    let suffix: String =
                        ac.chars().skip(input_chars).collect();

                    if !suffix.is_empty() {
                        spans.push(Span::styled(
                            suffix,
                            Style::default().fg(Color::Gray),
                        ));
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

        input_rect = layout[2];
        f.render_widget(input, input_rect);

        // -------- Cursor (char-width safe, capped) --------
        if state.ui.focus == Focus::Input {
            let prompt_w = prompt.len();
            let input_w = UnicodeWidthStr::width(state.ui.input.as_str());

            let max_visible =
                input_rect.width.saturating_sub(prompt_w as u16 + 2) as usize;

            let cursor_offset = input_w.min(max_visible);

            let cursor_x =
                input_rect.x + prompt_w as u16 + cursor_offset as u16;
            let cursor_y = input_rect.y + 1;

            f.set_cursor(cursor_x, cursor_y);
        }
    })?;

    Ok((input_rect, diff_rect, exec_rect))
}
