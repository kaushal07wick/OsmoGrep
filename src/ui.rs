use std::io;
use similar::ChangeTag;
use crate::Focus;
use crate::state::{DiffKind, SinglePanelView};
use crate::state::SymbolDelta;
use crate::testgen::resolve::TestResolution;
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use std::time::{Duration, Instant};
use crate::state::{AgentState, LogLevel, Phase, TestDecision, RiskLevel};

/* ================= Helpers ================= */

fn spinner(frame: usize) -> &'static str {
    const FRAMES: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];
    FRAMES[frame % FRAMES.len()]
}

fn ln(n: usize, color: Color) -> Span<'static> {
    Span::styled(format!("{:>4} ", n), Style::default().fg(color))
}

fn inline_diff(old: &str, new: &str, color: Color) -> Vec<Span<'static>> {
    use similar::TextDiff;

    let diff = TextDiff::from_chars(old, new);

    diff.iter_all_changes().map(|c| {
        let style = match c.tag() {
            ChangeTag::Insert | ChangeTag::Delete => {
                Style::default().fg(color).add_modifier(Modifier::UNDERLINED)
            }
            _ => Style::default().fg(color),
        };
        Span::styled(c.to_string(), style)
    }).collect()
}


fn phase_badge(phase: &Phase) -> (&'static str, &'static str, Color) {
    match phase {
        Phase::Idle => ("●", "Idle", Color::Yellow),
        Phase::ExecuteAgent => ("▶", "Running", Color::Cyan),
        Phase::CreateNewAgent => ("＋", "Creating", Color::Blue),
        Phase::Rollback => ("↩", "Rollback", Color::Magenta),
        Phase::Done => ("✔", "Done", Color::Green),
        _ => ("○", "Unknown", Color::DarkGray),
    }
}

fn language_badge(lang: &str) -> (&'static str, Color) {
    match lang {
        "Rust" => ("🦀 Rust", Color::Cyan),
        "Python" => ("🐍 Python", Color::Yellow),
        "Go" => ("🐹 Go", Color::Blue),
        "TypeScript" => ("📘 TypeScript", Color::Blue),
        "JavaScript" => ("📗 JavaScript", Color::Yellow),
        "Java" => ("☕ Java", Color::Red),
        "Ruby" => ("💎 Ruby", Color::Magenta),
        _ => ("❓ Unknown", Color::DarkGray),
    }
}

fn framework_badge(fw: &str) -> (&'static str, Color) {
    match fw {
        "CargoTest" => ("🦀🧪 Cargo", Color::Cyan),
        "Pytest" => ("🐍🧪 Pytest", Color::Yellow),
        "GoTest" => ("🐹🧪 Go test", Color::Blue),
        "JUnit" => ("☕🧪 JUnit", Color::Red),
        "None" => ("⚪ No tests", Color::DarkGray),
        _ => ("❓ Unknown", Color::DarkGray),
    }
}

fn decision_color(d: &TestDecision) -> Color {
    match d {
        TestDecision::Yes => Color::Red,
        TestDecision::Conditional => Color::Yellow,
        TestDecision::No => Color::Green,
    }
}

fn risk_color(r: &RiskLevel) -> Color {
    match r {
        RiskLevel::High => Color::Red,
        RiskLevel::Medium => Color::Yellow,
        RiskLevel::Low => Color::Green,
    }
}

fn hclip(s: &str, x: usize, width: usize) -> &str {
    let mut start_idx = None;
    let mut end_idx = None;

    for (i, (byte_idx, _)) in s.char_indices().enumerate() {
        if i == x {
            start_idx = Some(byte_idx);
        }
        if i == x + width {
            end_idx = Some(byte_idx);
            break;
        }
    }

    match (start_idx, end_idx) {
        (Some(start), Some(end)) if start < end => &s[start..end],
        (Some(start), None) if start < s.len() => &s[start..],
        _ => "",
    }
}

fn render_side_by_side(
    f: &mut ratatui::Frame,
    area: Rect,
    delta: &SymbolDelta,
    scroll: usize,
    state: &AgentState,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let height = area.height as usize;
    let gutter = 5; // line number width
    let content_width = chunks[0].width.saturating_sub(gutter + 1) as usize;

    let mut old_iter = delta.old_source.lines();
    let mut new_iter = delta.new_source.lines();

    // (text, color, is_hunk, line_no)
    let mut old_buf: Vec<(String, Color, bool, Option<usize>)> = Vec::new();
    let mut new_buf: Vec<(String, Color, bool, Option<usize>)> = Vec::new();

    let mut old_ln = 1usize;
    let mut new_ln = 1usize;

    for line in &delta.lines {
        match &line.kind {
            DiffKind::Hunk(h) => {
                old_buf.push((h.to_string(), Color::Cyan, true, None));
                new_buf.push((h.to_string(), Color::Cyan, true, None));
            }

            DiffKind::Line(ChangeTag::Equal) => {
                let o = old_iter.next().unwrap_or("");
                let n = new_iter.next().unwrap_or("");
                old_buf.push((o.to_string(), Color::DarkGray, false, Some(old_ln)));
                new_buf.push((n.to_string(), Color::DarkGray, false, Some(new_ln)));
                old_ln += 1;
                new_ln += 1;
            }

            DiffKind::Line(ChangeTag::Delete) => {
                let o = old_iter.next().unwrap_or("");
                old_buf.push((o.to_string(), Color::Red, false, Some(old_ln)));
                new_buf.push(("".into(), Color::DarkGray, false, None));
                old_ln += 1;
            }

            DiffKind::Line(ChangeTag::Insert) => {
                let n = new_iter.next().unwrap_or("");
                old_buf.push(("".into(), Color::DarkGray, false, None));
                new_buf.push((n.to_string(), Color::Green, false, Some(new_ln)));
                new_ln += 1;
            }
        }
    }

    let total = old_buf.len();
    let start = scroll.min(total);
    let end = (start + height).min(total);

    let mut left = Vec::new();
    let mut right = Vec::new();

    for i in start..end {
        let (ot, oc, oh, oln) = &old_buf[i];
        let (nt, nc, nh, nln) = &new_buf[i];

        let ln_span = match (oln, nln) {
            (Some(n), _) => ln(*n, Color::DarkGray),
            _ => Span::raw("     "),
        };

        // clip AFTER line numbers, BEFORE rendering
        let ot_clip = hclip(ot, state.diff_scroll_x, content_width);
        let nt_clip = hclip(nt, state.diff_scroll_x, content_width);

        let old_spans = if !oh && !nh && ot != nt && !ot.is_empty() && !nt.is_empty() {
            inline_diff(&ot_clip, &nt_clip, *oc)
        } else {
            vec![Span::styled(ot_clip, Style::default().fg(*oc))]
        };

        let new_spans = if !oh && !nh && ot != nt && !ot.is_empty() && !nt.is_empty() {
            inline_diff(&ot_clip, &nt_clip, *nc)
        } else {
            vec![Span::styled(nt_clip, Style::default().fg(*nc))]
        };


        left.push(Line::from(
            std::iter::once(ln_span.clone())
                .chain(old_spans)
                .collect::<Vec<_>>(),
        ));

        right.push(Line::from(
            std::iter::once(ln_span)
                .chain(new_spans)
                .collect::<Vec<_>>(),
        ));
    }

    let title_style = if state.focus == Focus::Diff {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    f.render_widget(
        Paragraph::new(left).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled("OLD", title_style))
                .title_alignment(Alignment::Center),
        ),
        chunks[0],
    );

    f.render_widget(
        Paragraph::new(right).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled("NEW", title_style))
                .title_alignment(Alignment::Center),
        ),
        chunks[1],
    );
}

fn render_testgen_panel(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
    candidate: &crate::testgen::candidate::TestCandidate,
) {
    let mut lines: Vec<Line> = Vec::new();

    /* ---------- EXIT HINT ---------- */
    lines.push(Line::from(Span::styled(
        "ESC / q → return to execution view",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )));
    lines.push(Line::from(""));

    /* ---------- HEADER ---------- */
    lines.push(Line::from(Span::styled(
        "TEST GENERATION PREVIEW",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    /* ---------- META ---------- */
    lines.push(Line::from(vec![
        Span::styled("FILE: ", Style::default().fg(Color::Gray)),
        Span::styled(
            &candidate.file,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
    ]));

    if let Some(sym) = &candidate.symbol {
        lines.push(Line::from(vec![
            Span::styled("SYMBOL: ", Style::default().fg(Color::Gray)),
            Span::styled(sym, Style::default().fg(Color::White)),
        ]));
    }

    if let Some(lang) = &state.language {
        let (badge, color) = language_badge(&format!("{:?}", lang));
        lines.push(Line::from(vec![
            Span::styled("LANG: ", Style::default().fg(Color::Gray)),
            Span::styled(badge, Style::default().fg(color)),
        ]));
    }

    if let Some(fw) = &state.framework {
        let (badge, color) = framework_badge(&format!("{:?}", fw));
        lines.push(Line::from(vec![
            Span::styled("TEST FW: ", Style::default().fg(Color::Gray)),
            Span::styled(badge, Style::default().fg(color)),
        ]));
    }
    /* ---------- TEST PRESENCE ---------- */
    let resolution = crate::testgen::resolve::resolve_test(state, candidate);

    match &resolution {
        TestResolution::Found { file, test_fn } => {
            lines.push(Line::from(vec![
                Span::styled("TEST: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    "EXISTS",
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(file, Style::default().fg(Color::DarkGray)),
            ]));

            if let Some(name) = test_fn {
                lines.push(Line::from(vec![
                    Span::styled("TEST FN: ", Style::default().fg(Color::Gray)),
                    Span::styled(name, Style::default().fg(Color::White)),
                ]));
            }
        }

        TestResolution::Ambiguous(files) => {
            lines.push(Line::from(vec![
                Span::styled("TEST: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    "AMBIGUOUS",
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
            ]));

            for f in files {
                lines.push(Line::from(Span::styled(
                    format!("  • {}", f),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        TestResolution::NotFound => {
            lines.push(Line::from(vec![
                Span::styled("TEST: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    "NOT FOUND",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    "New test will be generated",
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));


    lines.push(Line::from(vec![
        Span::styled("TEST TYPE: ", Style::default().fg(Color::Gray)),
        Span::styled(
            format!("{:?}", candidate.test_type),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("   "),
        Span::styled("RISK: ", Style::default().fg(Color::Gray)),
        Span::styled(
            format!("{:?}", candidate.risk),
            Style::default().fg(risk_color(&candidate.risk)),
        ),
    ]));

    lines.push(Line::from(""));

    /* ---------- BEHAVIOR ---------- */
    lines.push(Line::from(Span::styled(
        "BEHAVIOR",
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::raw(&candidate.behavior)));
    lines.push(Line::from(""));

    /* ---------- FAILURE MODE ---------- */
    lines.push(Line::from(Span::styled(
        "FAILURE MODE",
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::raw(&candidate.failure_mode)));
    lines.push(Line::from(""));

    /* ---------- OLD CODE ---------- */
    if let Some(old) = &candidate.old_code {
        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled(
            "──────── OLD CODE (before change) ────────",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));

        lines.push(Line::from(""));

        for l in old.lines() {
            lines.push(Line::from(Span::styled(
                l,
                Style::default().fg(Color::Red),
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(""));
    }

    if let Some(new) = &candidate.new_code {
        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled(
            "──────── NEW CODE (to be tested) ────────",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        )));

        lines.push(Line::from(""));

        for l in new.lines() {
            lines.push(Line::from(Span::styled(
                l,
                Style::default().fg(Color::Green),
            )));
        }
    }

    /* ---------- SCROLL ---------- */
    let height = area.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(height);
    let scroll = state.exec_scroll.min(max_scroll);

    let visible = lines
        .into_iter()
        .skip(scroll)
        .take(height)
        .collect::<Vec<_>>();

    let block = Block::default()
        .borders(Borders::ALL)
        .title("TESTGEN")
        .title_alignment(Alignment::Center);

    f.render_widget(Paragraph::new(visible).block(block), area);
}


/* ================= UI ================= */

pub fn draw_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<(Rect, Rect, Rect)>
{
    let mut input_rect = Rect::default();
    let mut diff_rect = Rect::default();
    let mut exec_rect = Rect::default();

    terminal.draw(|f| {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(8), // header
                Constraint::Length(8), // status
                Constraint::Min(6),    // execution
                Constraint::Length(3), // command
            ])
            .split(f.size());

        /* ================= HEADER ================= */

        let header_lines = [
            "░█████╗░░██████╗███╗░░░███╗░█████╗░░██████╗░██████╗░███████╗██████╗░",
            "██╔══██╗██╔════╝████╗░████║██╔══██╗██╔════╝░██╔══██╗██╔════╝██╔══██╗",
            "██║░░██║╚█████╗░██╔████╔██║██║░░██║██║░░██╗░██████╔╝█████╗░░██████╔╝",
            "██║░░██║░╚═══██╗██║╚██╔╝██║██║░░██║██║░░╚██╗██╔══██╗██╔══╝░░██╔═══╝░",
            "╚█████╔╝██████╔╝██║░╚═╝░██║╚█████╔╝╚██████╔╝██║░░██║███████╗██║░░░░░",
            "░╚════╝░╚═════╝░╚═╝░░░░░╚═╝░╚════╝░░╚═════╝░╚═╝░░╚═╝╚══════╝╚═╝░░░░░",
        ];

        let header = Paragraph::new(
            header_lines
                .iter()
                .map(|l| {
                    Line::from(Span::styled(
                        *l,
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ))
                })
                .collect::<Vec<_>>(),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));

        f.render_widget(header, layout[0]);

        /* ================= STATUS ================= */

        let (phase_sym, phase_label, phase_color) = phase_badge(&state.phase);
        let mut status_lines = Vec::new();

        /* -------- Phase (what the system is doing) -------- */

        status_lines.push(Line::from(vec![
            Span::styled("Phase: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{phase_sym} {phase_label}"),
                Style::default()
                    .fg(phase_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        /* -------- Activity (whether user/system is active) -------- */

        let idle_timeout = Duration::from_secs(10);
        let now = Instant::now();

        let is_active =
            state.selected_diff.is_some()
            || state.panel_view.is_some()
            || !matches!(state.phase, Phase::Idle)
            || now.duration_since(state.last_activity) < idle_timeout;

        let (status_label, status_color) = if is_active {
            ("Active", Color::Green)
        } else {
            ("Idle", Color::Yellow)
        };

        let mut activity_spans = vec![
            Span::styled("Status: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("● {}", status_label),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        if is_active {
            activity_spans.push(Span::raw(" "));
            activity_spans.push(Span::styled(
                spinner(state.spinner_tick),
                Style::default().fg(Color::Green),
            ));
        }

        status_lines.push(Line::from(activity_spans));


        /* -------- Branches -------- */

        status_lines.push(Line::from(vec![
            Span::styled("Current Branch: ", Style::default().fg(Color::Gray)),
            Span::styled(
                state.current_branch
                    .clone()
                    .unwrap_or_else(|| "unknown".into()),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        status_lines.push(Line::from(vec![
            Span::styled("Base Branch: ", Style::default().fg(Color::Gray)),
            Span::styled(
                state.base_branch.clone().unwrap_or_else(|| "unknown".into()),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        status_lines.push(Line::from(vec![
            Span::styled("Agent Branch: ", Style::default().fg(Color::Gray)),
            Span::styled(
                state.agent_branch.clone().unwrap_or_else(|| "none".into()),
                Style::default().fg(Color::Yellow),
            ),
        ]));

        /* -------- Language / framework -------- */

        if let Some(lang) = &state.language {
            let (badge, color) = language_badge(&format!("{:?}", lang));
            status_lines.push(Line::from(vec![
                Span::styled("Language: ", Style::default().fg(Color::Gray)),
                Span::styled(badge, Style::default().fg(color)),
            ]));
        }

        if let Some(fw) = &state.framework {
            let (badge, color) = framework_badge(&format!("{:?}", fw));
            status_lines.push(Line::from(vec![
                Span::styled("Framework: ", Style::default().fg(Color::Gray)),
                Span::styled(badge, Style::default().fg(color)),
            ]));
        }

        let status = Paragraph::new(status_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("STATUS")
                .title_alignment(Alignment::Center),
        );

        f.render_widget(status, layout[1]);


        /* ================= EXECUTION ================= */

        let mut exec_lines = Vec::new();
        exec_lines.push(Line::from(""));

        let mut rendered_custom = false;

        /* ---------- SINGLE PANEL VIEW (TESTGEN, LLM, RESULTS) ---------- */
        if let Some(view) = &state.panel_view {
            match view {
                SinglePanelView::TestGenPreview(c) => {
                    render_testgen_panel(f, layout[2], state, c);
                    rendered_custom = true;
                }
            }
        }

        /* ---------- SIDE-BY-SIDE DIFF VIEW ---------- */
        else if let Some(idx) = state.selected_diff {
            if let Some(delta) = &state.diff_analysis[idx].delta {
                render_side_by_side(
                    f,
                    layout[2],
                    delta,
                    state.diff_scroll,
                    state,
                );
                rendered_custom = true;
            }
        }

        /* ---------- NORMAL EXECUTION VIEW ---------- */
        if !rendered_custom {
            use std::time::{Duration, Instant};

            let height = layout[2].height.saturating_sub(2) as usize; // borders
            let mut all_lines: Vec<Line> = Vec::new();
            let mut diff_idx = 0;

            let now = Instant::now();
            let fade_after = Duration::from_secs(3);

            /* -------- BUILD FULL CHRONOLOGICAL STREAM -------- */

            for l in &state.logs {
                let expired = matches!(l.level, LogLevel::Warn | LogLevel::Error)
                    && now.duration_since(l.at) > fade_after;

                if expired {
                    continue; 
                }

                if l.text == "__DIFF_ANALYSIS__" {
                    all_lines.push(Line::from(""));
                    all_lines.push(Line::from(Span::styled(
                        "Diff Analysis",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )));
                    all_lines.push(Line::from(""));

                    while diff_idx < state.diff_analysis.len() {
                        let d = &state.diff_analysis[diff_idx];
                        diff_idx += 1;

                        all_lines.push(Line::from(vec![
                            Span::styled(
                                match &d.symbol {
                                    Some(sym) => format!("{} :: {}", d.file, sym),
                                    None => d.file.clone(),
                                },
                                Style::default().fg(Color::White),
                            ),
                            Span::raw(" | "),
                            Span::styled(
                                format!("{:?}", d.test_required),
                                Style::default().fg(decision_color(&d.test_required)),
                            ),
                            Span::raw(" | "),
                            Span::styled(
                                format!("{:?}", d.risk),
                                Style::default().fg(risk_color(&d.risk)),
                            ),
                        ]));

                        all_lines.push(Line::from(Span::styled(
                            format!("  ↳ {}", d.reason),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                } else {
                    let color = match l.level {
                        LogLevel::Info => Color::White,
                        LogLevel::Success => Color::Green,
                        LogLevel::Warn => Color::Yellow,
                        LogLevel::Error => Color::Red,
                    };

                    let style = if matches!(l.level, LogLevel::Warn | LogLevel::Error)
                        && now.duration_since(l.at).as_secs_f32() > 2.0
                    {
                        Style::default().fg(color).add_modifier(Modifier::DIM)
                    } else {
                        Style::default().fg(color)
                    };

                    all_lines.push(Line::from(Span::styled(&l.text, style)));
                }
            }

            /* -------- APPLY SCROLL -------- */

            let total = all_lines.len();
            /* terminal-style scroll clamp */
            let max_scroll = total.saturating_sub(height);
            let scroll = state.exec_scroll.min(max_scroll);

            let start = scroll;
            let end = (start + height).min(total);
            let visible = all_lines[start..end].to_vec();

            let execution = Paragraph::new(visible).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("EXECUTION")
                    .title_alignment(Alignment::Center),
            );

            f.render_widget(execution, layout[2]);
            exec_rect = layout[2];
        }


        /* -------- DIFF RECT -------- */

        if state.selected_diff.is_some() {
            diff_rect = layout[2];
        }

        /* ================= COMMAND ================= */

        let mut spans = Vec::new();
        spans.push(Span::styled("$_ ", Style::default().fg(Color::Cyan)));
        spans.push(Span::styled(&state.input, Style::default().fg(Color::White)));

        if let Some(ac) = &state.autocomplete {
            if ac.starts_with(&state.input) {
                let suffix = &ac[state.input.len()..];
                if !suffix.is_empty() {
                    spans.push(Span::styled(
                        suffix,
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
        }

        let input = Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .title("COMMAND")
                .title_alignment(Alignment::Center),
        );

        input_rect = layout[3];
        f.render_widget(input, input_rect);

        if state.focus == Focus::Input {
            f.set_cursor(
                input_rect.x + 4 + state.input.len() as u16,
                input_rect.y + 1,
            );
        }

    })?;

    Ok((input_rect, diff_rect, exec_rect))
}