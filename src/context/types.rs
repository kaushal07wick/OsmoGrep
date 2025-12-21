// src/context/types.rs
//
// Shared data model for the repository context engine.
//
// This file defines ONLY data structures and trivial helpers.
// No indexing, no traversal, no diffing, no LLM logic.
//
// These types are reused across:
// - repository indexing
// - context slicing
// - prompt construction
//
// Agent lifecycle, UI state, logging, cancellation, and execution
// are intentionally handled elsewhere (state.rs).
//

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::detectors::{
    framework::TestFramework,
    language::Language,
};

/* ================= Repo Facts ================= */

#[derive(Debug, Clone)]
pub struct RepoFacts {
    pub languages: Vec<Language>,
    pub test_frameworks: Vec<TestFramework>,
    pub forbidden_deps: Vec<String>,
}

/* ================= Symbol Index ================= */

#[derive(Debug, Clone)]
pub struct SymbolIndex {
    pub by_file: HashMap<PathBuf, FileSymbols>,
    pub by_symbol: HashMap<String, SymbolDef>,
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

/* ================= Testing Semantics ================= */

/// How values are executed / materialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionModel {
    Eager,
    Lazy,
}

/// What kind of test is intended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestIntent {
    Regression,
    Guardrail,
    NewBehavior,
}

/// What assertion philosophy to prefer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionStyle {
    Exact,        // strict equality
    Approximate,  // tolerances allowed
    Sanity,       // shape / finiteness / no-crash
}

/// Failure modes the test should protect against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureMode {
    Panic,
    RuntimeError,
    NaN,
    Inf,
    WrongShape,
}

/* ================= Context Slice ================= */

#[derive(Debug, Clone)]
pub struct ContextSlice {
    /* repo-level facts */
    pub repo_facts: RepoFactsLite,

    /* target */
    pub target: SymbolDef,

    /* local structure */
    pub deps: Vec<SymbolDef>,
    pub imports: Vec<Import>,

    /* derived testing semantics (NO logic here) */
    pub execution_model: Option<ExecutionModel>,
    pub test_intent: Option<TestIntent>,
    pub assertion_style: Option<AssertionStyle>,
    pub failure_modes: Vec<FailureMode>,
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

/* ================= Index Lifecycle ================= */

#[derive(Debug, Clone)]
pub enum IndexStatus {
    Indexing,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct IndexHandle {
    pub status: Arc<RwLock<IndexStatus>>,
    pub facts: Arc<RwLock<Option<RepoFacts>>>,
    pub symbols: Arc<RwLock<Option<SymbolIndex>>>,
}

impl IndexHandle {
    pub fn new_indexing() -> Self {
        Self {
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

/* ================= Incremental Indexing ================= */

#[derive(Debug, Clone)]
pub struct FileHash {
    pub path: PathBuf,
    pub hash: u64,
}
