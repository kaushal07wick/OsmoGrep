use crate::git;
use crate::state::{ChangeSurface, DiffAnalysis, RiskLevel, TestDecision};

pub fn analyze_diff() -> Vec<DiffAnalysis> {
    let raw = git::diff();
    let diff = String::from_utf8_lossy(&raw);

    split_diff_by_file(&diff)
        .into_iter()
        .map(|(file, hunks)| analyze_file(&file, &hunks))
        .collect()
}

/* ---------- core analysis ---------- */

fn analyze_file(file: &str, hunks: &str) -> DiffAnalysis {
    let surface = detect_surface(file, hunks);
    let (decision, risk, reason) = decide_test(&surface);

    DiffAnalysis {
        file: file.to_string(),
        symbol: None,
        surface,
        test_required: decision,
        risk,
        reason,
    }
}

/* ---------- surface detection ---------- */

fn detect_surface(file: &str, hunks: &str) -> ChangeSurface {
    // File intent bias (NOT exclusion)
    if is_ui_file(file) && !has_behavior(hunks) {
        return ChangeSurface::Cosmetic;
    }

    if is_orchestration_file(file) {
        return ChangeSurface::Integration;
    }

    // Content-based signals
    if is_observability(hunks) {
        ChangeSurface::Observability
    } else if is_branching(hunks) {
        ChangeSurface::Branching
    } else if is_contract(hunks) {
        ChangeSurface::Contract
    } else if is_error_path(hunks) {
        ChangeSurface::ErrorPath
    } else if is_stateful(hunks) {
        ChangeSurface::State
    } else {
        ChangeSurface::PureLogic
    }
}

/* ---------- decision table ---------- */

fn decide_test(surface: &ChangeSurface) -> (TestDecision, RiskLevel, String)
 {
    match surface {
        ChangeSurface::Cosmetic => (
            TestDecision::No,
            RiskLevel::Low,
            "UI or cosmetic-only change".into(),
        ),
        ChangeSurface::Observability => (
            TestDecision::No,
            RiskLevel::Low,
            "Logging/metrics change only".into(),
        ),
        ChangeSurface::PureLogic => (
            TestDecision::Conditional,
            RiskLevel::Medium,
            "Logic modified; test if uncovered".into(),
        ),
        ChangeSurface::Integration => (
            TestDecision::Conditional,
            RiskLevel::Medium,
            "Orchestration logic changed".into(),
        ),
        ChangeSurface::Branching
        | ChangeSurface::Contract
        | ChangeSurface::ErrorPath
        | ChangeSurface::State => (
            TestDecision::Yes,
            RiskLevel::High,
            "Behavioral path modified".into(),
        ),
    }
}

/* ---------- file classification ---------- */

fn is_ui_file(file: &str) -> bool {
    file.ends_with("/ui.rs") || file == "ui.rs"
}

fn is_orchestration_file(file: &str) -> bool {
    matches!(file, "src/main.rs" | "src/commands.rs" | "src/machine.rs")
}

/* ---------- content heuristics ---------- */

fn has_behavior(text: &str) -> bool {
    is_branching(text) || is_contract(text) || is_stateful(text)
}

fn is_observability(text: &str) -> bool {
    text.contains("println!")
        || text.contains("log::")
        || text.contains("tracing::")
}

fn is_branching(text: &str) -> bool {
    text.contains("if ")
        || text.contains("else")
        || text.contains("match ")
}

fn is_contract(text: &str) -> bool {
    text.contains("pub fn")
        || text.contains("->")
}

fn is_error_path(text: &str) -> bool {
    text.contains("unwrap")
        || text.contains("expect")
        || text.contains("Err(")
}

fn is_stateful(text: &str) -> bool {
    text.contains("fs::")
        || text.contains("File::")
        || text.contains("INSERT")
        || text.contains("UPDATE")
        || text.contains("DELETE")
}

/* ---------- diff parsing (FIXED) ---------- */

fn split_diff_by_file(diff: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut current_file: Option<String> = None;
    let mut buffer = String::new();

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(file) = current_file.take() {
                results.push((file, buffer.clone()));
                buffer.clear();
            }

            // diff --git a/foo.rs b/foo.rs
            let parts: Vec<_> = rest.split_whitespace().collect();
            let path = parts
                .get(1)
                .and_then(|p| p.strip_prefix("b/"))
                .unwrap_or(parts[1]);

            current_file = Some(path.to_string());
        } else if current_file.is_some() {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }

    if let Some(file) = current_file {
        results.push((file, buffer));
    }

    results
}
