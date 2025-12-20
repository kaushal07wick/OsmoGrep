//! diff_analyzer.rs
//!
//! Extracts raw, symbol-aware diffs.
//!
//! Responsibilities:
//! - Split git diff per file
//! - Detect affected symbol (best-effort)
//! - Extract before/after source (NEVER drop diffs)
//! - Classify change surface (cheap heuristics only)
//!
//! Non-responsibilities:
//! - NO test decision
//! - NO risk judgment
//! - NO semantic interpretation

use crate::git;
use crate::state::{ChangeSurface, DiffAnalysis};
use crate::detectors::ast::ast::detect_symbol;
use crate::detectors::ast::symboldelta::compute_symbol_delta;

/* ============================================================
   Public entrypoint
   ============================================================ */

pub fn analyze_diff() -> Vec<DiffAnalysis> {
    let raw = git::diff_cached();
    if raw.is_empty() {
        return Vec::new();
    }

    let diff = String::from_utf8_lossy(&raw);
    let base_branch = git::detect_base_branch();

    split_diff_by_file(&diff)
        .into_iter()
        .filter(|(file, _)| should_analyze(file))
        .map(|(file, hunks)| analyze_file(&base_branch, &file, &hunks))
        .collect()
}

/* ============================================================
   Per-file analysis (FACTS ONLY)
   ============================================================ */

fn analyze_file(
    base_branch: &str,
    file: &str,
    hunks: &str,
) -> DiffAnalysis {
    // Cheap surface classification
    let surface = detect_surface(file, hunks);

    // Best-effort symbol detection (AST owns correctness)
    let symbol = if is_supported_code_file(file) {
        detect_symbol(file, hunks)
    } else {
        None
    };

    // 🔥 ALWAYS attempt delta extraction for code files
    let delta = if is_supported_code_file(file) {
        match &symbol {
            Some(sym) => compute_symbol_delta(base_branch, file, sym),
            None => compute_symbol_delta(base_branch, file, "<file>"),
        }
    } else {
        None
    };

    DiffAnalysis {
        file: file.to_string(),
        symbol,
        surface,
        delta,
        semantic: None, // populated later (LLM stage)
    }
}

/* ============================================================
   Diff parsing (simple + correct)
   ============================================================ */

fn split_diff_by_file(diff: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut current_file: Option<String> = None;
    let mut buffer = String::new();

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(file) = current_file.take() {
                results.push((file, std::mem::take(&mut buffer)));
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

/* ============================================================
   Surface detection (CHEAP HEURISTICS ONLY)
   ============================================================ */

fn detect_surface(file: &str, hunks: &str) -> ChangeSurface {
    if is_python_file(file) && python_behavior_change(hunks) {
        return ChangeSurface::Branching;
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

/* ============================================================
   Heuristics (non-authoritative)
   ============================================================ */

fn python_behavior_change(text: &str) -> bool {
    text.contains("def ")
        || text.contains("class ")
        || text.contains("async ")
}

fn is_contract(text: &str) -> bool {
    text.contains("def ")
        || text.contains("pub fn")
        || text.contains("fn ")
}

fn is_error_path(text: &str) -> bool {
    text.contains("raise ")
        || text.contains("unwrap")
        || text.contains("expect")
        || text.contains("Err(")
}

fn is_stateful(text: &str) -> bool {
    text.contains("self.")
        || text.contains("global ")
}
