
// OSMOGREP TEST
```
mod tests;

#[cfg(test)]
pub mod symbol_delta {
    use super::*;

    #[test]
    fn compute_symbol_delta_base_branch() {
        let baseline = DiffBaseline::BaseBranch;
        let base_commit = git::base_commit("master");
        let old_file = git::show_file_at(&base_commit, "src/detectors/ast/symboldelta.rs");
        let new_file = git::show_index("src/detectors/ast/symboldelta.rs");

        assert_eq!(compute_symbol_delta(base_branch, &baseline_commit, "src/detectors/ast/symboldelta.rs", "symbol"), None);
    }

    #[test]
    fn compute_symbol_delta_staged() {
        let baseline = DiffBaseline::Staged;
        let base_commit = git::base_commit("master");
        let old_file = git::show_head("src/detectors/ast/symboldelta.rs");
        let new_file = git::show_index("src/detectors/ast/symboldelta.rs");

        assert_eq!(compute_symbol_delta(baseline, &baseline_commit, "src/detectors/ast/symboldelta.rs", "symbol"), None);
    }
}
```
