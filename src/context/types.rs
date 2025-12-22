// src/context/types.rs
//
// Shared data model for the repository context engine.
//
// Defines ONLY data structures and trivial helpers.
// NO indexing logic.
// NO traversal.
// NO diffing.
// NO LLM logic.
//
// These types are reused across:
// - repository indexing
// - test context synthesis
// - prompt construction
//
// Agent lifecycle, UI state, logging, cancellation, and execution
// are handled elsewhere (state.rs).
//

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::detectors::{
    framework::TestFramework,
    language::Language,
};

/* ============================================================
   Repository Facts
   ============================================================ */

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

/* ============================================================
   Symbol Index (Code Context)
   ============================================================ */

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

/* ============================================================
   Testing Semantics (Derived, Not Executed)
   ============================================================ */

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

/* ============================================================
   Test Ecosystem Context (CRITICAL)
   ============================================================ */

#[derive(Debug, Clone)]
pub struct TestContext {
    /// Existing test files related to the target (heuristic match).
    pub existing_tests: Vec<PathBuf>,

    /// Known helpers / fixtures / utilities used in tests.
    pub helpers: Vec<String>,

    /// Recommended test file location if a new test is required.
    pub recommended_location: Option<PathBuf>,

    /// What the test should validate against.
    pub oracle: Option<TestOracle>,

    /// Lightweight input/output expectations.
    pub io_contract: Option<IOContract>,
}

#[derive(Debug, Clone)]
pub enum TestOracle {
    /// Compare against another symbol (e.g. reference implementation).
    ReferenceSymbol(String),

    /// Assert an invariant (no crash, finite, shape, etc).
    Invariant(String),

    /// No usable oracle (sanity-only tests).
    None,
}

#[derive(Debug, Clone)]
pub struct IOContract {
    pub input_notes: Vec<String>,
    pub output_notes: Vec<String>,
}

/* ============================================================
   Context Slice (What the LLM Sees)
   ============================================================ */

#[derive(Debug, Clone)]
pub struct ContextSlice {
    /* repository-level constraints */
    pub repo_facts: RepoFactsLite,

    /* target under test */
    pub target: SymbolDef,

    /* local code structure */
    pub deps: Vec<SymbolDef>,
    pub imports: Vec<Import>,

    /* test ecosystem awareness */
    pub test_context: Option<TestContext>,

    /* derived testing semantics (NO logic here) */
    pub execution_model: Option<ExecutionModel>,
    pub test_intent: Option<TestIntent>,
    pub assertion_style: Option<AssertionStyle>,
    pub failure_modes: Vec<FailureMode>,
}

/* ============================================================
   Index Lifecycle
   ============================================================ */

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

/* ============================================================
   Incremental Indexing Support
   ============================================================ */

#[derive(Debug, Clone)]
pub struct FileHash {
    pub path: PathBuf,
    pub hash: u64,
}
