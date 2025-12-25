//! detectors/ast/symboldelta.rs
//!
//! Extracts before/after source for a symbol.


use crate::git;
use crate::detectors::ast::ast::extract_symbol_source;
use crate::state::{SymbolDelta, DiffBaseline};

/// Compute a before/after delta for a single symbol.
///
/// Rules:
/// - Extract symbol source from both sides
/// - If symbol extraction fails on either side → return None
/// - If extracted sources are identical → return None
/// - NEVER fall back to full file
pub fn compute_symbol_delta(
    baseline: DiffBaseline,
    base_branch: &str,
    file: &str,
    symbol: &str,
) -> Option<SymbolDelta> {
    let (old_file, new_file) = match baseline {
        DiffBaseline::BaseBranch => {
            let base_commit = git::base_commit(base_branch)?;
            let old = git::show_file_at(&base_commit, file)?;
            let new = git::show_index(file)
                .or_else(|| git::show_head(file))?;
            (old, new)
        }

        DiffBaseline::Staged => {
            let old = git::show_head(file)?;
            let new = git::show_index(file)?;
            (old, new)
        }
    };
    let old_symbol = extract_symbol_source(&old_file, file, symbol)?;
    let new_symbol = extract_symbol_source(&new_file, file, symbol)?;

    if old_symbol == new_symbol {
        return None;
    }

    Some(SymbolDelta {
        old_source: old_symbol,
        new_source: new_symbol,
    })
}
