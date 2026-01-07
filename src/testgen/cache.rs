// src/testgen/cache.rs
//
// Semantic cache for test generation.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::{Arc};
use crate::testgen::candidate::TestCandidate;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SemanticKey {
    pub file: String,
    pub symbol: Option<String>,
    pub code_hash: String,
}

impl SemanticKey {
    pub fn from_candidate(c: &TestCandidate) -> Self {
        let code = c
            .new_code
            .as_ref()
            .expect("TestCandidate must have new_code");

        Self {
            file: c.file.clone(),
            symbol: c.symbol.clone(),
            code_hash: hash_str(code),
        }
    }

    /// Stable cache key (used everywhere)
    pub fn to_cache_key(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.file.as_bytes());
        if let Some(sym) = &self.symbol {
            h.update(sym.as_bytes());
        }
        h.update(self.code_hash.as_bytes());
        hex::encode(h.finalize())
    }
}

#[derive(Default)]
pub struct SemanticCache {
    // cache_key -> existing test file path
    map: Mutex<HashMap<String, PathBuf>>,
}

impl SemanticCache {
    pub fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }

    /// Returns path to an already-generated test, if any
    pub fn get(&self, key: &str) -> Option<PathBuf> {
        self.map.lock().unwrap().get(key).cloned()
    }

    /// Store path of a passing test
    pub fn insert(&self, key: String, test_path: PathBuf) {
        self.map.lock().unwrap().insert(key, test_path);
    }
}

fn hash_str(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

// Full-suite failure repair cache
#[derive(Debug, Clone)]
pub struct FullSuiteCacheEntry {
    pub test_name: String,
    pub test_path: PathBuf,
    pub last_generated_test: String,
    pub passed: bool,
}

#[derive(Default, Clone)]
pub struct FullSuiteCache {
    map: Arc<Mutex<HashMap<String, FullSuiteCacheEntry>>>,
}

impl FullSuiteCache {
    pub fn new() -> Self {
        Self {
            map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get cached entry if exists
    pub fn get(&self, test_name: &str) -> Option<FullSuiteCacheEntry> {
        self.map.lock().unwrap().get(test_name).cloned()
    }

    /// Insert or update cache entry
    pub fn insert(&self, entry: FullSuiteCacheEntry) {
        self.map
            .lock()
            .unwrap()
            .insert(entry.test_name.clone(), entry);
    }

    /// Remove cache entry (usually when cached test still fails)
    pub fn remove(&self, test_name: &str) {
        self.map.lock().unwrap().remove(test_name);
    }

    /// Clear whole full-suite cache
    pub fn clear(&self) {
        self.map.lock().unwrap().clear();
    }
}
