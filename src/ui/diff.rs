use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

#[derive(Debug, Clone, Copy)]
pub enum DiffView {
    SideBySide,
    Unified,
}

#[derive(Debug, Clone)]
pub enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}

#[derive(Debug, Clone)]
pub struct Diff {
    pub file: String,
    pub hunks: Vec<DiffLine>,
}
impl Diff {
    pub fn from_texts(file: String, before: &str, after: &str) -> Self {
        let hunks = diff_strings(before, after);
        Self { file, hunks }
    }
}


pub fn choose_diff_view(changed: usize) -> DiffView {
    if changed <= 80 {
        DiffView::SideBySide
    } else {
        DiffView::Unified
    }
}

pub fn render_diff(diff: &Diff, width: u16) -> Vec<Line<'static>> {
    let changed = diff
        .hunks
        .iter()
        .filter(|h| !matches!(h, DiffLine::Context(_)))
        .count();

    let mut out = Vec::new();

    out.push(Line::from(Span::styled(
        format!("▌ {}", diff.file),
        Style::default()
            .fg(Color::Rgb(210, 210, 210))
            .add_modifier(Modifier::BOLD),
    )));

    out.push(Line::from(""));

    match choose_diff_view(changed) {
        DiffView::Unified => out.extend(render_unified(&diff.hunks)),
        DiffView::SideBySide => out.extend(render_side_by_side(&diff.hunks, width)),
    }

    out
}

fn diff_strings(before: &str, after: &str) -> Vec<DiffLine> {
    let mut hunks = Vec::new();

    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();

    let mut i = 0;
    let mut j = 0;

    while i < before_lines.len() || j < after_lines.len() {
        match (before_lines.get(i), after_lines.get(j)) {
            (Some(&b), Some(&a)) if b == a => {
                hunks.push(DiffLine::Context(b.to_string()));
                i += 1;
                j += 1;
            }

            (Some(&b), Some(&a)) => {
                hunks.push(DiffLine::Removed(b.to_string()));
                hunks.push(DiffLine::Added(a.to_string()));
                i += 1;
                j += 1;
            }

            (Some(&b), None) => {
                hunks.push(DiffLine::Removed(b.to_string()));
                i += 1;
            }

            (None, Some(&a)) => {
                hunks.push(DiffLine::Added(a.to_string()));
                j += 1;
            }

            (None, None) => break,
        }
    }

    hunks
}


fn render_unified(hunks: &[DiffLine]) -> Vec<Line<'static>> {
    let mut out = Vec::new();

    for h in hunks {
        match h {
            DiffLine::Context(t) => out.push(Line::from(Span::styled(
                format!("  {t}"),
                Style::default().fg(Color::Rgb(160, 160, 160)),
            ))),

            DiffLine::Added(t) => out.push(Line::from(Span::styled(
                format!("+ {t}"),
                Style::default()
                    .fg(Color::Rgb(40, 180, 99))
                    .bg(Color::Rgb(25, 60, 40))
                    .add_modifier(Modifier::BOLD),
            ))),

            DiffLine::Removed(t) => out.push(Line::from(Span::styled(
                format!("- {t}"),
                Style::default()
                    .fg(Color::Rgb(231, 76, 60))
                    .bg(Color::Rgb(60, 30, 30)),
            ))),
        }
    }

    out
}

fn render_side_by_side(hunks: &[DiffLine], width: u16) -> Vec<Line<'static>> {
    let gutter = 3;
    let col_width = (width as usize).saturating_sub(gutter) / 2;

    let mut left = Vec::new();
    let mut right = Vec::new();

    for h in hunks {
        match h {
            DiffLine::Context(t) => {
                left.push(Some((t.clone(), Style::default())));
                right.push(Some((t.clone(), Style::default())));
            }
            DiffLine::Removed(t) => {
                left.push(Some((
                    t.clone(),
                    Style::default()
                        .fg(Color::Rgb(231, 76, 60))
                        .bg(Color::Rgb(60, 30, 30)),
                )));
                right.push(None);
            }
            DiffLine::Added(t) => {
                left.push(None);
                right.push(Some((
                    t.clone(),
                    Style::default()
                        .fg(Color::Rgb(40, 180, 99))
                        .bg(Color::Rgb(25, 60, 40)),
                )));
            }
        }
    }

    let mut out = Vec::new();
    let max = left.len().max(right.len());

    for i in 0..max {
        let l = left.get(i).and_then(|v| v.clone());
        let r = right.get(i).and_then(|v| v.clone());

        let l_span = match l {
            Some((t, s)) => Span::styled(pad_or_trim(&t, col_width), s),
            None => Span::raw(" ".repeat(col_width)),
        };

        let r_span = match r {
            Some((t, s)) => Span::styled(pad_or_trim(&t, col_width), s),
            None => Span::raw(" ".repeat(col_width)),
        };

        out.push(Line::from(vec![
            l_span,
            Span::styled(" │ ", Style::default().fg(Color::Rgb(80, 80, 80))),
            r_span,
        ]));
    }

    out
}

fn pad_or_trim(s: &str, max: usize) -> String {
    if s.len() <= max {
        format!("{s:width$}", width = max)
    } else {
        let mut out = s[..max.saturating_sub(1)].to_string();
        out.push('…');
        out
    }
}
