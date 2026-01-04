// src/context/types.rs
//
// Canonical context data types.
// Pure, immutable, snapshot-oriented.

use std::path::PathBuf;
#[derive(Clone, Copy)]
pub enum LanguageKind {
    Python,
    Rust,
}

#[derive(Debug, Clone)]
pub struct FileContext {
    pub path: PathBuf,
    pub symbols: Vec<SymbolDef>,
    pub imports: Vec<Import>,
    pub symbol_resolution: SymbolResolution,
}

#[derive(Debug, Clone)]
pub struct ContextSnapshot {
    pub files: Vec<FileContext>,
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
    Resolved(SymbolDef),
    Ambiguous(Vec<SymbolDef>),
    NotFound,
}

#[derive(Debug, Clone)]
pub struct TestContextSnapshot {
    pub exists: bool,

    // Where tests live
    pub test_roots: Vec<PathBuf>,

    // pytest / unittest / rust / unknown
    pub framework: Option<TestFramework>,

    // equivalence / example / property / edge
    pub style: Option<TestStyle>,

    // Known helpers (names only)
    pub helpers: Vec<String>,

    // Reference libs used in tests (torch, jax, numpy, etc)
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestFramework {
    Pytest,
    Unittest,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStyle {
    Equivalence,
    Example,
    Property,
    Edge,
}

#[derive(Debug, Clone)]
pub struct FullContextSnapshot {
    pub code: ContextSnapshot,
    pub tests: TestContextSnapshot,
}
