use std::{process::Command, time::Instant};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::state::{AgentState};

const FG_MAIN: Color = Color::Rgb(220, 220, 220);
const BG_CMD: Color = Color::Rgb(170, 170, 170);

pub fn render_static_command_line(text: &str) -> Vec<Line> {
    let content = format!("  {}  ", text);
    let width = content.len();

    vec![
        Line::from(Span::styled(
            " ".repeat(width),
            Style::default().bg(FG_MAIN),
        )),
        Line::from(Span::styled(
            content,
            Style::default().fg(Color::Black).bg(FG_MAIN),
        )),
        Line::from(Span::styled(
            " ".repeat(width),
            Style::default().bg(FG_MAIN),
        )),
    ]
}



pub fn running_pulse(start: Option<Instant>) -> Option<String> {
    let start = start?;
    let t = (start.elapsed().as_millis() / 90) as usize;

    let width = 5;
    let cycle = (width - 1) * 2;
    let mut pos = t % cycle;

    if pos >= width {
        pos = cycle - pos;
    }

    let mut s = String::with_capacity(width);
    for i in 0..width {
        if i == pos {
            s.push('■');
        } else if i + 1 == pos || i == pos + 1 {
            s.push('▪');
        } else {
            s.push('·');
        }
    }

    Some(s)
}


pub fn git_branch(repo_root: &std::path::Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    String::from_utf8(out.stdout).ok().map(|s| s.trim().to_string())
}

/// Calculate how many lines the input will take when rendered
pub fn calculate_input_lines(input: &str, width: usize, prompt_len: usize) -> usize {
    if input.is_empty() {
        return 1;
    }
    
    let text_width = width.saturating_sub(prompt_len).max(1);
    let mut line_count = 0;
    
    for line in input.lines() {
        if line.is_empty() {
            line_count += 1;
        } else {
            let chars = line.len();
            line_count += (chars + text_width - 1) / text_width;
        }
    }
    
    line_count.max(1)
}

pub fn slash_hints_active(state: &AgentState) -> bool {
    state.ui.input.trim_start().starts_with('/')
        && state.ui.hint.is_some()
}
