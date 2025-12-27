//! diff_analyzer.rs
//!
//! Extracts raw, symbol-aware diffs.
//!
//! Responsibilities:
//! - Read git diff
//! - Load file contents
//! - Detect affected symbols
//! - Compute before/after deltas
//! - Classify change surface
//!
//! Does NOT:
//! - Parse AST directly
//! - Perform semantic reasoning
//! - Mutate global state

use crate::git;
use crate::state::{ChangeSurface, DiffAnalysis, DiffBaseline};
use crate::detectors::ast::ast::detect_symbol;
use crate::detectors::ast::symboldelta::compute_symbol_delta;

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
        .filter_map(|(file, hunks)| analyze_file(&base_branch, &file, &hunks))
        .collect()
}

fn analyze_file(
    base_branch: &str,
    file: &str,
    hunks: &str,
) -> Option<DiffAnalysis> {
    let is_code = is_supported_code_file(file);

    let surface = if is_code {
        detect_surface(file, hunks)
    } else {
        ChangeSurface::Cosmetic
    };

    // Load file contents (ONLY place touching git)
    let (old_src, new_src) = if is_code {
        match DiffBaseline::Staged {
            DiffBaseline::BaseBranch => {
                let base = git::base_commit(base_branch)?;
                (
                    git::show_file_at(&base, file)?,
                    git::show_index(file)?,
                )
            }
            DiffBaseline::Staged => (
                git::show_head(file)?,
                git::show_index(file)?,
            ),
        }
    } else {
        (String::new(), String::new())
    };

    // Symbol detection (pure)
    let symbol = if is_code {
        detect_symbol(&new_src, hunks, file)
    } else {
        None
    };

    // Symbol delta (pure)
    let delta = match (&symbol, is_code) {
        (Some(sym), true) => {
            compute_symbol_delta(&old_src, &new_src, file, sym)
        }
        _ => None,
    };

    Some(DiffAnalysis {
        file: file.to_string(),
        symbol,
        surface,
        delta,
        summary: None,
    })
}

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
