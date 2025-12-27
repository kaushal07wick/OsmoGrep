// src/context/types.rs


use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::detectors::language::Language;

#[derive(Debug, Clone)]
pub struct RepoFacts {
    pub languages: Vec<Language>,
    pub forbidden_deps: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DiffTarget {
    pub file: PathBuf,
    pub symbol: Option<String>,
}



#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
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
    Resolved(SymbolDef),
    Ambiguous(Vec<SymbolDef>),
    NotFound,
}

#[derive(Debug, Clone)]
pub enum IndexStatus {
    Indexing,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct ContextSlice {
    pub repo_facts: RepoFacts,
    pub diff_target: DiffTarget,

    pub symbol_resolution: SymbolResolution,
    pub local_symbols: Vec<SymbolDef>,
    pub imports: Vec<Import>,
}

#[derive(Debug, Clone)]
pub struct IndexHandle {
    pub repo_root: PathBuf,
    pub code_roots: Arc<RwLock<Vec<PathBuf>>>,
    pub status: Arc<RwLock<IndexStatus>>,
    pub facts: Arc<RwLock<Option<RepoFacts>>>,
    pub symbols: Arc<RwLock<Option<SymbolIndex>>>,
}

impl IndexHandle {
    pub fn new_indexing(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            code_roots: Arc::new(RwLock::new(Vec::new())),
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
