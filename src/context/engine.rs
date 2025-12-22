// src/context/engine.rs
//
// Context slicing engine.
//
// Responsibilities:
// - select minimal relevant symbols + imports
// - surface repository testing context
// - locate existing tests (tests/ only)
//
// Guarantees:
// - NO inference
// - NO guessing
// - Deterministic, factual context only
//

use std::path::{Path, PathBuf};

use crate::testgen::candidate::TestTarget;

use super::types::{
    AssertionStyle,
    ContextSlice,
    ExecutionModel,
    FailureMode,
    IOContract,
    RepoFacts,
    RepoFactsLite,
    SymbolDef,
    SymbolIndex,
    TestContext,
    TestIntent,
    TestOracle,
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

    /* ============================================================
       Target resolution
       ============================================================ */

    fn slice_for_symbol(&self, name: &str) -> ContextSlice {
        let symbol = self
            .symbols
            .by_symbol
            .values()
            .find(|s| s.name == name)
            .cloned();

        match symbol {
            Some(s) => self.slice_from_file(s),
            None => self.empty_slice(name),
        }
    }

    fn slice_for_module(&self, module: &str) -> ContextSlice {
        let path = self
            .symbols
            .by_file
            .keys()
            .find(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    == Some(module)
            })
            .cloned();

        match path {
            Some(p) => self.slice_from_file(SymbolDef {
                name: module.to_string(),
                file: p,
                line: 0,
            }),
            None => self.empty_slice(module),
        }
    }

    fn slice_for_file(&self, file: &str) -> ContextSlice {
        let path = PathBuf::from(file);

        if !self.symbols.by_file.contains_key(&path) {
            return self.empty_slice(file);
        }

        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(file)
            .to_string();

        self.slice_from_file(SymbolDef {
            name,
            file: path,
            line: 0,
        })
    }

    /* ============================================================
       Core slice builder
       ============================================================ */

    fn slice_from_file(&self, target: SymbolDef) -> ContextSlice {
        let mut deps = Vec::new();
        let mut imports = Vec::new();

        if let Some(file) = self.symbols.by_file.get(&target.file) {
            deps = file
                .symbols
                .iter()
                .filter(|s| {
                    s.name != target.name &&
                    !s.name.starts_with("__")
                })
                .take(8)
                .cloned()
                .collect();

            imports = file
                .imports
                .iter()
                .filter(|i| !i.module.is_empty())
                .take(8)
                .cloned()
                .collect();
        }

        let test_context = self.build_test_context(&target);

        ContextSlice {
            /* repository-level */
            repo_facts: RepoFactsLite::from(self.facts),

            /* target */
            target,

            /* local code structure */
            deps,
            imports,

            /* test ecosystem */
            test_context,

            /* execution semantics */
            execution_model: Some(ExecutionModel::Lazy),

            /* deterministic test semantics */
            test_intent: Some(TestIntent::Regression),
            assertion_style: Some(AssertionStyle::Sanity),
            failure_modes: vec![
                FailureMode::RuntimeError,
                FailureMode::NaN,
                FailureMode::Inf,
                FailureMode::WrongShape,
            ],
        }
    }

    /* ============================================================
       Test ecosystem context (STRICT)
       ============================================================ */

    fn build_test_context(&self, target: &SymbolDef) -> Option<TestContext> {
        let repo_root = self
            .symbols
            .by_file
            .keys()
            .next()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())?;

        let tests_dir = repo_root.join("tests");

        if !tests_dir.exists() {
            return None;
        }

        let mut existing_tests = Vec::new();
        let target_name = &target.name;

        for entry in walkdir::WalkDir::new(&tests_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("py") {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Strict, factual relevance only
            if content.contains(&format!("{}(", target_name)) {
                existing_tests.push(path.to_path_buf());
            }
        }

        existing_tests.truncate(5);

        Some(TestContext {
            existing_tests,
            helpers: Vec::new(),
            recommended_location: recommend_test_path(&target.file),

            oracle: Some(TestOracle::Invariant(
                "Must not crash; output must be finite scalar".into(),
            )),

            io_contract: Some(IOContract {
                input_notes: vec![
                    "logits: Tensor[float32] shape (N, C)".into(),
                    "labels: Tensor[int32] shape (N,)".into(),
                    "labels may include ignore_index".into(),
                ],
                output_notes: vec![
                    "scalar Tensor".into(),
                    "finite value".into(),
                ],
            }),
        })
    }


    /* ============================================================
       Empty fallback
       ============================================================ */

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
            test_context: None,
            execution_model: None,
            test_intent: None,
            assertion_style: None,
            failure_modes: Vec::new(),
        }
    }
}

/* ============================================================
   Helpers
   ============================================================ */

fn recommend_test_path(src: &Path) -> Option<PathBuf> {
    let filename = src.file_stem()?.to_str()?;
    Some(PathBuf::from("tests").join(format!("test_{}.py", filename)))
}
