use std::{io};

use ratatui::{
    backend::Backend,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Terminal, Frame,
};

use crate::{
    logger::parse_user_input_log,
    state::{AgentState, InputMode},
};
use crate::ui::helper::{render_static_command_line, running_pulse, git_branch, calculate_input_lines};
const TOP_PADDING: u16 = 1;

const FG_MAIN: Color = Color::Rgb(220, 220, 220);
const FG_DIM: Color = Color::Rgb(170, 170, 170);
const FG_MUTED: Color = Color::Rgb(120, 120, 120);
const PROMPT: &str = ">_ ";

const LOGO: [&str; 7] = [
    " ██████  ███████ ███    ███  ██████   ██████  ██████  ███████ ██████  ",
    "██    ██ ██      ████  ████ ██    ██ ██       ██   ██ ██      ██   ██ ",
    "██    ██ ███████ ██ ████ ██ ██    ██ ██   ███ ██████  █████   ██████  ",
    "██    ██      ██ ██  ██  ██ ██    ██ ██    ██ ██   ██ ██      ██      ",
    " ██████  ███████ ██      ██  ██████   ██████  ██   ██ ███████ ██      ",
    "                                                                      ",
    "                                                                      ",
];


pub fn draw_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<(Rect, Rect, Rect)> {
    let mut input_rect = Rect::default();
    let mut exec_rect = Rect::default();

    terminal.draw(|f| {
        let area = f.size();

        let padded_area = Rect {
            x: area.x,
            y: area.y + TOP_PADDING,
            width: area.width,
            height: area.height.saturating_sub(TOP_PADDING),
        };

        let header_height = 1 + LOGO.len() as u16 + 2;
        let status_height = 1;

        let input_width = padded_area.width.saturating_sub(4) as usize;
        let input_lines =
            calculate_input_lines(&state.ui.input, input_width, PROMPT.len());

        let max_content_lines = 3;
        let visible_lines = input_lines.min(max_content_lines);
        let cmd_height = (visible_lines + 2) as u16;

        let header_rect = Rect {
            x: padded_area.x,
            y: padded_area.y,
            width: padded_area.width,
            height: header_height,
        };

        let status_rect = Rect {
            x: padded_area.x,
            y: padded_area.y + padded_area.height - status_height,
            width: padded_area.width,
            height: status_height,
        };

        let cmd_rect = Rect {
            x: padded_area.x,
            y: status_rect.y.saturating_sub(cmd_height),
            width: padded_area.width,
            height: cmd_height,
        };

        let exec_rect_calc = Rect {
            x: padded_area.x,
            y: header_rect.y + header_rect.height,
            width: padded_area.width,
            height: cmd_rect
                .y
                .saturating_sub(header_rect.y + header_rect.height),
        };

        render_header(f, header_rect, state);
        render_execution(f, exec_rect_calc, state);
        render_input_box(f, cmd_rect, state);
        render_status_bar(f, status_rect, state);

        exec_rect = exec_rect_calc;
        input_rect = cmd_rect;
    })?;

    Ok((input_rect, Rect::default(), exec_rect))
}

fn render_header(f: &mut Frame, area: Rect, state: &AgentState) {
    let mut lines: Vec<Line> = LOGO
        .iter()
        .map(|l| Line::from(Span::styled(*l, Style::default().fg(FG_MAIN))))
        .collect();

    let branch = git_branch(&state.repo_root).unwrap_or_else(|| "detached".into());
    let version = env!("CARGO_PKG_VERSION");

    lines.push(Line::from(vec![
        Span::styled(branch, Style::default().fg(FG_DIM)),
        Span::styled(" · ", Style::default().fg(FG_MUTED)),
        Span::styled(state.repo_root.to_string_lossy(), Style::default().fg(FG_DIM)),
        Span::styled(" · ", Style::default().fg(FG_DIM)),
        Span::styled(version, Style::default().fg(FG_MUTED)),
    ]));

    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_execution(f: &mut Frame, area: Rect, state: &AgentState) {
    let padded = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let height = padded.height.max(1) as usize;
    let mut lines: Vec<Line> = Vec::new();

    let mut last_was_user = false;
    let mut md = crate::ui::markdown::Markdown::new();

    for log in state.logs.iter() {
        let text = log.text.as_str();

        if let Some(input) = parse_user_input_log(text) {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(render_static_command_line(input));
            last_was_user = true;
            continue;
        }

        if text.starts_with("● ") {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(text, Style::default().fg(FG_MAIN))));
            last_was_user = false;
            continue;
        }

        if text.starts_with("└ ") {
            lines.push(Line::from(Span::styled(text, Style::default().fg(FG_DIM))));
            continue;
        }

        if last_was_user {
            lines.push(Line::from(""));
        }

        lines.push(md.render_line(text));
        last_was_user = false;
    }

    if state.ui.diff_active && !state.ui.diff_snapshot.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Changes", Style::default().fg(FG_MAIN))));
        for diff in &state.ui.diff_snapshot {
            lines.extend(crate::ui::diff::render_diff(diff, padded.width));
        }
    }

    let max_scroll = lines.len().saturating_sub(height);
    let scroll = if state.ui.follow_tail {
        max_scroll
    } else {
        state.ui.exec_scroll.min(max_scroll)
    };

    f.render_widget(
        Paragraph::new(lines)
            .scroll((scroll as u16, 0))
            .wrap(Wrap { trim: false }),
        padded,
    );
}

fn render_input_box(f: &mut Frame, area: Rect, state: &AgentState) {
    if area.height < 3 {
        return;
    }

    let inner_width = area.width as usize;
    let text_width = inner_width.saturating_sub(PROMPT.len()).max(1);

    let raw = if matches!(state.ui.input_mode, InputMode::ApiKey) {
        "•".repeat(state.ui.input.len())
    } else {
        state.ui.input.clone()
    };

    // Split into visual lines (handle wrapping)
    let mut visual = Vec::new();
    if raw.is_empty() {
        visual.push(String::new());
    } else {
        for line in raw.lines() {
            if line.is_empty() {
                visual.push(String::new());
            } else {
                let mut start = 0;
                while start < line.len() {
                    let end = (start + text_width).min(line.len());
                    visual.push(line[start..end].to_string());
                    start = end;
                }
            }
        }
    }

    let total_lines = visual.len();
    let max_visible = (area.height - 2) as usize; // -2 for borders
    let visible_lines = total_lines.min(max_visible);
    let hidden_lines = total_lines.saturating_sub(max_visible);

    let mut out = Vec::new();

    // Top border
    out.push(Line::from("─".repeat(inner_width)));

    // Show visible lines
    for (i, line) in visual.iter().take(visible_lines).enumerate() {
        if i == 0 {
            // First line with prompt
            let mut spans = vec![
                Span::styled(PROMPT, Style::default().fg(Color::White)),
                Span::styled(line, Style::default().fg(Color::White)),
            ];
            
            // Add badge on first line if there are hidden lines
            if hidden_lines > 0 {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("[pasted {} lines]", total_lines),
                    Style::default().fg(Color::Yellow),
                ));
            }
            
            out.push(Line::from(spans));
        } else {
            // Continuation lines (no prompt)
            out.push(Line::from(Span::styled(
                line,
                Style::default().fg(Color::White),
            )));
        }
    }

    // Bottom border
    out.push(Line::from("─".repeat(inner_width)));

    f.render_widget(Paragraph::new(out).wrap(Wrap { trim: false }), area);

    // Cursor position: at end of first visible line (after prompt)
    let first_line_len = visual.get(0).map(|s| s.len()).unwrap_or(0);
    let cursor_x = area.x + PROMPT.len() as u16 + first_line_len as u16;
    let cursor_y = area.y + 1;

    f.set_cursor(cursor_x, cursor_y);
}

fn render_status_bar(f: &mut Frame, area: Rect, state: &AgentState) {
    let spinner = running_pulse(state.ui.spinner_started_at)
        .unwrap_or_else(|| "····".into());

    let left = vec![
        Span::styled(spinner, Style::default().fg(FG_MAIN)),
        Span::raw(" "),
        Span::styled("OPENAI", Style::default().fg(FG_DIM)),
    ];

    let right = vec![
        Span::styled("[esc]", Style::default().fg(FG_MAIN)),
        Span::styled(" quit  ", Style::default().fg(FG_MUTED)),
        Span::styled("[enter]", Style::default().fg(FG_MAIN)),
        Span::styled(" run", Style::default().fg(FG_MUTED)),
    ];

    let lw: usize = left.iter().map(|s| s.content.len()).sum();
    let rw: usize = right.iter().map(|s| s.content.len()).sum();

    let gap = area.width.saturating_sub((lw + rw) as u16).max(1) as usize;

    let mut spans = Vec::new();
    spans.extend(left);
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right);

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}