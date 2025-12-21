// src/context/engine.rs

use crate::testgen::candidate::TestTarget;

use super::types::{
    ContextSlice,
    RepoFacts,
    RepoFactsLite,
    SymbolDef,
    SymbolIndex,
};

use std::path::PathBuf;

pub struct ContextEngine<'a> {
    facts: &'a RepoFacts,
    symbols: &'a SymbolIndex,
}

impl<'a> ContextEngine<'a> {
    pub fn new(facts: &'a RepoFacts, symbols: &'a SymbolIndex) -> Self {
        Self { facts, symbols }
    }

    pub fn slice_for(&self, target: &TestTarget) -> ContextSlice {
        match target {
            TestTarget::Symbol(name) => self.slice_for_symbol(name),
            TestTarget::Module(name) => self.slice_for_module(name),
            TestTarget::File(path) => self.slice_for_file(path),
        }
    }

    /* ================= Symbol ================= */

    fn slice_for_symbol(&self, name: &str) -> ContextSlice {
        let target = match self.symbols.by_symbol.get(name) {
            Some(s) => s.clone(),
            None => return self.empty_slice(name),
        };

        self.slice_from_file(target)
    }

    /* ================= Module ================= */

    fn slice_for_module(&self, module: &str) -> ContextSlice {
        let file = self.symbols.by_file.keys().find(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                == Some(module)
        });

        let target = match file {
            Some(path) => SymbolDef {
                name: module.to_string(),
                file: path.clone(),
                line: 0,
            },
            None => return self.empty_slice(module),
        };

        self.slice_from_file(target)
    }

    /* ================= File ================= */

    fn slice_for_file(&self, file: &str) -> ContextSlice {
        let target = SymbolDef {
            name: file.to_string(),
            file: PathBuf::from(file),
            line: 0,
        };

        self.slice_from_file(target)
    }

    /* ================= Core ================= */

    fn slice_from_file(&self, target: SymbolDef) -> ContextSlice {
        let mut deps = Vec::new();
        let mut imports = Vec::new();

        if let Some(file) = self.symbols.by_file.get(&target.file) {
            deps = file
                .symbols
                .iter()
                .filter(|s| s.name != target.name)
                .take(6)
                .cloned()
                .collect();

            imports = file
                .imports
                .iter()
                .filter(|i| !i.module.is_empty())
                .take(6)
                .cloned()
                .collect();
        }

        ContextSlice {
            repo_facts: RepoFactsLite::from(self.facts),
            target,
            deps,
            imports,

            // ⬇️ DO NOT GUESS — fill only what is defined
            execution_model: None,
            assertion_style: None,
            failure_modes: Vec::new(),
            test_intent: None,
        }
    }

    fn empty_slice(&self, name: &str) -> ContextSlice {
        ContextSlice {
            repo_facts: RepoFactsLite::from(self.facts),
            target: SymbolDef {
                name: name.to_string(),
                file: PathBuf::new(),
                line: 0,
            },
            deps: Vec::new(),
            imports: Vec::new(),

            execution_model: None,
            assertion_style: None,
            failure_modes: Vec::new(),
            test_intent: None,
        }
    }
}
