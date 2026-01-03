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
