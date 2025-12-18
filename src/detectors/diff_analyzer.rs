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
        .filter(|(file, _)| should_analyze(file))
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
    let (decision, risk, reason) = decide_test(file, hunks, &surface);

    let symbol = if is_supported_code_file(file) {
        detect_symbol(file, hunks)
    } else {
        None
    };

    let mut delta = None;
    let mut pretty = None;

    if matches!(decision, TestDecision::Yes | TestDecision::Conditional) {
        if let Some(sym) = &symbol {
            if let Some(d) = compute_symbol_delta(base_branch, file, sym) {
                pretty = Some(pretty_diff(
                    Some(&d.old_source),
                    Some(&d.new_source),
                ));
                delta = Some(d);
            }
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
    if is_python_file(file) {
        if python_behavior_change(hunks) {
            return ChangeSurface::Branching;
        }
    }

    if is_stateful(hunks) {
        return ChangeSurface::State;
    }

    if is_error_path(hunks) {
        return ChangeSurface::ErrorPath;
    }

    if is_contract(hunks) {
        return ChangeSurface::Contract;
    }

    ChangeSurface::PureLogic
}

/* ============================================================
   Decision + risk logic
   ============================================================ */

fn decide_test(
    file: &str,
    hunks: &str,
    surface: &ChangeSurface,
) -> (TestDecision, RiskLevel, String) {
    if is_always_low(file) {
        return (
            TestDecision::No,
            RiskLevel::Low,
            "Non-runtime or structural file".into(),
        );
    }

    // Python is primary
    if is_python_file(file) {
        if python_behavior_change(hunks) {
            return (
                TestDecision::Yes,
                RiskLevel::High,
                "Python behavioral change affecting execution paths".into(),
            );
        }

        return (
            TestDecision::Conditional,
            RiskLevel::Medium,
            "Python logic change".into(),
        );
    }

    // Rust default
    if is_rust_file(file) {
        return (
            TestDecision::Conditional,
            RiskLevel::Medium,
            "Rust code change (default medium)".into(),
        );
    }

    (
        TestDecision::No,
        RiskLevel::Low,
        "Non-code change".into(),
    )
}

/* ============================================================
   File classification
   ============================================================ */

fn should_analyze(file: &str) -> bool {
    if file.starts_with('.') {
        return false;
    }

    !matches!(
        file,
        "pyproject.toml"
            | "poetry.lock"
            | "Cargo.toml"
            | "Cargo.lock"
    )
}

fn is_supported_code_file(file: &str) -> bool {
    is_python_file(file) || is_rust_file(file)
}

fn is_python_file(file: &str) -> bool {
    file.ends_with(".py")
}

fn is_rust_file(file: &str) -> bool {
    file.ends_with(".rs")
}

fn is_always_low(file: &str) -> bool {
    file.ends_with("mod.rs")
        || file == "pyproject.toml"
        || file == "poetry.lock"
}

/* ============================================================
   Python-focused heuristics
   ============================================================ */

fn python_behavior_change(text: &str) -> bool {
    text.contains("def ")
        || text.contains("class ")
        || text.contains("async ")
        || text.contains("await ")
        || text.contains("@")
        || text.contains("raise ")
        || text.contains("except ")
        || text.contains("import ")
        || text.contains("from ")
}

/* ============================================================
   Shared heuristics
   ============================================================ */

fn is_contract(text: &str) -> bool {
    text.contains("def ")
        || text.contains("pub fn")
        || text.contains("->")
}

fn is_error_path(text: &str) -> bool {
    text.contains("raise ")
        || text.contains("unwrap")
        || text.contains("expect")
        || text.contains("Err(")
}

fn is_stateful(text: &str) -> bool {
    text.contains("=")
        && (text.contains("self.")
            || text.contains("global ")
            || text.contains("INSERT")
            || text.contains("UPDATE")
            || text.contains("DELETE"))
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
