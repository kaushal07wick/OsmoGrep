//! Extracts before/after source for a symbol.

use crate::detectors::ast::ast::extract_symbol_source;
use crate::state::SymbolDelta;

/// Compute a before/after delta for a single symbol.
///
/// Rules:
/// - Extract symbol source from both sides
/// - If extraction fails on either side → return None
/// - If extracted sources are identical → return None
pub fn compute_symbol_delta(
    old_source: &str,
    new_source: &str,
    file: &str,
    symbol: &str,
) -> Option<SymbolDelta> {
    let old_symbol = extract_symbol_source(old_source, file, symbol)?;
    let new_symbol = extract_symbol_source(new_source, file, symbol)?;

    if old_symbol == new_symbol {
        return None;
    }

    Some(SymbolDelta {
        old_source: old_symbol,
        new_source: new_symbol,
    })
}
