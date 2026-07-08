use std::io;

use ratatui::{
    backend::Backend,
    layout::{Alignment, Rect},
    prelude::*,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame, Terminal,
};

use crate::ui::helper::{
    calculate_input_lines, git_branch, render_static_command_line, running_pulse,
};
use crate::{
    logger::parse_user_input_log,
    state::{AgentState, InputMode, UiAccent, UiDensity, UiTheme},
};

const FG_MAIN: Color = Color::Rgb(220, 220, 220);
const FG_DIM: Color = Color::Rgb(170, 170, 170);
const FG_MUTED: Color = Color::Rgb(120, 120, 120);
const ACCENT_ORANGE: Color = Color::Rgb(255, 165, 80);
const PROMPT: &str = ">_ ";

#[derive(Clone, Copy)]
struct UiPalette {
    fg_main: Color,
    fg_dim: Color,
    fg_muted: Color,
    accent: Color,
    input_fg: Color,
    border: Color,
}

fn palette(state: &AgentState) -> UiPalette {
    match state.theme {
        UiTheme::Dark => UiPalette {
            fg_main: FG_MAIN,
            fg_dim: FG_DIM,
            fg_muted: FG_MUTED,
            accent: accent_color(state.accent, false),
            input_fg: Color::White,
            border: Color::Rgb(70, 70, 70),
        },
        UiTheme::Light => UiPalette {
            fg_main: Color::Rgb(35, 35, 35),
            fg_dim: Color::Rgb(75, 75, 75),
            fg_muted: Color::Rgb(120, 120, 120),
            accent: accent_color(state.accent, true),
            input_fg: Color::Black,
            border: Color::Rgb(195, 195, 195),
        },
    }
}

fn accent_color(accent: UiAccent, light: bool) -> Color {
    match (accent, light) {
        (UiAccent::Orange, false) => ACCENT_ORANGE,
        (UiAccent::Blue, false) => Color::Rgb(95, 175, 255),
        (UiAccent::Green, false) => Color::Rgb(70, 190, 120),
        (UiAccent::Violet, false) => Color::Rgb(185, 140, 255),
        (UiAccent::Rose, false) => Color::Rgb(255, 110, 145),
        (UiAccent::Orange, true) => Color::Rgb(210, 105, 20),
        (UiAccent::Blue, true) => Color::Rgb(20, 105, 210),
        (UiAccent::Green, true) => Color::Rgb(20, 135, 80),
        (UiAccent::Violet, true) => Color::Rgb(120, 85, 210),
        (UiAccent::Rose, true) => Color::Rgb(190, 55, 90),
    }
}

fn top_padding(state: &AgentState) -> u16 {
    match state.density {
        UiDensity::Compact => 0,
        UiDensity::Standard => 1,
        UiDensity::Spacious => 2,
    }
}

fn header_height(state: &AgentState, available_height: u16) -> u16 {
    match state.density {
        UiDensity::Compact => 2,
        UiDensity::Standard if available_height < 24 => 2,
        UiDensity::Standard => 3,
        UiDensity::Spacious if available_height < 24 => 3,
        UiDensity::Spacious => 4,
    }
}

/// master ui
pub fn draw_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<(Rect, Rect, Rect)> {
    let mut input_rect = Rect::default();
    let mut exec_rect = Rect::default();
    let mut hint_rect = Rect::default();
    let mut voice_rect = Rect::default();
    let mut running_rect = Rect::default();

    terminal.draw(|f| {
        let area = f.size();
        let top_padding = top_padding(state);

        let padded_area = Rect {
            x: area.x,
            y: area.y + top_padding,
            width: area.width,
            height: area.height.saturating_sub(top_padding),
        };

        let header_height = header_height(state, padded_area.height);
        let status_height = 1;

        let input_width = padded_area.width.saturating_sub(4) as usize;
        let input_lines = calculate_input_lines(&state.ui.input, input_width, PROMPT.len());

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
            let voice_lines = calculate_input_lines(&voice_text, padded_area.width as usize, 0);
            let available = cmd_rect
                .y
                .saturating_sub(header_rect.y + header_rect.height);
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

        let mut exec_rect_calc = Rect {
            x: padded_area.x,
            y: header_rect.y + header_rect.height,
            width: padded_area.width,
            height: voice_rect
                .y
                .saturating_sub(header_rect.y + header_rect.height),
        };
        if state.ui.agent_running && exec_rect_calc.height > 1 {
            running_rect = Rect {
                x: exec_rect_calc.x,
                y: exec_rect_calc.y + exec_rect_calc.height - 1,
                width: exec_rect_calc.width,
                height: 1,
            };
            exec_rect_calc.height = exec_rect_calc.height.saturating_sub(1);
        } else {
            running_rect = Rect {
                x: exec_rect_calc.x,
                y: exec_rect_calc.y + exec_rect_calc.height,
                width: exec_rect_calc.width,
                height: 0,
            };
        }

        let palette_active = !state.ui.command_items.is_empty();
        let palette_height = if palette_active {
            (state.ui.command_items.len().min(10) as u16) + 2
        } else {
            0
        };

        if palette_height > 0 {
            let palette_width = padded_area.width.saturating_sub(6).min(72).max(44);

            let palette_x = cmd_rect.x + PROMPT.len() as u16 + 2; // align with text start

            let palette_y = cmd_rect.y.saturating_sub(palette_height + 1);

            hint_rect = Rect {
                x: palette_x,
                y: palette_y,
                width: palette_width,
                height: palette_height,
            };
        }

        render_header(f, header_rect, state);
        render_execution(f, exec_rect_calc, state);
        render_running_badge(f, running_rect, state);
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
    let p = palette(state);
    let branch = git_branch(&state.repo_root).unwrap_or_else(|| "detached".into());
    let version = env!("CARGO_PKG_VERSION");
    let repo_display = state
        .repo_root
        .strip_prefix(dirs::home_dir().unwrap_or_default())
        .map(|p| {
            format!(
                "~{}",
                std::path::MAIN_SEPARATOR.to_string() + &p.display().to_string()
            )
        })
        .unwrap_or_else(|_| state.repo_root.display().to_string());

    let budget = if state.ui.run_iteration_limit > 0 {
        format!(
            "{}/{}",
            state.ui.run_iteration, state.ui.run_iteration_limit
        )
    } else {
        "-".to_string()
    };
    let run_label = if state.ui.agent_running {
        format!("{} {budget}", state.ui.run_phase)
    } else {
        "idle".to_string()
    };
    let approval = if state.plan_mode {
        "plan-mode"
    } else if state.ui.auto_approve {
        "auto-approve"
    } else {
        "ask"
    };
    let tool = state
        .ui
        .current_tool
        .as_deref()
        .map(|t| format!(" · tool {}", truncate_for_badge(t, 28)))
        .unwrap_or_default();
    let session = state
        .session_name
        .as_deref()
        .map(|name| format!(" · {}", truncate_for_badge(name, 24)))
        .unwrap_or_default();

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                "osmogrep",
                Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(session, Style::default().fg(p.fg_dim)),
            Span::styled(" · ", Style::default().fg(p.fg_muted)),
            Span::styled(run_label, Style::default().fg(p.fg_main)),
            Span::styled(tool, Style::default().fg(p.fg_dim)),
            Span::styled(" · ", Style::default().fg(p.fg_muted)),
            Span::styled(
                state.permission_profile.as_str(),
                Style::default().fg(p.fg_dim),
            ),
            Span::styled(" · ", Style::default().fg(p.fg_muted)),
            Span::styled(approval, Style::default().fg(p.fg_dim)),
            Span::styled(" · v", Style::default().fg(p.fg_muted)),
            Span::styled(version, Style::default().fg(p.fg_muted)),
        ]),
        Line::from(vec![
            Span::styled(repo_display, Style::default().fg(p.fg_dim)),
            Span::styled(" · ", Style::default().fg(p.fg_muted)),
            Span::styled(branch, Style::default().fg(p.fg_dim)),
        ]),
    ];

    if state.ui.indexing {
        lines[1].spans.push(Span::styled(
            " · indexing…",
            Style::default()
                .fg(p.fg_muted)
                .add_modifier(Modifier::ITALIC),
        ));
    } else if state.ui.indexed {
        lines[1]
            .spans
            .push(Span::styled(" · indexed ✓", Style::default().fg(p.fg_dim)));
    }

    if area.height > 2 {
        lines.push(Line::from(Span::styled(
            "Type a task, /help for commands, !<cmd> for shell, Esc to cancel/quit.",
            Style::default()
                .fg(p.fg_muted)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// main panel
fn render_execution(f: &mut Frame, area: Rect, state: &AgentState) {
    if area.width < 4 || area.height < 3 {
        return;
    }
    let p = palette(state);
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
            lines.extend(render_static_command_line(input, padded.width as usize));
            continue;
        }

        if text.starts_with("● ") {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                text,
                Style::default().fg(p.fg_main),
            )));
            continue;
        }

        if text.starts_with("└ ") {
            lines.push(Line::from(Span::styled(
                text,
                Style::default().fg(p.fg_dim),
            )));
            continue;
        }

        if text.starts_with("· ") {
            lines.push(Line::from(Span::styled(
                text,
                Style::default()
                    .fg(p.fg_muted)
                    .add_modifier(Modifier::ITALIC),
            )));
            continue;
        }

        lines.push(md.render_line(text));
    }

    if state.ui.diff_active && !state.ui.diff_snapshot.is_empty() {
        let rendered_diffs: Vec<_> = state
            .ui
            .diff_snapshot
            .iter()
            .map(|snap| {
                crate::ui::diff::Diff::from_texts(snap.target.clone(), &snap.before, &snap.after)
            })
            .collect();
        let total_added: usize = rendered_diffs.iter().map(|d| d.added).sum();
        let total_removed: usize = rendered_diffs.iter().map(|d| d.removed).sum();

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                format!("Changes ({})", state.ui.diff_snapshot.len()),
                Style::default().fg(p.fg_main).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("+{total_added}"),
                Style::default().fg(Color::Rgb(70, 190, 120)),
            ),
            Span::raw(" "),
            Span::styled(
                format!("-{total_removed}"),
                Style::default().fg(Color::Rgb(220, 95, 90)),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            "Use /undo to revert latest change, /diff to revisit session changes.",
            Style::default()
                .fg(p.fg_muted)
                .add_modifier(Modifier::ITALIC),
        )));

        for (idx, diff) in rendered_diffs.iter().enumerate() {
            lines.extend(crate::ui::diff::render_diff(diff, padded.width));
            if idx + 1 < state.ui.diff_snapshot.len() {
                lines.push(Line::from(Span::styled(
                    "─".repeat(padded.width.saturating_sub(1) as usize),
                    Style::default().fg(Color::Rgb(70, 70, 70)),
                )));
            }
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
        let partial = state.ui.streaming_buffer.rsplit('\n').next().unwrap_or("");
        if !partial.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("{partial}█"),
                Style::default().fg(p.fg_dim),
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

fn render_running_badge(f: &mut Frame, area: Rect, state: &AgentState) {
    if !state.ui.agent_running || area.height == 0 || area.width < 10 {
        return;
    }
    let p = palette(state);
    let pulse = running_pulse(state.ui.spinner_started_at).unwrap_or_else(|| "…".to_string());
    let budget = if state.ui.run_iteration_limit > 0 {
        format!(
            " · {}/{}",
            state.ui.run_iteration, state.ui.run_iteration_limit
        )
    } else {
        String::new()
    };
    let phase = if state.ui.run_phase.is_empty() {
        "running"
    } else {
        state.ui.run_phase.as_str()
    };
    let tool = state
        .ui
        .current_tool
        .as_deref()
        .map(|s| format!(" · {}", truncate_for_badge(s, 32)))
        .or_else(|| {
            state
                .ui
                .run_detail
                .as_deref()
                .map(|s| format!(" · {}", truncate_for_badge(s, 42)))
        })
        .unwrap_or_default();
    let target = state
        .ui
        .active_edit_target
        .as_deref()
        .map(|s| format!(" · editing {}", truncate_for_badge(s, 44)))
        .unwrap_or_default();
    let recent = recent_edited_summary(state, 3);
    let recent = if recent.is_empty() {
        String::new()
    } else {
        format!(" · edited {}", recent)
    };
    let text = format!("  ● {phase} {pulse}{budget}{tool}{target}{recent}  (Esc to cancel)");
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        ))),
        area,
    );
}

fn truncate_for_badge(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn recent_edited_summary(state: &AgentState, max: usize) -> String {
    let mut files: Vec<String> = Vec::new();
    for snap in state.session_changes.iter().rev() {
        if !files.iter().any(|x| x == &snap.target) {
            files.push(snap.target.clone());
        }
        if files.len() >= max {
            break;
        }
    }
    files
        .into_iter()
        .map(|f| truncate_for_badge(&f, 20))
        .collect::<Vec<_>>()
        .join(", ")
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
    let p = palette(state);

    let text = voice_bar_text(state);
    let line = Line::from(vec![
        Span::styled("voice: ", Style::default().fg(p.fg_dim)),
        Span::styled(text, Style::default().fg(p.fg_muted)),
    ]);
    f.render_widget(Paragraph::new(line).wrap(Wrap { trim: true }), area);
}

/// input box ui
fn render_input_box(f: &mut Frame, area: Rect, state: &AgentState) {
    if area.height < 3 {
        return;
    }
    let p = palette(state);

    let inner_width = area.width as usize;
    let text_width = inner_width.saturating_sub(PROMPT.len()).max(1);

    let raw = if matches!(state.ui.input_mode, InputMode::ApiKey) {
        "•".repeat(state.ui.input.chars().count())
    } else {
        state.ui.input.clone()
    };

    // Split into visual lines (handle wrapping)
    let visual = wrap_visual_lines(&raw, text_width);

    let total_lines = visual.len();
    let max_visible = (area.height - 2) as usize; // -2 for borders
    let visible_lines = total_lines.min(max_visible);
    let hidden_lines = total_lines.saturating_sub(max_visible);

    let mut out = Vec::new();

    // Top border
    out.push(Line::from("─".repeat(inner_width)));

    // Show visible lines
    let mut image_idx = 1usize;
    for (i, line) in visual.iter().take(visible_lines).enumerate() {
        if i == 0 {
            // First line with prompt
            let mut spans = vec![Span::styled(PROMPT, Style::default().fg(p.input_fg))];
            spans.extend(render_image_alias_spans(
                line,
                &mut image_idx,
                p.accent,
                p.input_fg,
            ));

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
            out.push(Line::from(render_image_alias_spans(
                line,
                &mut image_idx,
                p.accent,
                p.input_fg,
            )));
        }
    }

    // Bottom border
    out.push(Line::from("─".repeat(inner_width)));

    f.render_widget(Paragraph::new(out).wrap(Wrap { trim: false }), area);

    // Cursor position: at end of first visible line (after prompt)
    let first_line_len = visual.get(0).map(|s| s.chars().count()).unwrap_or(0);
    let cursor_x = area.x + PROMPT.len() as u16 + first_line_len as u16;
    let cursor_y = area.y + 1;

    f.set_cursor(cursor_x, cursor_y);
}

fn wrap_visual_lines(raw: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut visual = Vec::new();

    if raw.is_empty() {
        visual.push(String::new());
        return visual;
    }

    for line in raw.lines() {
        if line.is_empty() {
            visual.push(String::new());
            continue;
        }

        let mut wrapped = String::new();
        let mut count = 0usize;
        for ch in line.chars() {
            if count == width {
                visual.push(wrapped);
                wrapped = String::new();
                count = 0;
            }
            wrapped.push(ch);
            count += 1;
        }
        visual.push(wrapped);
    }

    visual
}

fn render_image_alias_spans(
    line: &str,
    image_idx: &mut usize,
    accent: Color,
    input_fg: Color,
) -> Vec<Span<'static>> {
    if line.is_empty() {
        return vec![Span::raw(String::new())];
    }

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut token = String::new();

    let flush_token = |t: &mut String, spans: &mut Vec<Span<'static>>, image_idx: &mut usize| {
        if t.is_empty() {
            return;
        }
        if looks_like_image_path(t) {
            spans.push(Span::styled(
                format!("@image_{}", *image_idx),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ));
            *image_idx += 1;
        } else {
            spans.push(Span::styled(t.clone(), Style::default().fg(input_fg)));
        }
        t.clear();
    };

    for ch in line.chars() {
        if ch.is_whitespace() {
            flush_token(&mut token, &mut spans, image_idx);
            spans.push(Span::raw(ch.to_string()));
        } else {
            token.push(ch);
        }
    }
    flush_token(&mut token, &mut spans, image_idx);
    spans
}

fn looks_like_image_path(token: &str) -> bool {
    let trimmed = token
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches(|c: char| ",;:!?)[]{}".contains(c));
    if trimmed.is_empty() {
        return false;
    }
    let has_path_hint =
        trimmed.contains('/') || trimmed.starts_with("~") || trimmed.starts_with("./");
    if !has_path_hint {
        return false;
    }
    let ext = trimmed
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "heic" | "tiff" | "avif"
    )
}

/// bottom status with animation and tabs
fn render_status_bar(f: &mut Frame, area: Rect, state: &AgentState) {
    let p = palette(state);
    let voice_label = if state.voice.enabled {
        if state.voice.connected {
            "voice on"
        } else {
            "voice connecting"
        }
    } else {
        "voice off"
    };

    let input_label = if state.voice.enabled { "voice" } else { "text" };

    let run_label = if state.ui.agent_running {
        "running"
    } else {
        "idle"
    };
    let total_tokens = state.usage.prompt_tokens + state.usage.completion_tokens;
    let est_cost = ((state.usage.prompt_tokens as f64) * 0.0000025)
        + ((state.usage.completion_tokens as f64) * 0.0000100);
    let usage_label = format!(
        "tokens {} ctx {} ${:.4}",
        total_tokens, state.conversation.token_estimate, est_cost
    );

    let mut left = vec![
        Span::styled(input_label, Style::default().fg(p.fg_dim)),
        Span::styled(
            format!(" · {run_label}"),
            Style::default().fg(if state.ui.agent_running {
                p.accent
            } else {
                p.fg_main
            }),
        ),
        Span::styled(format!(" · {usage_label}"), Style::default().fg(p.fg_muted)),
        Span::styled(
            format!(" · {}", state.permission_profile.as_str()),
            Style::default().fg(p.fg_dim),
        ),
    ];
    if state.plan_mode {
        left.push(Span::styled(" · plan mode", Style::default().fg(p.accent)));
    }
    if state.ui.pending_permission.is_some() {
        left.push(Span::styled(
            " · approval needed",
            Style::default().fg(Color::Yellow),
        ));
    }
    if state.voice.visible || state.voice.enabled || state.voice.connected {
        left.push(Span::styled(
            format!(" · {voice_label}"),
            Style::default().fg(p.fg_dim),
        ));
    }

    let esc_action = if state.ui.agent_running {
        " cancel  "
    } else {
        " quit  "
    };

    let right = vec![
        Span::styled("[esc]", Style::default().fg(p.fg_main)),
        Span::styled(esc_action, Style::default().fg(p.fg_muted)),
        Span::styled("[enter]", Style::default().fg(p.fg_main)),
        Span::styled(" run", Style::default().fg(p.fg_muted)),
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
pub fn render_command_palette(f: &mut Frame, area: Rect, state: &AgentState) {
    if state.ui.command_items.is_empty() {
        return;
    }
    let p = palette(state);

    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" commands ")
        .border_style(Style::default().fg(p.border));

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
                .fg(match state.theme {
                    UiTheme::Dark => Color::Black,
                    UiTheme::Light => Color::White,
                })
                .bg(p.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.fg_dim)
        };

        let text = format!("{:<14} {}", item.cmd, item.desc);

        let padded = format!("{:<width$}", text, width = inner_width);

        lines.push(Line::from(Span::styled(padded, style)));
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::wrap_visual_lines;

    #[test]
    fn wraps_multibyte_mask_without_splitting_chars() {
        let masked = "•".repeat(26);
        let lines = wrap_visual_lines(&masked, 25);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].chars().count(), 25);
        assert_eq!(lines[1].chars().count(), 1);
    }
}
