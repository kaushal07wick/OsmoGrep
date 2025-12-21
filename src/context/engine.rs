// src/context/engine.rs

use crate::testgen::candidate::TestTarget;

use super::types::{
    ContextSlice,
    RepoFacts,
    RepoFactsLite,
    SymbolDef,
    SymbolIndex,
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
        match target {
            TestTarget::Symbol(name) => self.slice_for_symbol(name),
            TestTarget::Module(name) => self.slice_for_module(name),
            TestTarget::File(path) => self.slice_for_file(path),
        }
    }

    /* ---------- symbol ---------- */

    fn slice_for_symbol(&self, name: &str) -> ContextSlice {
        let target = match self.symbols.by_symbol.get(name) {
            Some(s) => s.clone(),
            None => return self.empty_slice(name),
        };

        self.slice_from_file(target)
    }

    /* ---------- module ---------- */

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

    /* ---------- file ---------- */

    fn slice_for_file(&self, file: &str) -> ContextSlice {
        let path = std::path::PathBuf::from(file);

        let target = SymbolDef {
            name: file.to_string(),
            file: path,
            line: 0,
        };

        self.slice_from_file(target)
    }

    /* ---------- shared ---------- */

    fn slice_from_file(&self, target: SymbolDef) -> ContextSlice {
        let mut deps = Vec::new();
        let mut imports = Vec::new();

        if let Some(file) = self.symbols.by_file.get(&target.file) {
            deps = file
                .symbols
                .iter()
                .filter(|s| s.name != target.name)
                .take(5)
                .cloned()
                .collect();

            imports = file.imports.iter().take(5).cloned().collect();
        }

        ContextSlice {
            repo_facts: RepoFactsLite::from(self.facts),
            target,
            deps,
            imports,
        }
    }

    fn empty_slice(&self, name: &str) -> ContextSlice {
        ContextSlice {
            repo_facts: RepoFactsLite::from(self.facts),
            target: SymbolDef {
                name: name.to_string(),
                file: std::path::PathBuf::new(),
                line: 0,
            },
            deps: Vec::new(),
            imports: Vec::new(),
        }
    }
}
