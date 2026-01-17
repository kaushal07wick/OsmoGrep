// src/ui/tui.rs

use std::{io, time::Instant};

use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Terminal,
};

use crate::{
    logger::parse_user_input_log,
    state::{AgentState, InputMode, LogLevel},
};


const BG_MAIN: Color = Color::Rgb(22, 22, 22);
const BG_INPUT: Color = Color::Rgb(40, 40, 40);

const GREEN: Color = Color::Rgb(0, 220, 140);
const DIM: Color = Color::Rgb(140, 140, 140);

const HEADER: [&str; 6] = [
    " █████╗  ██████╗███╗   ███╗ █████╗  ██████╗ ██████╗ ███████╗██████╗ ",
    "██╔══██╗██╔════╝████╗ ████║██╔══██╗██╔════╝ ██╔══██╗██╔════╝██╔══██╗",
    "██║  ██║╚█████╗ ██╔████╔██║██║  ██║██║  ██╗ ██████╔╝█████╗  ██████╔╝",
    "██║  ██║ ╚═══██╗██║╚██╔╝██║██║  ██║██║  ╚██╗██╔══██╗██╔══╝  ██╔═══╝ ",
    "╚█████╔╝██████╔╝██║ ╚═╝ ██║╚█████╔╝╚██████╔╝██║  ██║███████╗██║     ",
    " ╚════╝ ╚═════╝ ╚═╝     ╚═╝ ╚════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝     ",
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
                Constraint::Length(6),  // HEADER
                Constraint::Min(5),     // EXECUTION
                Constraint::Length(3),  // INPUT
                Constraint::Length(1),  // HINTS + OPENAI
                Constraint::Length(1),  // breathing space
            ])
            .split(area);

        render_header(f, chunks[0]);
        render_execution(f, chunks[1], state);
        render_input(f, chunks[2], state);
        render_status(f, chunks[3], state);

        exec_rect = chunks[1];
        input_rect = chunks[2];
    })?;

    Ok((input_rect, Rect::default(), exec_rect))
}


fn render_header(f: &mut ratatui::Frame, area: Rect) {
    let lines = HEADER.iter().map(|l| {
        Line::from(Span::styled(
            *l,
            Style::default()
                .fg(GREEN)
                .add_modifier(Modifier::BOLD),
        ))
    });

    f.render_widget(
        Paragraph::new(lines.collect::<Vec<_>>())
            .alignment(ratatui::layout::Alignment::Center),
        area,
    );
}

fn render_execution(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    f.render_widget(
        Block::default().style(Style::default().bg(BG_MAIN)),
        area,
    );

    let padded = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let height = padded.height.max(1) as usize;
    let mut lines: Vec<Line> = Vec::new();

    for log in state.logs.iter() {
        // User input → render EXACTLY like command box, but without prefix
        if let Some(input) = parse_user_input_log(&log.text) {
            lines.push(render_command_box_line(input, false));
            continue;
        }

        let color = match log.level {
            LogLevel::Success => Color::Green,
            LogLevel::Warn => Color::Yellow,
            LogLevel::Error => Color::Red,
            _ => Color::Gray,
        };

        lines.push(Line::from(Span::styled(
            &log.text,
            Style::default().fg(color),
        )));
    }

    let max_scroll = lines.len().saturating_sub(height);
    let scroll = if state.ui.exec_scroll == usize::MAX {
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


fn render_input(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    f.render_widget(
        Block::default().style(Style::default().bg(BG_INPUT)),
        area,
    );

    let input_area = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: 1,
    };

    let masked = matches!(state.ui.input_mode, InputMode::ApiKey);

    let input = if masked {
        "•".repeat(state.ui.input.len())
    } else {
        state.ui.input.clone()
    };

    let line = render_command_box_line(&input, true);
    f.render_widget(Paragraph::new(line), input_area);

    let cursor_x = input_area.x + 3 + state.ui.input.len() as u16;
    f.set_cursor(cursor_x, input_area.y);
}


fn render_status(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
) {
    let left = match running_pulse(state.ui.spinner_started_at) {
        Some(p) => format!("OPENAI {}", p),
        None => "OPENAI".into(),
    };

    let right = vec![
        Span::styled("[esc]", Style::default().fg(GREEN)),
        Span::styled(" exit  ", Style::default().fg(DIM)),
        Span::styled("[enter]", Style::default().fg(GREEN)),
        Span::styled(" run  ", Style::default().fg(DIM)),
        Span::styled("[tab]", Style::default().fg(GREEN)),
        Span::styled(" autocomplete", Style::default().fg(DIM)),
    ];

    let right_width: usize = right.iter().map(|s| s.content.len()).sum();
    let spacing = area
        .width
        .saturating_sub(left.len() as u16 + right_width as u16)
        .max(1) as usize;

    let mut spans = Vec::new();
    spans.push(Span::styled(left, Style::default().fg(GREEN)));
    spans.push(Span::raw(" ".repeat(spacing)));
    spans.extend(right);

    f.render_widget(
        Paragraph::new(Line::from(spans))
            .style(Style::default().bg(BG_MAIN)),
        area,
    );
}


fn render_command_box_line(text: &str, with_prefix: bool) -> Line {
    let mut spans = Vec::new();

    spans.push(Span::raw("  ")); // left padding

    if with_prefix {
        spans.push(Span::styled(">_ ", Style::default().fg(GREEN)));
    }

    spans.push(Span::styled(
        text,
        Style::default().fg(Color::White),
    ));

    Line::from(spans)
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
