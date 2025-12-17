/* src/detectors/ast/symboldelta.rs */
use crate::git;
use crate::detectors::ast::ast::extract_symbol_source;
use crate::state::{AgentState, SymbolDelta};

pub fn compute_symbol_delta(
    base_branch: &str,
    file: &str,
    symbol: &str,
) -> Option<SymbolDelta> {
    let base_commit = git::base_commit(base_branch)?;

    let old_file = git::show_file_at(&base_commit, file)?;
    let new_file = git::show_index(file)
        .or_else(|| git::show_head(file))?;

    let old_src = extract_symbol_source(&old_file, file, symbol)?;
    let new_src = extract_symbol_source(&new_file, file, symbol)?;

    if old_src == new_src {
        return None;
    }

    Some(SymbolDelta {
        file: file.to_string(),
        symbol: symbol.to_string(),
        old_source: old_src.clone(),
        new_source: new_src.clone(),
        lines: AgentState::compute_diff(&old_src, &new_src),
    })
}
