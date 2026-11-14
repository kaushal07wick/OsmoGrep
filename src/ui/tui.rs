use std::io;

use ratatui::{
    backend::Backend,
    layout::{Alignment, Rect},
    prelude::*,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame, Terminal,
};
use unicode_width::UnicodeWidthChar;

use crate::ui::helper::{calculate_input_lines, running_pulse};
use crate::{
    logger::parse_user_input_log,
    state::{
        AgentState, InputMode, LogLevel, PendingUpdate, PlanItem, UiAccent, UiDensity, UiTheme,
    },
};

const FG_MAIN: Color = Color::Rgb(220, 220, 220);
const FG_DIM: Color = Color::Rgb(170, 170, 170);
const FG_MUTED: Color = Color::Rgb(120, 120, 120);
const ACCENT_ORANGE: Color = Color::Rgb(255, 165, 80);
const PROMPT: &str = ">_ ";
const CONTENT_WIDTH: u16 = 112;

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

fn centered_content(area: Rect) -> Rect {
    let gutter = if area.width >= 60 { 2 } else { 1 };
    let width = area.width.saturating_sub(gutter * 2).min(CONTENT_WIDTH);
    Rect {
        x: area.x + (area.width - width) / 2,
        width,
        ..area
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

/// All panels share one readable column, including cursor and popup coordinates.
pub fn draw_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<(Rect, Rect, Rect)> {
    let mut input_rect = Rect::default();
    let mut exec_rect = Rect::default();
    let mut hint_rect = Rect::default();

    terminal.draw(|f| {
        let area = f.size();
        let mut content = centered_content(area);
        let padding = top_padding(state).min(content.height / 2);
        content.y += padding;
        content.height = content.height.saturating_sub(padding * 2);
        let header_height = header_height(state, content.height);
        let input_lines =
            calculate_input_lines(&state.ui.input, content.width as usize, PROMPT.len());
        let cmd_height = (input_lines.min(3) + 2) as u16;
        let show_voice = state.voice.visible || state.voice.enabled || state.voice.connected;
        let voice_height = if show_voice {
            calculate_input_lines(&voice_bar_text(state), content.width as usize, 0).min(
                content
                    .height
                    .saturating_sub(header_height + cmd_height + 1) as usize,
            ) as u16
        } else {
            0
        };
        let lines = execution_lines(state, content.width.saturating_sub(4));
        let mut transcript_height = if lines.is_empty() {
            1
        } else {
            u16::try_from(lines.len())
                .unwrap_or(u16::MAX)
                .saturating_add(2)
        }
        .saturating_add(u16::from(state.ui.agent_running));
        if !state.ui.command_items.is_empty() {
            transcript_height =
                transcript_height.max((state.ui.command_items.len().min(10) + 3) as u16);
        }
        content.height = content
            .height
            .min(transcript_height.saturating_add(header_height + voice_height + cmd_height + 1));
        let panels = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height),
                Constraint::Min(0),
                Constraint::Length(voice_height),
                Constraint::Length(cmd_height),
                Constraint::Length(1),
            ])
            .split(content);
        let header_rect = panels[0];
        let mut execution = panels[1];
        let mut running = Rect {
            y: execution.bottom(),
            height: 0,
            ..execution
        };
        if state.ui.agent_running && execution.height > 1 {
            execution.height -= 1;
            running.y = execution.bottom();
            running.height = 1;
        }
        input_rect = panels[3];
        exec_rect = execution;

        render_header(f, header_rect, state);
        render_execution(f, execution, state, lines);
        render_running_badge(f, running, state);
        render_voice_bar(f, panels[2], state);
        render_input_box(f, input_rect, state);
        render_status_bar(f, panels[4], state);

        if !state.ui.command_items.is_empty() {
            let height = ((state.ui.command_items.len().min(10) + 2) as u16)
                .min(input_rect.y.saturating_sub(area.y + 1));
            let x = (input_rect.x + PROMPT.len() as u16).min(content.right());
            hint_rect = Rect {
                x,
                y: input_rect.y.saturating_sub(height + 1),
                width: content.right().saturating_sub(x).min(72),
                height,
            };
            render_command_palette(f, hint_rect, state);
        }
        if state.ui.pending_permission.is_none() && state.ui.pending_update.is_none() {
            if let Some(panel) = super::workbench::view(state) {
                f.render_widget(Clear, area);
                let count = area.height.saturating_sub(10) as usize;
                let start = panel
                    .selected
                    .saturating_sub(count / 2)
                    .min(panel.rows.len().saturating_sub(count));
                let mut lines = vec![
                    Line::from(format!(
                        "  ◌ {}  /  WORKSPACE",
                        panel.sections[panel.section]
                    )),
                    Line::from(""),
                    Line::from(format!(
                        "  {}{}",
                        if panel.editing { "Edit: " } else { "Search: " },
                        panel.query
                    )),
                    Line::from(""),
                ];
                for (i, row) in panel.rows.iter().enumerate().skip(start).take(count) {
                    lines.push(Line::styled(
                        format!(
                            "{} {:<26} {}",
                            if i == panel.selected { " ›" } else { "  " },
                            row.label,
                            row.value
                        ),
                        if i == panel.selected {
                            Style::default()
                                .fg(Color::Rgb(235, 155, 105))
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        },
                    ));
                }
                if let Some(row) = panel.rows.get(panel.selected) {
                    lines.push(Line::from(""));
                    lines.push(Line::from(row.description.clone()));
                }
                lines.push(Line::from(panel.notice));
                lines.push(Line::from(panel.footer));
                f.render_widget(
                    Paragraph::new(lines).style(Style::default().bg(Color::Black)),
                    area,
                );
            }
        }
    })?;

    Ok((input_rect, hint_rect, exec_rect))
}

/// header with info
fn render_header(f: &mut Frame, area: Rect, state: &AgentState) {
    let p = palette(state);
    let branch = state
        .ui
        .repo_branch
        .clone()
        .unwrap_or_else(|| "detached".into());
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
        format!(" · {} {budget}", state.ui.run_phase)
    } else {
        String::new()
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

    let status_line = Line::from(vec![
        Span::styled(
            "osmogrep",
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" v{version}"), Style::default().fg(p.fg_muted)),
        Span::styled(session, Style::default().fg(p.fg_dim)),
        Span::styled(run_label, Style::default().fg(p.fg_main)),
        Span::styled(tool, Style::default().fg(p.fg_dim)),
        Span::styled(" · ", Style::default().fg(p.fg_muted)),
        Span::styled(approval, Style::default().fg(p.fg_dim)),
    ]);

    let mut repo_line = Line::from(vec![
        Span::styled(repo_display, Style::default().fg(p.fg_dim)),
        Span::styled(" · ", Style::default().fg(p.fg_muted)),
        Span::styled(branch, Style::default().fg(p.fg_dim)),
    ]);

    if state.ui.indexing {
        repo_line.spans.push(Span::styled(
            " · indexing…",
            Style::default()
                .fg(p.fg_muted)
                .add_modifier(Modifier::ITALIC),
        ));
    } else if state.ui.indexed {
        repo_line
            .spans
            .push(Span::styled(" · indexed ✓", Style::default().fg(p.fg_dim)));
    }

    let lines = vec![
        status_line,
        repo_line,
        Line::from(Span::styled(
            "/help commands  ·  ! shell",
            Style::default().fg(p.fg_muted),
        )),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn execution_lines(state: &AgentState, width: u16) -> Vec<Line<'static>> {
    let p = palette(state);
    let mut lines: Vec<Line> = Vec::new();

    let mut md = crate::ui::markdown::Markdown::new();

    let mut logs = state.logs.iter().peekable();
    while let Some(log) = logs.next() {
        let level = log.level;
        let fresh = log.at.elapsed().as_secs() <= 2;
        let text = log.text.as_str();
        if let Some(input) = parse_user_input_log(text) {
            let shell = logs
                .peek()
                .and_then(|next| next.text.strip_prefix("SHELL : $ "))
                == Some(input);
            if shell {
                logs.next();
            }
            lines.push(Line::default());
            for (index, line) in input.split('\n').enumerate() {
                let prefix = if index > 0 {
                    "  "
                } else if shell {
                    "$ "
                } else {
                    "› "
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        prefix,
                        Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        line.to_owned(),
                        Style::default().fg(p.fg_main).add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
            lines.push(Line::default());
            continue;
        }
        if let Some(command) = text.strip_prefix("SHELL : $ ") {
            lines.push(Line::from(vec![
                Span::styled("$ ", Style::default().fg(p.accent)),
                Span::styled(command.to_owned(), Style::default().fg(p.fg_main)),
            ]));
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

        for line in text.split('\n') {
            lines.push(style_log_line(md.render_line(line), level, fresh, p));
        }
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
            lines.extend(crate::ui::diff::render_diff(diff, width));
            if idx + 1 < state.ui.diff_snapshot.len() {
                lines.push(Line::from(Span::styled(
                    "─".repeat(width.saturating_sub(1) as usize),
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

    if let Some(update) = &state.ui.pending_update {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            pending_update_prompt(update),
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

    if !state.plan_items.is_empty() {
        lines.push(Line::from(""));
        lines.extend(render_plan_lines(state, p));
    }

    wrap_lines_safely(lines, width as usize)
}

fn render_execution(f: &mut Frame, area: Rect, state: &AgentState, lines: Vec<Line<'static>>) {
    if area.width < 4 || area.height < 3 {
        return;
    }
    let padded = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };
    let height = padded.height.max(1) as usize;
    let max_scroll = lines.len().saturating_sub(height);
    let scroll = if state.ui.follow_tail {
        max_scroll
    } else {
        max_scroll.saturating_sub(state.ui.exec_scroll)
    };

    f.render_widget(
        Paragraph::new(lines).scroll((clamp_scroll_offset(scroll), 0)),
        padded,
    );
}

fn clamp_scroll_offset(scroll: usize) -> u16 {
    scroll.min(u16::MAX as usize) as u16
}

fn style_log_line(
    mut line: Line<'static>,
    level: LogLevel,
    fresh: bool,
    p: UiPalette,
) -> Line<'static> {
    let color = match level {
        LogLevel::Info => return line,
        LogLevel::Success => Color::Rgb(70, 190, 120),
        LogLevel::Warn => Color::Yellow,
        LogLevel::Error => Color::Rgb(220, 95, 90),
    };
    let emphasize = fresh || matches!(level, LogLevel::Error);

    for span in &mut line.spans {
        span.style = span.style.fg(color);
        if emphasize {
            span.style = span.style.add_modifier(Modifier::BOLD);
        }
    }

    if line.spans.is_empty() {
        line.spans
            .push(Span::styled("", Style::default().fg(p.fg_dim)));
    }

    line
}

fn render_plan_lines(state: &AgentState, p: UiPalette) -> Vec<Line<'static>> {
    render_plan_lines_for_items(&state.plan_items, p)
}

fn render_plan_lines_for_items(plan_items: &[PlanItem], p: UiPalette) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let completed = plan_items.iter().filter(|item| item.done).count();
    lines.push(Line::from(vec![
        Span::styled(
            "Plan",
            Style::default().fg(p.fg_main).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" · {}/{}", completed, plan_items.len()),
            Style::default().fg(p.fg_muted),
        ),
    ]));

    for item in plan_items.iter().take(8) {
        let (mark, style) = if item.done {
            ("[x]", Style::default().fg(Color::Rgb(70, 190, 120)))
        } else if item.active {
            (
                "[>]",
                Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
            )
        } else {
            ("[ ]", Style::default().fg(p.fg_muted))
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{mark} "), style),
            Span::styled(item.text.clone(), Style::default().fg(p.fg_dim)),
        ]));
    }

    if plan_items.len() > 8 {
        lines.push(Line::from(Span::styled(
            format!("... {} more", plan_items.len() - 8),
            Style::default().fg(p.fg_muted),
        )));
    }

    lines
}

pub(super) fn pending_update_prompt(update: &PendingUpdate) -> String {
    if update.installing {
        format!("Installing Osmogrep {}...", update.latest_version)
    } else {
        format!(
            "Update Osmogrep {} -> {}? [y]es [n]o",
            update.current_version, update.latest_version
        )
    }
}

fn update_status_label(update: &PendingUpdate) -> &'static str {
    if update.installing {
        " · updating"
    } else {
        " · update available"
    }
}

fn wrap_lines_safely(lines: Vec<Line<'_>>, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut wrapped = Vec::new();

    for line in lines {
        let mut current_spans: Vec<Span<'static>> = Vec::new();
        let mut current_text = String::new();
        let mut current_style = Style::default();
        let mut current_width = 0usize;
        let mut saw_content = false;

        for span in line.spans {
            let span_style = span.style;
            for ch in span.content.chars() {
                let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
                if current_width > 0 && current_width + ch_width > width {
                    flush_wrapped_span(&mut current_spans, &mut current_text, current_style);
                    wrapped.push(Line::from(std::mem::take(&mut current_spans)));
                    current_width = 0;
                }

                if current_text.is_empty() {
                    current_style = span_style;
                } else if current_style != span_style {
                    flush_wrapped_span(&mut current_spans, &mut current_text, current_style);
                    current_style = span_style;
                }

                current_text.push(ch);
                current_width += ch_width;
                saw_content = true;
            }
        }

        flush_wrapped_span(&mut current_spans, &mut current_text, current_style);
        if saw_content {
            wrapped.push(Line::from(current_spans));
        } else {
            wrapped.push(Line::from(""));
        }
    }

    wrapped
}

fn flush_wrapped_span(spans: &mut Vec<Span<'static>>, text: &mut String, style: Style) {
    if text.is_empty() {
        return;
    }
    spans.push(Span::styled(std::mem::take(text), style));
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

pub(super) fn voice_bar_text(state: &AgentState) -> String {
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
    f.render_widget(Paragraph::new(line), area);
}

/// input box ui
fn render_input_box(f: &mut Frame, area: Rect, state: &AgentState) {
    if area.height < 3 || area.width == 0 {
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
    let selected = state.ui.input_all_selected && !raw.is_empty();

    let cursor_byte = display_cursor_byte_index(state, &raw);
    let (cursor_line, cursor_col) = input_cursor_visual_position(&raw, cursor_byte, text_width);

    // Split into visual lines (handle wrapping)
    let mut visual = wrap_visual_lines(&raw, text_width);
    if cursor_line >= visual.len() {
        visual.resize(cursor_line + 1, String::new());
    }

    let total_lines = visual.len();
    let max_visible = (area.height - 2) as usize; // -2 for borders
    let visible_lines = total_lines.min(max_visible);
    let hidden_lines = total_lines.saturating_sub(max_visible);
    let visual_start = if visible_lines == 0 {
        0
    } else if cursor_line >= visible_lines {
        cursor_line + 1 - visible_lines
    } else {
        0
    };

    let mut out = Vec::new();

    // Top border
    out.push(Line::from(Span::styled(
        "─".repeat(inner_width),
        Style::default().fg(p.border),
    )));

    // Show visible lines
    let mut image_idx = 1usize;
    for (offset, line) in visual
        .iter()
        .skip(visual_start)
        .take(visible_lines)
        .enumerate()
    {
        let visual_index = visual_start + offset;
        if visual_index == 0 {
            // First line with prompt
            let mut spans = vec![Span::styled(PROMPT, Style::default().fg(p.accent))];
            if raw.is_empty() && !matches!(state.ui.input_mode, InputMode::ApiKey) {
                spans.push(Span::styled(
                    "Ask a question or describe a change…",
                    Style::default().fg(p.fg_muted),
                ));
            }
            let mut input_spans =
                render_image_alias_spans(line, &mut image_idx, p.accent, p.input_fg);
            style_selected_input(&mut input_spans, selected, p);
            spans.extend(input_spans);

            // Add badge on first line if there are hidden lines
            if offset == 0 && hidden_lines > 0 {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("[pasted {} lines]", total_lines),
                    Style::default().fg(Color::Yellow),
                ));
            }

            out.push(Line::from(spans));
        } else {
            // Continuation lines (no prompt)
            let mut spans = render_image_alias_spans(line, &mut image_idx, p.accent, p.input_fg);
            style_selected_input(&mut spans, selected, p);
            out.push(Line::from(spans));
        }
    }

    // Bottom border
    out.push(Line::from(Span::styled(
        "─".repeat(inner_width),
        Style::default().fg(p.border),
    )));

    let paper = match state.theme {
        UiTheme::Dark => Color::Rgb(37, 37, 33),
        UiTheme::Light => Color::Rgb(220, 214, 199),
    };
    f.render_widget(Paragraph::new(out).style(Style::default().bg(paper)), area);

    let prompt_width = if cursor_line == 0 { PROMPT.len() } else { 0 };
    let cursor_y_offset = cursor_line.saturating_sub(visual_start);
    let cursor_x = area
        .x
        .saturating_add(prompt_width as u16)
        .saturating_add(cursor_col.min(text_width) as u16)
        .min(area.x + area.width.saturating_sub(1));
    let cursor_y = area
        .y
        .saturating_add(1)
        .saturating_add(cursor_y_offset.min(visible_lines.saturating_sub(1)) as u16);

    f.set_cursor(cursor_x, cursor_y);
}

fn display_cursor_byte_index(state: &AgentState, raw: &str) -> usize {
    let input_cursor = clamp_char_boundary(&state.ui.input, state.ui.input_cursor);
    if !matches!(state.ui.input_mode, InputMode::ApiKey) {
        return input_cursor.min(raw.len());
    }

    let cursor_chars = state.ui.input[..input_cursor].chars().count();
    raw.char_indices()
        .nth(cursor_chars)
        .map(|(idx, _)| idx)
        .unwrap_or(raw.len())
}

fn input_cursor_visual_position(raw: &str, cursor: usize, width: usize) -> (usize, usize) {
    let width = width.max(1);
    let cursor = clamp_char_boundary(raw, cursor);
    let mut line = 0usize;
    let mut col = 0usize;

    for ch in raw[..cursor].chars() {
        if ch == '\n' {
            line += 1;
            col = 0;
            continue;
        }

        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if col > 0 && col + ch_width > width {
            line += 1;
            col = 0;
        }
        col += ch_width;
        if col >= width {
            line += 1;
            col = 0;
        }
    }

    (line, col)
}

fn clamp_char_boundary(value: &str, index: usize) -> usize {
    let mut index = index.min(value.len());
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn style_selected_input(spans: &mut [Span<'static>], selected: bool, p: UiPalette) {
    if !selected {
        return;
    }

    for span in spans {
        span.style = span
            .style
            .fg(Color::Black)
            .bg(p.accent)
            .add_modifier(Modifier::BOLD);
    }
}

fn wrap_visual_lines(raw: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut visual = Vec::new();

    if raw.is_empty() {
        visual.push(String::new());
        return visual;
    }

    for line in raw.split('\n') {
        if line.is_empty() {
            visual.push(String::new());
            continue;
        }

        let mut wrapped = String::new();
        let mut width_used = 0usize;
        for ch in line.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if width_used > 0 && width_used + ch_width > width {
                visual.push(wrapped);
                wrapped = String::new();
                width_used = 0;
            }
            wrapped.push(ch);
            width_used += ch_width;
            if width_used >= width {
                visual.push(wrapped);
                wrapped = String::new();
                width_used = 0;
            }
        }
        if !wrapped.is_empty() {
            visual.push(wrapped);
        }
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
    let mut left = vec![Span::styled(
        super::workbench::statusline(state).join(" · "),
        Style::default().fg(p.fg_dim),
    )];
    if state.ui.pending_permission.is_some() {
        left.push(Span::styled(
            " · approval needed",
            Style::default().fg(Color::Yellow),
        ));
    }
    if let Some(update) = &state.ui.pending_update {
        left.push(Span::styled(
            update_status_label(update),
            Style::default().fg(Color::Yellow),
        ));
    }

    let esc_action = if state.ui.agent_running {
        " cancel  "
    } else {
        " quit  "
    };

    let submit = super::keymap::BINDINGS
        .iter()
        .find(|b| b.id == "submit")
        .map(|b| super::keymap::effective(&state.preferences, b))
        .unwrap_or("enter");
    let right = if area.width < 32 {
        Vec::new()
    } else if area.width < 72 {
        vec![Span::styled(
            format!("esc / {submit}"),
            Style::default().fg(p.fg_dim),
        )]
    } else {
        vec![
            Span::styled("[esc]", Style::default().fg(p.fg_main)),
            Span::styled(esc_action, Style::default().fg(p.fg_muted)),
            Span::styled(format!("[{submit}]"), Style::default().fg(p.fg_main)),
            Span::styled(" run", Style::default().fg(p.fg_muted)),
        ]
    };

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
        width: area.width.saturating_sub(right_width + 2),
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

    let paragraph = Paragraph::new(lines).block(block);

    f.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::{
        centered_content, clamp_scroll_offset, draw_ui, header_height,
        input_cursor_visual_position, pending_update_prompt, render_plan_lines_for_items,
        update_status_label, wrap_lines_safely, wrap_visual_lines, UiPalette, CONTENT_WIDTH,
    };
    use crate::state::{PendingUpdate, PlanItem, UiDensity};
    use ratatui::{
        backend::{Backend, TestBackend},
        layout::Rect,
        Terminal,
    };
    use ratatui::{style::Color, text::Line};

    #[test]
    fn wraps_multibyte_mask_without_splitting_chars() {
        let masked = "•".repeat(26);
        let lines = wrap_visual_lines(&masked, 25);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].chars().count(), 25);
        assert_eq!(lines[1].chars().count(), 1);
    }

    #[test]
    fn safely_wraps_multibyte_execution_lines() {
        let masked = "•".repeat(26);
        let lines = wrap_lines_safely(vec![Line::from(masked)], 25);
        let snapshot = lines.iter().map(plain_text).collect::<Vec<_>>();

        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].chars().count(), 25);
        assert_eq!(snapshot[1].chars().count(), 1);
    }

    #[test]
    fn wraps_input_by_terminal_display_width() {
        let lines = wrap_visual_lines("界界界", 4);

        assert_eq!(lines, vec!["界界".to_string(), "界".to_string()]);
    }

    #[test]
    fn input_cursor_position_tracks_wrapped_display_width() {
        assert_eq!(input_cursor_visual_position("abcd", 3, 2), (1, 1));
        assert_eq!(
            input_cursor_visual_position("界界x", "界界".len(), 4),
            (1, 0)
        );
    }

    #[test]
    fn input_wrapping_preserves_trailing_empty_line() {
        let lines = wrap_visual_lines("first\n", 10);

        assert_eq!(lines, vec!["first".to_string(), String::new()]);
    }

    #[test]
    fn clamps_large_scroll_offsets_for_ratatui() {
        assert_eq!(clamp_scroll_offset(12), 12);
        assert_eq!(clamp_scroll_offset(usize::MAX), u16::MAX);
    }

    #[test]
    fn layout_centers_a_readable_column_and_fills_narrow_terminals() {
        assert_eq!(
            centered_content(Rect::new(0, 0, 180, 50)),
            Rect::new(34, 0, CONTENT_WIDTH, 50)
        );
        assert_eq!(
            centered_content(Rect::new(0, 0, 40, 20)),
            Rect::new(1, 0, 38, 20)
        );
    }

    #[test]
    fn session_grows_from_top_and_scrolls_when_transcript_fills_terminal() {
        let mut state = crate::init_state();
        let mut terminal = Terminal::new(TestBackend::new(180, 50)).unwrap();
        let (idle, _, _) = draw_ui(&mut terminal, &state).unwrap();
        save_preview(&terminal, "session");
        assert!(idle.y < 10);
        assert_eq!(idle.x, 34);
        let buffer = terminal.backend().buffer();
        let header: String = (0..180).map(|x| buffer.get(x, 1).symbol()).collect();
        assert!(header.trim_start().starts_with("osmogrep v"));
        let screen: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(!screen.contains("What are we working on?"));
        crate::logger::log_user_input(&mut state, "Explain how this repository is organized.");
        state
            .logs
            .push(crate::state::LogLevel::Info, "● read_file src/main.rs");
        state.logs.push(
            crate::state::LogLevel::Info,
            "The Rust agent lives in `src/`. Terminal rendering is in `src/ui/` and repository intelligence is in `src/intelligence/`.\n\nThe existing agent and the optional intelligence layer share tool execution.",
        );
        let (active, _, execution) = draw_ui(&mut terminal, &state).unwrap();
        save_preview(&terminal, "conversation");
        assert!(active.y > idle.y);
        assert!(active.y < 20);
        assert_eq!(active.x, execution.x);
        assert_eq!(active.width, CONTENT_WIDTH);
        assert_eq!(execution.width, active.width);
        for _ in 0..100 {
            state
                .logs
                .push(crate::state::LogLevel::Info, "More output.");
        }
        let (full, _, _) = draw_ui(&mut terminal, &state).unwrap();
        assert!(full.y > 40);
        assert!(full.bottom() <= 50);
        state.density = UiDensity::Compact;
        assert_eq!(header_height(&state, 50), 2);
    }

    #[test]
    fn shell_history_has_one_themed_command_and_no_light_cards() {
        let mut state = crate::init_state();
        crate::logger::log_user_input(&mut state, "ls -al");
        state
            .logs
            .push(crate::state::LogLevel::Info, "SHELL : $ ls -al");
        state
            .logs
            .push(crate::state::LogLevel::Info, "src  Cargo.toml");
        let mut terminal = Terminal::new(TestBackend::new(160, 40)).unwrap();
        draw_ui(&mut terminal, &state).unwrap();
        let buffer = terminal.backend().buffer();
        save_preview(&terminal, "shell");
        let rows: Vec<String> = (0..40)
            .map(|y| (0..160).map(|x| buffer.get(x, y).symbol()).collect())
            .collect();
        assert_eq!(
            rows.iter().filter(|line| line.contains("ls -al")).count(),
            1
        );
        assert!(!rows.iter().any(|line| line.contains("SHELL :")));
        assert!(rows.iter().any(|line| line.contains("$ ls -al")));
        assert!(buffer
            .content
            .iter()
            .all(|cell| cell.bg != Color::Rgb(220, 220, 220)));
    }

    #[test]
    fn transcript_preserves_paragraphs_and_multiline_code() {
        let mut state = crate::init_state();
        state.logs.push(
            crate::state::LogLevel::Info,
            "First paragraph.\n\nSecond paragraph.\n```rust\nfn main() {}\n```",
        );
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        draw_ui(&mut terminal, &state).unwrap();
        let buffer = terminal.backend().buffer();
        let rows: Vec<String> = (0..24)
            .map(|y| {
                (0..80)
                    .map(|x| buffer.get(x, y).symbol())
                    .collect::<String>()
                    .trim()
                    .to_owned()
            })
            .collect();
        let first = rows
            .iter()
            .position(|row| row == "First paragraph.")
            .unwrap();
        assert_eq!(rows[first + 1], "");
        assert_eq!(rows[first + 2], "Second paragraph.");
        assert_eq!(rows[first + 3], "```rust");
        assert_eq!(rows[first + 4], "fn main() {}");
        assert_eq!(rows[first + 5], "```");
    }

    #[test]
    fn rendering_and_popups_stay_inside_small_and_wide_terminals() {
        for (width, height) in [(1, 1), (20, 5), (40, 12), (80, 24), (160, 50), (240, 80)] {
            let mut state = crate::init_state();
            state.ui.input = "/h".into();
            state.ui.input_cursor = 2;
            state.ui.command_items = vec![
                crate::state::CommandItem {
                    cmd: "/help",
                    desc: "Show commands"
                };
                10
            ];
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let (input, hint, output) = draw_ui(&mut terminal, &state).unwrap();
            if width == 80 {
                save_preview(&terminal, "narrow-palette");
            }
            if width >= 80 && height >= 24 {
                assert!(hint.y >= 3, "menu must leave the session header visible");
            }
            for rect in [input, hint, output] {
                assert!(rect.right() <= width, "{width}x{height}: {rect:?}");
                assert!(rect.bottom() <= height, "{width}x{height}: {rect:?}");
            }
            if input.height >= 3 && input.width > 0 {
                let (x, y) = terminal.backend_mut().get_cursor().unwrap();
                assert!(x >= input.x && x < input.right());
                assert!(y >= input.y && y < input.bottom());
            }
        }
    }

    #[test]
    fn renders_update_prompt_snapshot() {
        let available = pending_update(false);
        let installing = pending_update(true);

        assert_eq!(
            pending_update_prompt(&available),
            "Update Osmogrep 0.3.2 -> 0.3.3? [y]es [n]o"
        );
        assert_eq!(update_status_label(&available), " · update available");
        assert_eq!(
            pending_update_prompt(&installing),
            "Installing Osmogrep 0.3.3..."
        );
        assert_eq!(update_status_label(&installing), " · updating");
    }

    #[test]
    fn renders_plan_progress_snapshot() {
        let lines = render_plan_lines_for_items(
            &[
                PlanItem {
                    text: "inspect".to_string(),
                    done: true,
                    active: false,
                },
                PlanItem {
                    text: "implement".to_string(),
                    done: false,
                    active: true,
                },
                PlanItem {
                    text: "verify".to_string(),
                    done: false,
                    active: false,
                },
            ],
            test_palette(),
        );
        let snapshot = lines.iter().map(plain_text).collect::<Vec<_>>();

        assert_eq!(
            snapshot,
            vec!["Plan · 1/3", "[x] inspect", "[>] implement", "[ ] verify"]
        );
    }

    fn pending_update(installing: bool) -> PendingUpdate {
        PendingUpdate {
            current_version: "0.3.2".to_string(),
            latest_version: "0.3.3".to_string(),
            asset_url: "https://example.com/osmogrep.tar.gz".to_string(),
            asset_name: "osmogrep.tar.gz".to_string(),
            checksum_url: "https://example.com/osmogrep.tar.gz.sha256".to_string(),
            installing,
        }
    }

    fn test_palette() -> UiPalette {
        UiPalette {
            fg_main: Color::White,
            fg_dim: Color::Gray,
            fg_muted: Color::DarkGray,
            accent: Color::Yellow,
            input_fg: Color::White,
            border: Color::Gray,
        }
    }

    fn save_preview(terminal: &Terminal<TestBackend>, name: &str) {
        let Ok(directory) = std::env::var("OSMOGREP_TUI_PREVIEW_DIR") else {
            return;
        };
        let buffer = terminal.backend().buffer();
        let cells = buffer.content.iter().map(|cell| serde_json::json!({
            "text":cell.symbol(), "fg":format!("{:?}",cell.fg), "bg":format!("{:?}",cell.bg),
        })).collect::<Vec<_>>();
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(std::path::Path::new(&directory).join(format!("{name}.json")),
            serde_json::to_vec(&serde_json::json!({"width":buffer.area.width,"height":buffer.area.height,"cells":cells})).unwrap()).unwrap();
    }

    fn plain_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }
}
