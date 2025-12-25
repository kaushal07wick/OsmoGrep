//! ui/helpers.rs
//!
//! Shared UI helper utilities.


use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use crate::state::{Phase, RiskLevel, TestDecision};
use crate::state::ChangeSurface;

pub fn spinner(frame: usize) -> &'static str {
    const FRAMES: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];
    FRAMES[frame % FRAMES.len()]
}

/// Phase badge (symbol, label, color).
pub fn phase_badge(phase: &Phase) -> (&'static str, &'static str, Color) {
    match phase {
        Phase::Idle => ("●", "Idle", Color::Yellow),
        Phase::ExecuteAgent => ("▶", "Running", Color::Cyan),
        Phase::CreateNewAgent => ("＋", "Creating", Color::Blue),
        Phase::Running => ("⏳", "Running", Color::Green),
        Phase::Rollback => ("↩", "Rollback", Color::Magenta),
        Phase::Done => ("✔", "Done", Color::Green),
        _ => ("○", "Unknown", Color::DarkGray),
    }
}


/// Language badge (emoji label, color).
pub fn language_badge(lang: &str) -> (&'static str, Color) {
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

/// Test framework badge (emoji label, color).
pub fn framework_badge(fw: &str) -> (&'static str, Color) {
    match fw {
        "CargoTest" => ("🦀🧪 Cargo", Color::Cyan),
        "Pytest" => ("🐍🧪 Pytest", Color::Yellow),
        "GoTest" => ("🐹🧪 Go test", Color::Blue),
        "JUnit" => ("☕🧪 JUnit", Color::Red),
        "None" => ("⚪ No tests", Color::DarkGray),
        _ => ("❓ Unknown", Color::DarkGray),
    }
}


/// Fixed-width line number span.
pub fn ln(n: usize, color: Color) -> Span<'static> {
    Span::styled(
        format!("{:>4} ", n),
        Style::default().fg(color),
    )
}

/// Color mapping for test decision.
pub fn decision_color(d: &TestDecision) -> Color {
    match d {
        TestDecision::Yes => Color::Red,
        TestDecision::Conditional => Color::Yellow,
        TestDecision::No => Color::Green,
    }
}

/// Color mapping for risk level.
pub fn risk_color(r: &RiskLevel) -> Color {
    match r {
        RiskLevel::High => Color::Red,
        RiskLevel::Medium => Color::Yellow,
        RiskLevel::Low => Color::Green,
    }
}

/// Used for diff horizontal scrolling.
pub fn hclip(s: &str, x: usize, width: usize) -> &str {
    if width == 0 {
        return "";
    }

    let mut start = None;
    let mut end = None;

    for (i, (byte_idx, _)) in s.char_indices().enumerate() {
        if i == x {
            start = Some(byte_idx);
        }
        if i == x + width {
            end = Some(byte_idx);
            break;
        }
    }

    match (start, end) {
        (Some(a), Some(b)) if a < b => &s[a..b],
        (Some(a), None) if a < s.len() => &s[a..],
        _ => "",
    }
}


/// Style for function / symbol names.
pub fn symbol_style() -> Style {
    Style::default()
        .fg(Color::Red)
        .add_modifier(Modifier::BOLD)
}


/// Color mapping for diff change surface.
pub fn surface_color(surface: &ChangeSurface) -> Color {
    match surface {
        // Safe / low-risk surfaces → muted
        ChangeSurface::PureLogic
        | ChangeSurface::Cosmetic
        | ChangeSurface::Observability => Color::DarkGray,

        // Conditional / structural changes → warning
        ChangeSurface::Branching
        | ChangeSurface::Integration => Color::Yellow,

        // API / state / error paths → attention
        ChangeSurface::Contract
        | ChangeSurface::State
        | ChangeSurface::ErrorPath => Color::Red,
    }
}

/// Styled label for diff surface.
pub fn surface_style(surface: &ChangeSurface) -> Style {
    Style::default()
        .fg(surface_color(surface))
        .add_modifier(Modifier::BOLD)
}

/// Style for keywords / glue text.
pub fn keyword_style() -> Style {
    Style::default().fg(Color::DarkGray)
}
