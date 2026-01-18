use std::{io, process::Command, time::Instant};

use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Terminal,
};

use crate::{
    logger::parse_user_input_log,
    state::{AgentState, InputMode},
};

const FG_MAIN: Color = Color::Rgb(220, 220, 220);
const FG_DIM: Color = Color::Rgb(170, 170, 170);
const FG_MUTED: Color = Color::Rgb(120, 120, 120);

const PROMPT: &str = "> ";

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

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(LOGO.len() as u16 + 2),
                Constraint::Min(5),     // execution
                Constraint::Length(3), // command box
                Constraint::Length(1), // status bar
            ])
            .split(area);

        render_header(f, chunks[1], state);
        render_execution(f, chunks[2], state);
        render_input_box(f, chunks[3], state);
        render_status_bar(f, chunks[4], state);

        exec_rect = chunks[2];
        input_rect = chunks[3];
    })?;

    Ok((input_rect, Rect::default(), exec_rect))
}

/* ---------------- Header ---------------- */

fn render_header(f: &mut ratatui::Frame, area: Rect, state: &AgentState) {
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

/* ---------------- Execution ---------------- */

fn render_execution(f: &mut ratatui::Frame, area: Rect, state: &AgentState) {
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
        lines.push(Line::from(Span::styled(
            "Changes",
            Style::default().fg(FG_MAIN),
        )));

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

/* ---------------- Command Box ---------------- */

fn render_input_box(f: &mut ratatui::Frame, area: Rect, state: &AgentState) {
    let inner = Rect {
        x: area.x + 2,
        y: area.y,
        width: area.width.saturating_sub(4),
        height: area.height,
    };

    let input = if matches!(state.ui.input_mode, InputMode::ApiKey) {
        "•".repeat(state.ui.input.len())
    } else {
        state.ui.input.clone()
    };

    let top = Line::from(Span::styled(
        "─".repeat(inner.width as usize),
        Style::default().fg(Color::White),
    ));

    let middle = Line::from(vec![
        Span::styled(PROMPT, Style::default().fg(Color::White)),
        Span::styled(input, Style::default().fg(Color::White)),
    ]);

    let bottom = Line::from(Span::styled(
        "─".repeat(inner.width as usize),
        Style::default().fg(Color::White),
    ));

    let lines = vec![top, middle, bottom];

    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner,
    );

    let cursor_x =
        inner.x + PROMPT.len() as u16 + state.ui.input.len() as u16;
    let cursor_y = inner.y + 1;

    f.set_cursor(cursor_x, cursor_y);
}

/* ---------------- Status Bar ---------------- */

fn render_status_bar(f: &mut ratatui::Frame, area: Rect, state: &AgentState) {
    let spinner = running_pulse(state.ui.spinner_started_at)
        .unwrap_or_else(|| "····".into());

    let left = vec![
        Span::styled(spinner, Style::default().fg(FG_MAIN)),
        Span::raw(" "),
        Span::styled("OPENAI", Style::default().fg(FG_DIM)),
    ];

    let right = vec![
        Span::styled("[esc]", Style::default().fg(FG_MAIN)),
        Span::raw(" quit  "),
        Span::styled("[enter]", Style::default().fg(FG_MAIN)),
        Span::raw(" run"),
    ];

    let left_w: usize = left.iter().map(|s| s.content.len()).sum();
    let right_w: usize = right.iter().map(|s| s.content.len()).sum();

    let space = area
        .width
        .saturating_sub((left_w + right_w) as u16)
        .max(1) as usize;

    let mut spans = Vec::new();
    spans.extend(left);
    spans.push(Span::raw(" ".repeat(space)));
    spans.extend(right);

    f.render_widget(
        Paragraph::new(Line::from(spans)),
        area,
    );
}

/* ---------------- Helpers ---------------- */

fn render_static_command_line(text: &str) -> Line {
    Line::from(vec![
        Span::styled("│ ", Style::default().fg(FG_MAIN)),
        Span::styled(text, Style::default().fg(FG_MAIN)),
    ])
}

fn running_pulse(start: Option<Instant>) -> Option<String> {
    let start = start?;
    let t = (start.elapsed().as_millis() / 120) as usize;

    let pos = match t % 6 {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 2,
        _ => 1,
    };

    let tail = if pos == 0 { 3 } else { pos - 1 };

    let mut s = String::with_capacity(4);
    for i in 0..4 {
        if i == pos {
            s.push('■');
        } else if i == tail {
            s.push('▪');
        } else {
            s.push('·');
        }
    }

    Some(s)
}

fn git_branch(repo_root: &std::path::Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;

    let s = String::from_utf8(out.stdout).ok()?;
    Some(s.trim().to_string())
}
