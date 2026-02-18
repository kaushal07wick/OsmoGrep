use std::{io};

use ratatui::{
    backend::Backend,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap, Block, Borders, Clear},
    Terminal, Frame,
    prelude::*,
};

use crate::{
    logger::parse_user_input_log,
    state::{AgentState, InputMode},
};
use crate::ui::helper::{
    render_static_command_line,
    running_pulse,
    git_branch,
    calculate_input_lines,
};
const TOP_PADDING: u16 = 1;

const FG_MAIN: Color = Color::Rgb(220, 220, 220);
const FG_DIM: Color = Color::Rgb(170, 170, 170);
const FG_MUTED: Color = Color::Rgb(120, 120, 120);
const PROMPT: &str = ">_ ";
const SPINNER_WIDTH: usize = 4;

const LOGO: [&str; 7] = [
    " ██████  ███████ ███    ███  ██████   ██████  ██████  ███████ ██████  ",
    "██    ██ ██      ████  ████ ██    ██ ██       ██   ██ ██      ██   ██ ",
    "██    ██ ███████ ██ ████ ██ ██    ██ ██   ███ ██████  █████   ██████  ",
    "██    ██      ██ ██  ██  ██ ██    ██ ██    ██ ██   ██ ██      ██      ",
    " ██████  ███████ ██      ██  ██████   ██████  ██   ██ ███████ ██      ",
    "                                                                      ",
    "                                                                      ",
];


/// master ui
pub fn draw_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<(Rect, Rect, Rect)> {
    let mut input_rect = Rect::default();
    let mut exec_rect = Rect::default();
    let mut hint_rect = Rect::default();
    let mut voice_rect = Rect::default();

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

        let show_voice = state.voice.visible || state.voice.enabled || state.voice.connected;
        if show_voice {
            let voice_text = voice_bar_text(state);
            let voice_lines =
                calculate_input_lines(&voice_text, padded_area.width as usize, 0);
            let available =
                cmd_rect.y.saturating_sub(header_rect.y + header_rect.height);
            let max_voice = available.max(1) as usize;
            let voice_height = voice_lines.clamp(1, max_voice) as u16;
            voice_rect = Rect {
                x: padded_area.x,
                y: cmd_rect.y.saturating_sub(voice_height),
                width: padded_area.width,
                height: voice_height,
            };
        } else {
            voice_rect = Rect {
                x: padded_area.x,
                y: cmd_rect.y,
                width: padded_area.width,
                height: 0,
            };
        }

        let exec_rect_calc = Rect {
            x: padded_area.x,
            y: header_rect.y + header_rect.height,
            width: padded_area.width,
            height: voice_rect
                .y
                .saturating_sub(header_rect.y + header_rect.height),
        };

        let palette_active = !state.ui.command_items.is_empty();
        let palette_height = if palette_active {
            (state.ui.command_items.len().min(10) as u16) + 2
        } else {
            0
        };

        if palette_height > 0 {
            let palette_width = padded_area.width.saturating_sub(6).min(72).max(44);

            let palette_x =
                cmd_rect.x + PROMPT.len() as u16 + 2; // align with text start

            let palette_y =
                cmd_rect.y.saturating_sub(palette_height + 1);

            hint_rect = Rect {
                x: palette_x,
                y: palette_y,
                width: palette_width,
                height: palette_height,
            };
        }

        render_header(f, header_rect, state);
        render_execution(f, exec_rect_calc, state);
        render_voice_bar(f, voice_rect, state);
        render_input_box(f, cmd_rect, state);
        render_status_bar(f, status_rect, state);

        if palette_height > 0 {
            render_command_palette(f, hint_rect, state);
        }

        exec_rect = exec_rect_calc;
        input_rect = cmd_rect;
    })?;

    Ok((input_rect, hint_rect, exec_rect))
}

/// header with info
fn render_header(f: &mut Frame, area: Rect, state: &AgentState) {
    let mut lines: Vec<Line> = LOGO
        .iter()
        .map(|l| Line::from(Span::styled(*l, Style::default().fg(FG_MAIN))))
        .collect();

    let branch = git_branch(&state.repo_root).unwrap_or_else(|| "detached".into());
    let version = env!("CARGO_PKG_VERSION");
    let repo_display = state
        .repo_root
        .strip_prefix(dirs::home_dir().unwrap_or_default())
        .map(|p| format!("~{}", std::path::MAIN_SEPARATOR.to_string() + &p.display().to_string()))
        .unwrap_or_else(|_| state.repo_root.display().to_string());

    let mut meta_spans = vec![
        Span::styled(repo_display, Style::default().fg(FG_DIM)),
        Span::styled(" · ", Style::default().fg(FG_MUTED)),
        Span::styled(branch, Style::default().fg(FG_DIM)),
        Span::styled(" · ", Style::default().fg(FG_DIM)),
        Span::styled(version, Style::default().fg(FG_MUTED)),
    ];

    if state.ui.indexing {
        meta_spans.push(Span::styled(
            " · indexing…",
            Style::default()
                .fg(FG_MUTED)
                .add_modifier(Modifier::ITALIC),
        ));
    } else if state.ui.indexed {
        meta_spans.push(Span::styled(
            " · indexed ✓",
            Style::default().fg(FG_DIM),
        ));
    }

    lines.push(Line::from(meta_spans));
    lines.push(Line::from(" "));

    lines.push(Line::from(Span::styled(
        "Type a task to make osmogrep work for you or /help (for commands) · !<cmd> runs linux shell · ",
        Style::default()
            .fg(FG_MUTED)
            .add_modifier(Modifier::ITALIC),
    )));

    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// main panel
fn render_execution(f: &mut Frame, area: Rect, state: &AgentState) {
    let padded = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let height = padded.height.max(1) as usize;
    let mut lines: Vec<Line> = Vec::new();

    let mut md = crate::ui::markdown::Markdown::new();

    for log in state.logs.iter() {
        let text = log.text.as_str();
        if let Some(input) = parse_user_input_log(text) {
            lines.extend(render_static_command_line(
                input,
                padded.width as usize,
            ));
            continue;
        }

        if text.starts_with("● ") {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(text, Style::default().fg(FG_MAIN))));
            continue;
        }

        if text.starts_with("└ ") {
            lines.push(Line::from(Span::styled(text, Style::default().fg(FG_DIM))));
            continue;
        }

        if text.starts_with("· ") {
            lines.push(Line::from(Span::styled(
                text,
                Style::default()
                    .fg(FG_MUTED)
                    .add_modifier(Modifier::ITALIC),
            )));
            continue;
        }

        lines.push(md.render_line(text));
    }

    if state.ui.diff_active && !state.ui.diff_snapshot.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Changes",
            Style::default().fg(FG_MAIN).add_modifier(Modifier::BOLD),
        )));

        for snap in &state.ui.diff_snapshot {
            let diff = crate::ui::diff::Diff::from_texts(
                snap.target.clone(),
                &snap.before,
                &snap.after,
            );

            lines.extend(crate::ui::diff::render_diff(&diff, padded.width));
        }
    }

    if let Some(p) = &state.ui.pending_permission {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "Allow {} ({})? [y]es [n]o [a]lways",
                p.tool_name, p.args_summary
            ),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
    }

    if state.ui.streaming_active {
        let partial = state
            .ui
            .streaming_buffer
            .rsplit('\n')
            .next()
            .unwrap_or("");
        if !partial.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("{partial}█"),
                Style::default().fg(FG_DIM),
            )));
        }
    }

    let max_scroll = lines.len().saturating_sub(height);
    let scroll = if state.ui.follow_tail {
        max_scroll
    } else {
        max_scroll.saturating_sub(state.ui.exec_scroll)
    };

    f.render_widget(
        Paragraph::new(lines)
            .scroll((scroll as u16, 0))
            .wrap(Wrap { trim: false }),
        padded,
    );
}

fn voice_bar_text(state: &AgentState) -> String {
    let status = if state.voice.connected {
        "VOICE"
    } else if state.voice.enabled {
        "VOICE (connecting)"
    } else {
        "VOICE (off)"
    };

    let mut text = state
        .voice
        .last_final
        .clone()
        .or_else(|| {
            if state.voice.buffer.is_empty() {
                None
            } else {
                Some(state.voice.buffer.clone())
            }
        })
        .or_else(|| state.voice.partial.clone())
        .unwrap_or_default();

    if text.is_empty() && state.voice.connected {
        text = "listening...".into();
    }

    format!("{status} {text}")
}

fn render_voice_bar(f: &mut Frame, area: Rect, state: &AgentState) {
    if area.height == 0 {
        return;
    }

    let text = voice_bar_text(state);
    let line = Line::from(vec![
        Span::styled("voice: ", Style::default().fg(FG_DIM)),
        Span::styled(text, Style::default().fg(FG_MUTED)),
    ]);
    f.render_widget(
        Paragraph::new(line).wrap(Wrap { trim: true }),
        area,
    );
}

/// input box ui
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

/// bottom status with animation and tabs
fn render_status_bar(f: &mut Frame, area: Rect, state: &AgentState) {
    let spinner = running_pulse(state.ui.spinner_started_at)
        .unwrap_or_else(|| "....".into());

    let mut spinner_fixed = spinner;
    let len = spinner_fixed.chars().count();

    if len < SPINNER_WIDTH {
        spinner_fixed.push_str(&" ".repeat(SPINNER_WIDTH - len));
    } else if len > SPINNER_WIDTH {
        spinner_fixed = spinner_fixed.chars().take(SPINNER_WIDTH).collect();
    }

    let voice_label = if state.voice.enabled {
        if state.voice.connected {
            "VOICE ON"
        } else {
            "VOICE ..."
        }
    } else {
        "VOICE OFF"
    };

    let model_label = if state.voice.enabled {
        "VOXTRAL"
    } else {
        "OPENAI"
    };

    let run_label = if state.ui.agent_running { "RUN" } else { "IDLE" };
    let perm_label = if state.ui.pending_permission.is_some() {
        "PERM?"
    } else {
        "PERM-"
    };
    let ctx_label = format!("CTX {}", state.conversation.token_estimate);

    let mut left = vec![
        Span::styled(spinner_fixed, Style::default().fg(FG_MAIN)),
        Span::raw(" "),
        Span::styled(model_label, Style::default().fg(FG_DIM)),
        Span::raw(" "),
        Span::styled(run_label, Style::default().fg(FG_MAIN)),
        Span::raw(" "),
        Span::styled(
            perm_label,
            Style::default().fg(if state.ui.pending_permission.is_some() {
                Color::Yellow
            } else {
                FG_DIM
            }),
        ),
        Span::raw(" "),
        Span::styled(ctx_label, Style::default().fg(FG_MUTED)),
    ];
    if state.voice.visible || state.voice.enabled || state.voice.connected {
        left.insert(3, Span::raw(" "));
        left.insert(4, Span::styled(voice_label, Style::default().fg(FG_DIM)));
    }

    let esc_action = if state.ui.agent_running {
        " cancel  "
    } else {
        " quit  "
    };

    let right = vec![
        Span::styled("[esc]", Style::default().fg(FG_MAIN)),
        Span::styled(esc_action, Style::default().fg(FG_MUTED)),
        Span::styled("[enter]", Style::default().fg(FG_MAIN)),
        Span::styled(" run", Style::default().fg(FG_MUTED)),
    ];

    let rw: u16 = right.iter().map(|s| s.content.len() as u16).sum();
    let right_width = rw.min(area.width);
    let right_rect = Rect {
        x: area.x + area.width.saturating_sub(right_width),
        y: area.y,
        width: right_width,
        height: area.height,
    };
    let left_rect = Rect {
        x: area.x,
        y: area.y,
        width: area.width.saturating_sub(right_width),
        height: area.height,
    };

    f.render_widget(Paragraph::new(Line::from(left)), left_rect);
    f.render_widget(
        Paragraph::new(Line::from(right)).alignment(Alignment::Right),
        right_rect,
    );
}

/// command hints pallete
pub fn render_command_palette(
    f: &mut Frame,
    area: Rect,
    state: &AgentState,
) {
    if state.ui.command_items.is_empty() {
        return;
    }

    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" commands ")
        .border_style(Style::default().fg(Color::DarkGray));

    let inner_width = area.width.saturating_sub(2) as usize; // borders

    let mut lines = Vec::new();

    let visible_rows = area.height.saturating_sub(2) as usize;
    if visible_rows == 0 {
        return;
    }

    let len = state.ui.command_items.len();
    let start = if len <= visible_rows {
        0
    } else if state.ui.command_selected >= visible_rows {
        state.ui.command_selected + 1 - visible_rows
    } else {
        0
    };
    let end = (start + visible_rows).min(len);

    for (i, item) in state.ui.command_items[start..end].iter().enumerate() {
        let actual_idx = start + i;
        let selected = actual_idx == state.ui.command_selected;

        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        let text = format!("{:<14} {}", item.cmd, item.desc);

        let padded = format!(
            "{:<width$}",
            text,
            width = inner_width
        );

        lines.push(Line::from(Span::styled(padded, style)));
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}
