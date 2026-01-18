use std::process::Command;
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

pub fn collect_unstaged_diffs(repo_root: &std::path::Path) -> Vec<Diff> {
    let out = Command::new("git")
        .args(["diff", "--no-color", "--unified=3"])
        .current_dir(repo_root)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    parse_git_diff(&out)
}

fn parse_git_diff(input: &str) -> Vec<Diff> {
    let mut diffs = Vec::new();

    let mut current_file: Option<String> = None;
    let mut hunks: Vec<DiffLine> = Vec::new();

    for line in input.lines() {
        if line.starts_with("diff --git ") {
            if let Some(file) = current_file.take() {
                if !hunks.is_empty() {
                    diffs.push(Diff { file, hunks });
                }
                hunks = Vec::new();
            }

            let file = line
                .split_whitespace()
                .nth(3)
                .and_then(|s| s.strip_prefix("b/"))
                .unwrap_or("")
                .to_string();

            if file.starts_with('.') || file.is_empty() {
                current_file = None;
            } else {
                current_file = Some(file);
            }

            continue;
        }

        if current_file.is_none() {
            continue;
        }

        if line.starts_with("+++")
            || line.starts_with("---")
            || line.starts_with("@@")
        {
            continue;
        }

        if let Some(t) = line.strip_prefix('+') {
            hunks.push(DiffLine::Added(t.to_string()));
        } else if let Some(t) = line.strip_prefix('-') {
            hunks.push(DiffLine::Removed(t.to_string()));
        } else {
            hunks.push(DiffLine::Context(line.to_string()));
        }
    }

    if let Some(file) = current_file {
        if !hunks.is_empty() {
            diffs.push(Diff { file, hunks });
        }
    }

    diffs
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
