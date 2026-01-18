use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum MdState {
    Normal,
    CodeBlock,
}

pub struct Markdown {
    state: MdState,
}

impl Markdown {
    pub fn new() -> Self {
        Self { state: MdState::Normal }
    }

    pub fn render_line(&mut self, input: &str) -> Line<'static> {
        let line = input.trim_end();

        if line.starts_with("```") {
            self.state = match self.state {
                MdState::Normal => MdState::CodeBlock,
                MdState::CodeBlock => MdState::Normal,
            };

            return Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Color::Rgb(120, 120, 120)),
            ));
        }

        if self.state == MdState::CodeBlock {
            return Line::from(Span::styled(
                format!("  {}", line),
                Style::default()
                    .fg(Color::Rgb(200, 200, 200))
                    .bg(Color::Rgb(35, 35, 35)),
            ));
        }

        if line == "---" {
            return Line::from(Span::styled(
                "────────────────────────────────".to_string(),
                Style::default().fg(Color::Rgb(90, 90, 90)),
            ));
        }

        if let Some(rest) = line.strip_prefix("### ") {
            return heading(rest);
        }
        if let Some(rest) = line.strip_prefix("## ") {
            return heading(rest);
        }
        if let Some(rest) = line.strip_prefix("# ") {
            return heading(rest);
        }

        render_inline_owned(line)
    }
}

/* ---------------- Inline ---------------- */

fn render_inline_owned(text: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            chars.next();
            let content = take_until(&mut chars, "**");
            spans.push(Span::styled(
                content,
                Style::default()
                    .fg(Color::Rgb(220, 220, 220))
                    .add_modifier(Modifier::BOLD),
            ));
            continue;
        }

        if c == '*' {
            let content = take_until(&mut chars, "*");
            spans.push(Span::styled(
                content,
                Style::default()
                    .fg(Color::Rgb(200, 200, 200))
                    .add_modifier(Modifier::ITALIC),
            ));
            continue;
        }

        if c == '`' {
            let content = take_until(&mut chars, "`");
            spans.push(Span::styled(
                content,
                Style::default()
                    .fg(Color::Rgb(210, 210, 210))
                    .bg(Color::Rgb(50, 50, 50)),
            ));
            continue;
        }

        spans.push(Span::styled(
            c.to_string(),
            Style::default().fg(Color::Rgb(200, 200, 200)),
        ));
    }

    Line::from(spans)
}

/* ---------------- Helpers ---------------- */

fn heading(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default()
            .fg(Color::Rgb(220, 220, 220))
            .add_modifier(Modifier::BOLD),
    ))
}

fn take_until<I>(it: &mut std::iter::Peekable<I>, end: &str) -> String
where
    I: Iterator<Item = char>,
{
    let mut buf = String::new();

    while let Some(c) = it.next() {
        buf.push(c);
        if buf.ends_with(end) {
            buf.truncate(buf.len() - end.len());
            break;
        }
    }

    buf
}
