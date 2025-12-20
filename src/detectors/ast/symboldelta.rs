//! detectors/ast/symboldelta.rs
//!
//! Extracts before/after source for a symbol.
//! Falls back to file-level delta if symbol extraction fails.
//!
//! FACTS ONLY — no semantics, no confidence, no interpretation.

use crate::git;
use crate::detectors::ast::ast::extract_symbol_source;
use crate::state::SymbolDelta;

/// Compute a before/after delta between base branch and index/HEAD.
///
/// Rules:
/// - Try symbol-level extraction first
/// - Fall back to file-level extraction if symbol fails
/// - Return None ONLY if before == after
pub fn compute_symbol_delta(
    base_branch: &str,
    file: &str,
    symbol: &str,
) -> Option<SymbolDelta> {
    let base_commit = git::base_commit(base_branch)?;

    let old_file = git::show_file_at(&base_commit, file)?;
    let new_file = git::show_index(file)
        .or_else(|| git::show_head(file))?;

    // Try symbol-level extraction
    let old_symbol = extract_symbol_source(&old_file, file, symbol);
    let new_symbol = extract_symbol_source(&new_file, file, symbol);

    let (old_src, new_src) = match (old_symbol, new_symbol) {
        (Some(o), Some(n)) => (o, n),
        _ => (old_file, new_file), // fallback is intentional and visible upstream
    };

    if old_src == new_src {
        return None;
    }

    Some(SymbolDelta {
        old_source: old_src,
        new_source: new_src,
    })
}
