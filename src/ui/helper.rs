use std::{process::Command, time::Instant};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

const FG_MAIN: Color = Color::Rgb(220, 220, 220);
pub fn render_static_command_line(
    text: &str,
    term_width: usize,
) -> Vec<Line<'static>> {
    // visual tuning knobs
    let max_fraction = 0.55;      // never exceed 55% of terminal width
    let side_padding = 2;         // spaces on left + right inside the box
    let outer_margin = 2;         // visual breathing room

    let max_width = (term_width as f32 * max_fraction) as usize;
    let desired_width = text.len() + side_padding * 2 + outer_margin * 2;

    let box_width = desired_width
        .min(max_width)
        .min(term_width)
        .max(10); // sanity floor

    let inner_width = box_width
        .saturating_sub(side_padding * 2 + outer_margin * 2);

    let mut content = text.to_string();
    if content.len() > inner_width {
        content.truncate(inner_width.saturating_sub(1));
        content.push('…');
    }

    let line = format!(
        "{}{: <inner_width$}{}",
        " ".repeat(side_padding),
        content,
        " ".repeat(side_padding),
    );

    let full_line = format!(
        "{}{}{}",
        " ".repeat(outer_margin),
        line,
        " ".repeat(outer_margin),
    );

    let line_len = full_line.len();

    vec![
        Line::from(Span::styled(
            " ".repeat(line_len),
            Style::default().bg(FG_MAIN),
        )),
        Line::from(Span::styled(
            full_line,
            Style::default().fg(Color::Black).bg(FG_MAIN),
        )),
        Line::from(Span::styled(
            " ".repeat(line_len),
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