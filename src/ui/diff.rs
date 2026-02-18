use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use similar::{ChangeTag, TextDiff};

const CONTEXT_RADIUS: usize = 3;
const MAX_RENDER_LINES: usize = 500;

#[derive(Debug, Clone)]
pub struct Diff {
    pub file: String,
    pub lines: Vec<DiffRenderLine>,
    pub added: usize,
    pub removed: usize,
}

#[derive(Debug, Clone)]
pub struct DiffRenderLine {
    pub kind: DiffLineKind,
    pub old_lineno: Option<usize>,
    pub new_lineno: Option<usize>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Header,
    Context,
    Added,
    Removed,
    Omitted,
}

impl Diff {
    pub fn from_texts(file: String, before: &str, after: &str) -> Self {
        let diff = TextDiff::from_lines(before, after);
        let mut lines = Vec::new();
        let mut added = 0usize;
        let mut removed = 0usize;

        let groups = diff.grouped_ops(CONTEXT_RADIUS);
        for (group_idx, group) in groups.iter().enumerate() {
            if group_idx > 0 {
                lines.push(DiffRenderLine {
                    kind: DiffLineKind::Omitted,
                    old_lineno: None,
                    new_lineno: None,
                    text: "…".to_string(),
                });
            }

            let mut header_written = false;

            for op in group {
                if !header_written {
                    lines.push(DiffRenderLine {
                        kind: DiffLineKind::Header,
                        old_lineno: Some(op.old_range().start + 1),
                        new_lineno: Some(op.new_range().start + 1),
                        text: format!(
                            "@@ -{},{} +{},{} @@",
                            op.old_range().start + 1,
                            op.old_range().len(),
                            op.new_range().start + 1,
                            op.new_range().len()
                        ),
                    });
                    header_written = true;
                }

                for change in diff.iter_changes(op) {
                    let old_ln = change.old_index().map(|i| i + 1);
                    let new_ln = change.new_index().map(|i| i + 1);
                    let text = change
                        .value()
                        .trim_end_matches('\n')
                        .replace('\t', "    ")
                        .to_string();

                    let kind = match change.tag() {
                        ChangeTag::Equal => DiffLineKind::Context,
                        ChangeTag::Insert => {
                            added += 1;
                            DiffLineKind::Added
                        }
                        ChangeTag::Delete => {
                            removed += 1;
                            DiffLineKind::Removed
                        }
                    };

                    lines.push(DiffRenderLine {
                        kind,
                        old_lineno: old_ln,
                        new_lineno: new_ln,
                        text,
                    });

                    if lines.len() >= MAX_RENDER_LINES {
                        lines.push(DiffRenderLine {
                            kind: DiffLineKind::Header,
                            old_lineno: None,
                            new_lineno: None,
                            text: "... diff truncated ...".to_string(),
                        });
                        return Self {
                            file,
                            lines,
                            added,
                            removed,
                        };
                    }
                }
            }
        }

        Self {
            file,
            lines,
            added,
            removed,
        }
    }
}

pub fn render_diff(diff: &Diff, width: u16) -> Vec<Line<'static>> {
    let mut out = Vec::new();

    out.push(Line::from(vec![
        Span::styled(
            format!("▌ {}", diff.file),
            Style::default()
                .fg(Color::Rgb(210, 210, 210))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("+{}", diff.added),
            Style::default().fg(Color::Rgb(70, 190, 120)),
        ),
        Span::raw(" "),
        Span::styled(
            format!("-{}", diff.removed),
            Style::default().fg(Color::Rgb(220, 95, 90)),
        ),
    ]));
    out.push(Line::from(""));

    let content_width = width.saturating_sub(2) as usize;
    let max_ln = diff
        .lines
        .iter()
        .flat_map(|l| [l.old_lineno, l.new_lineno])
        .flatten()
        .max()
        .unwrap_or(0);
    let ln_width = max_ln.to_string().len().max(2);

    for line in &diff.lines {
        match line.kind {
            DiffLineKind::Header => {
                out.push(Line::from(Span::styled(
                    fit_line(&line.text, content_width),
                    Style::default()
                        .fg(Color::Rgb(150, 150, 210))
                        .add_modifier(Modifier::ITALIC),
                )));
            }
            DiffLineKind::Omitted => {
                out.push(Line::from(Span::styled(
                    fit_line("   …", content_width),
                    Style::default().fg(Color::Rgb(115, 115, 115)),
                )));
            }
            _ => {
                let old_ln = line
                    .old_lineno
                    .map(|n| format!("{:>width$}", n, width = ln_width))
                    .unwrap_or_else(|| " ".repeat(ln_width));
                let new_ln = line
                    .new_lineno
                    .map(|n| format!("{:>width$}", n, width = ln_width))
                    .unwrap_or_else(|| " ".repeat(ln_width));
                let marker = match line.kind {
                    DiffLineKind::Context => " ",
                    DiffLineKind::Added => "+",
                    DiffLineKind::Removed => "-",
                    _ => " ",
                };

                let content_prefix = format!("{old_ln} {new_ln} │{marker} ");
                let content = fit_line(&format!("{content_prefix}{}", line.text), content_width);

                let style = match line.kind {
                    DiffLineKind::Context => Style::default().fg(Color::Rgb(145, 145, 145)),
                    DiffLineKind::Added => Style::default()
                        .fg(Color::Rgb(45, 175, 95))
                        .bg(Color::Rgb(18, 45, 28)),
                    DiffLineKind::Removed => Style::default()
                        .fg(Color::Rgb(220, 90, 80))
                        .bg(Color::Rgb(52, 24, 24)),
                    _ => Style::default().fg(Color::Rgb(145, 145, 145)),
                };
                out.push(Line::from(Span::styled(content, style)));
            }
        }
    }

    if diff.lines.is_empty() {
        out.push(Line::from(Span::styled(
            "(no visible line-level diff)",
            Style::default()
                .fg(Color::Rgb(125, 125, 125))
                .add_modifier(Modifier::ITALIC),
        )));
    }

    out
}

fn fit_line(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
