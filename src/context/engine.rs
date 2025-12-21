// src/context/engine.rs
//
// Query layer over indexed repository context.
//
// This module:
// - reads RepoFacts and SymbolIndex
// - builds ContextSlice for a given TestTarget
//
// It is immutable, deterministic, and allocation-light.
//

use std::path::PathBuf;

use crate::testgen::candidate::TestTarget;

use super::types::{
    RepoFacts,
    SymbolIndex,
    ContextSlice,
    RepoFactsLite,
    SymbolDef,
};

pub struct ContextEngine<'a> {
    facts: &'a RepoFacts,
    symbols: &'a SymbolIndex,
}

impl<'a> ContextEngine<'a> {
    pub fn new(facts: &'a RepoFacts, symbols: &'a SymbolIndex) -> Self {
        Self { facts, symbols }
    }

    pub fn slice_for(&self, target: &TestTarget) -> ContextSlice {
        ContextSlice::build(self.facts, self.symbols, target)
    }
}

impl ContextSlice {
    pub fn build(
        facts: &RepoFacts,
        symbols: &SymbolIndex,
        target: &TestTarget,
    ) -> Self {
        let target_def = match target {
            // -------- Symbol --------
            TestTarget::Symbol(name) => {
                match symbols.by_symbol.get(name) {
                    Some(sym) => sym.clone(),
                    None => return ContextSlice::empty(facts, name),
                }
            }

            // -------- Module --------
            TestTarget::Module(module) => {
                let file = symbols.by_file.keys().find(|p| {
                    p.file_stem().and_then(|s| s.to_str()) == Some(module)
                });

                match file {
                    Some(path) => SymbolDef {
                        name: module.clone(),
                        file: path.clone(),
                        line: 0,
                    },
                    None => return ContextSlice::empty(facts, module),
                }
            }

            // -------- File --------
            TestTarget::File(file) => {
                SymbolDef {
                    name: file.clone(),
                    file: PathBuf::from(file),
                    line: 0,
                }
            }
        };

        let mut deps = Vec::new();
        let mut imports = Vec::new();

        if let Some(file) = symbols.by_file.get(&target_def.file) {
            deps = file
                .symbols
                .iter()
                .filter(|s| s.name != target_def.name)
                .take(5)
                .cloned()
                .collect();

            imports = file.imports.iter().take(5).cloned().collect();
        }

        ContextSlice {
            repo_facts: RepoFactsLite::from(facts),
            target: target_def,
            deps,
            imports,
        }
    }

    fn empty(facts: &RepoFacts, name: &str) -> Self {
        Self {
            repo_facts: RepoFactsLite::from(facts),
            target: SymbolDef {
                name: name.to_string(),
                file: PathBuf::new(),
                line: 0,
            },
            deps: Vec::new(),
            imports: Vec::new(),
        }
    }
}
