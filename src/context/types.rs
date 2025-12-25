// src/context/types.rs
//
// Shared data model for the repository context engine.


use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::detectors::{
    framework::TestFramework,
    language::Language,
};


#[derive(Debug, Clone)]
pub struct RepoFacts {
    pub languages: Vec<Language>,
    pub test_frameworks: Vec<TestFramework>,
    pub forbidden_deps: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RepoFactsLite {
    pub languages: Vec<String>,
    pub test_frameworks: Vec<String>,
    pub forbidden_deps: Vec<String>,
}

impl RepoFactsLite {
    pub fn from(facts: &RepoFacts) -> Self {
        Self {
            languages: facts.languages.iter().map(|l| l.to_string()).collect(),
            test_frameworks: facts
                .test_frameworks
                .iter()
                .map(|t| t.to_string())
                .collect(),
            forbidden_deps: facts.forbidden_deps.clone(),
        }
    }
}


#[derive(Debug, Clone)]
pub struct DiffTarget {
    /// File containing the change (from git diff)
    pub file: PathBuf,

    /// Symbol name if detected (from diff analyzer)
    pub symbol: Option<String>,
}


#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
    /// File → symbols/imports (shallow, best-effort)
    pub by_file: HashMap<PathBuf, FileSymbols>,
}

#[derive(Debug, Clone)]
pub struct FileSymbols {
    pub symbols: Vec<SymbolDef>,
    pub imports: Vec<Import>,
}

#[derive(Debug, Clone)]
pub struct SymbolDef {
    pub name: String,
    pub file: PathBuf,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub names: Vec<String>,
}


#[derive(Debug, Clone)]
pub enum SymbolResolution {
    /// Exactly one symbol resolved
    Resolved(SymbolDef),

    /// Multiple possible matches (e.g. method name)
    Ambiguous(Vec<SymbolDef>),

    /// No matching symbol found in file
    NotFound,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestMatchKind {
    Filename,
    Content,
    None,
}


#[derive(Debug, Clone)]
pub struct TestContext {
    pub existing_tests: Vec<PathBuf>,
    pub recommended_location: PathBuf,
    pub match_kind: TestMatchKind,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionModel {
    Eager,
    Lazy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestIntent {
    Regression,
    Guardrail,
    NewBehavior,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionStyle {
    Exact,
    Approximate,
    Sanity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureMode {
    Panic,
    RuntimeError,
    NaN,
    Inf,
    WrongShape,
}


#[derive(Debug, Clone)]
pub struct ContextSlice {
    pub repo_facts: RepoFactsLite,
    pub diff_target: DiffTarget,

    /// Explicit symbol resolution result
    pub symbol_resolution: SymbolResolution,

    /// Local structure (ENTIRE file, no truncation)
    pub local_symbols: Vec<SymbolDef>,
    pub imports: Vec<Import>,

    pub test_context: TestContext,

    pub execution_model: Option<ExecutionModel>,
    pub test_intent: Option<TestIntent>,
    pub assertion_style: Option<AssertionStyle>,
    pub failure_modes: Vec<FailureMode>,
}


#[derive(Debug, Clone)]
pub enum IndexStatus {
    Indexing,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct IndexHandle {
    pub repo_root: PathBuf,

    pub code_roots: Arc<RwLock<Vec<PathBuf>>>,
    pub test_roots: Arc<RwLock<Vec<PathBuf>>>,

    pub status: Arc<RwLock<IndexStatus>>,
    pub facts: Arc<RwLock<Option<RepoFacts>>>,
    pub symbols: Arc<RwLock<Option<SymbolIndex>>>,
}

impl IndexHandle {
    pub fn new_indexing(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            code_roots: Arc::new(RwLock::new(Vec::new())),
            test_roots: Arc::new(RwLock::new(Vec::new())),
            status: Arc::new(RwLock::new(IndexStatus::Indexing)),
            facts: Arc::new(RwLock::new(None)),
            symbols: Arc::new(RwLock::new(None)),
        }
    }

    pub fn set_facts(&self, facts: RepoFacts) {
        *self.facts.write().unwrap() = Some(facts);
    }

    pub fn set_symbols(&self, symbols: SymbolIndex) {
        *self.symbols.write().unwrap() = Some(symbols);
    }

    pub fn mark_ready(&self) {
        *self.status.write().unwrap() = IndexStatus::Ready;
    }

    pub fn mark_failed(&self, error: impl Into<String>) {
        *self.status.write().unwrap() = IndexStatus::Failed(error.into());
    }
}
