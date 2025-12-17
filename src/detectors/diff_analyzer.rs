use crate::git;
use crate::state::{ChangeSurface, DiffAnalysis, RiskLevel, TestDecision};
use crate::detectors::ast::ast::detect_symbol;

pub fn analyze_diff() -> Vec<DiffAnalysis> {
    let raw = git::diff_cached();
    let diff = String::from_utf8_lossy(&raw);

    split_diff_by_file(&diff)
        .into_iter()
        .map(|(file, hunks)| analyze_file(&file, &hunks))
        .collect()
}

/* ============================================================
   Core analysis
   ============================================================ */

fn analyze_file(file: &str, hunks: &str) -> DiffAnalysis {
    let surface = detect_surface(file, hunks);
    let (decision, risk, reason) = decide_test(file, &surface);

    // AST symbol detection for Rust + Python only
    let symbol = if is_supported_code_file(file) {
        detect_symbol(file, hunks)
    } else {
        None
    };

    DiffAnalysis {
        file: file.to_string(),
        symbol,
        surface,
        test_required: decision,
        risk,
        reason,
    }
}

/* ============================================================
   Surface detection
   ============================================================ */

fn detect_surface(file: &str, hunks: &str) -> ChangeSurface {
    // Config files → never require tests
    if is_config_file(file) {
        return ChangeSurface::Cosmetic;
    }

    // Non-code files → no importance
    if !is_supported_code_file(file) {
        return ChangeSurface::Cosmetic;
    }

    // Orchestration files
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

/* ============================================================
   Decision table
   ============================================================ */

fn decide_test(
    file: &str,
    surface: &ChangeSurface,
) -> (TestDecision, RiskLevel, String) {
    // Config & non-code files
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
            "Orchestration or wiring logic changed".into(),
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
