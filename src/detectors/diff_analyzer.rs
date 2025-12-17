use crate::git;
use crate::state::{ChangeSurface, DiffAnalysis, RiskLevel, TestDecision};
use crate::detectors::ast::ast::detect_symbol;
use crate::detectors::ast::symboldelta::compute_symbol_delta;
use crate::detectors::ast::pretty::pretty_diff;

/* ============================================================
   Public entry
   ============================================================ */

pub fn analyze_diff() -> Vec<DiffAnalysis> {
    let raw = git::diff_cached();
    let diff = String::from_utf8_lossy(&raw);

    let base_branch = git::detect_base_branch();

    split_diff_by_file(&diff)
        .into_iter()
        .map(|(file, hunks)| analyze_file(&base_branch, &file, &hunks))
        .collect()
}

/* ============================================================
   Core analysis
   ============================================================ */

fn analyze_file(
    base_branch: &str,
    file: &str,
    hunks: &str,
) -> DiffAnalysis {
    let surface = detect_surface(file, hunks);
    let (decision, risk, reason) = decide_test(file, &surface);

    // AST symbol detection only for supported code files
    let symbol = if is_supported_code_file(file) {
        detect_symbol(file, hunks)
    } else {
        None
    };

    let mut delta = None;
    let mut pretty = None;

    if let Some(sym) = &symbol {
        if let Some(d) = compute_symbol_delta(base_branch, file, sym) {
            pretty = Some(pretty_diff(
                Some(&d.old_source),
                Some(&d.new_source),
            ));
            delta = Some(d);
        }
    }

    DiffAnalysis {
        file: file.to_string(),
        symbol,
        surface,
        test_required: decision,
        risk,
        reason,
        delta,
        pretty,
    }
}

/* ============================================================
   Surface detection
   ============================================================ */

fn detect_surface(file: &str, hunks: &str) -> ChangeSurface {
    // Config or non-code files → cosmetic only
    if is_config_file(file) || !is_supported_code_file(file) {
        return ChangeSurface::Cosmetic;
    }

    if is_orchestration_file(file) {
        return ChangeSurface::Integration;
    }

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

/* ============================================================
   Decision table
   ============================================================ */

fn decide_test(
    file: &str,
    surface: &ChangeSurface,
) -> (TestDecision, RiskLevel, String) {
    if is_config_file(file) || !is_supported_code_file(file) {
        return (
            TestDecision::No,
            RiskLevel::Low,
            "Non-runtime or configuration change".into(),
        );
    }

    match surface {
        ChangeSurface::Cosmetic => (
            TestDecision::No,
            RiskLevel::Low,
            "Cosmetic or formatting-only change".into(),
        ),
        ChangeSurface::Observability => (
            TestDecision::No,
            RiskLevel::Low,
            "Logging or metrics change only".into(),
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

/* ============================================================
   File classification
   ============================================================ */

fn is_supported_code_file(file: &str) -> bool {
    file.ends_with(".rs") || file.ends_with(".py")
}

fn is_config_file(file: &str) -> bool {
    matches!(
        file,
        "Cargo.toml"
            | "Cargo.lock"
            | "pyproject.toml"
            | "requirements.txt"
    ) || file.ends_with(".json")
        || file.ends_with(".yaml")
        || file.ends_with(".yml")
}

fn is_orchestration_file(file: &str) -> bool {
    matches!(
        file,
        "src/main.rs"
            | "src/commands.rs"
            | "src/machine.rs"
    )
}

/* ============================================================
   Content heuristics
   ============================================================ */

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

/* ============================================================
   Diff parsing
   ============================================================ */

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
