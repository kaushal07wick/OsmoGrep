//! resolve.rs
//!
//! Test resolution engine for Osmogrep.
//!
//! Responsibilities:
//! - Determine whether a test already exists for a given TestCandidate
//! - Classify resolution outcome as Found / Ambiguous / NotFound
//!
//! Guarantees:
//! - Read-only filesystem access
//! - No side effects
//! - Deterministic results
//!
//! Non-responsibilities:
//! - No test generation
//! - No file writes
//! - No UI logic

use crate::detectors::language::Language;
use crate::testgen::candidate::TestCandidate;

/* ============================================================
   Resolution result
   ============================================================ */

#[derive(Debug, Clone)]
pub enum TestResolution {
    Found {
        file: String,
        test_fn: Option<String>,
    },
    Ambiguous(Vec<String>),
    NotFound,
}

/* ============================================================
   Public entrypoint
   ============================================================ */

pub fn resolve_test(
    language: &Language,
    c: &TestCandidate,
) -> TestResolution {
    match language {
        Language::Rust => resolve_rust_test(c),
        Language::Python => resolve_python_test(c),
        _ => TestResolution::NotFound,
    }
}

/* ============================================================
   Rust resolution
   ============================================================ */

fn resolve_rust_test(c: &TestCandidate) -> TestResolution {
    let symbol = match &c.symbol {
        Some(s) => s,
        None => return TestResolution::NotFound,
    };

    let src = match std::fs::read_to_string(&c.file) {
        Ok(s) => s,
        Err(_) => return TestResolution::NotFound,
    };

    if !src.contains("#[cfg(test)]") {
        return TestResolution::NotFound;
    }

    if src.contains(&format!("{}(", symbol)) {
        TestResolution::Found {
            file: c.file.clone(),
            test_fn: None,
        }
    } else {
        TestResolution::NotFound
    }
}

/* ============================================================
   Python resolution
   ============================================================ */

fn resolve_python_test(c: &TestCandidate) -> TestResolution {
    let symbol = match &c.symbol {
        Some(s) => s,
        None => return TestResolution::NotFound,
    };

    let mut hits = Vec::new();
    let roots = ["tests", "test", "testing"];

    for root in roots {
        if !std::path::Path::new(root).exists() {
            continue;
        }

        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("py") {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            if content.contains(&format!("{}(", symbol)) {
                hits.push(path.display().to_string());
            }
        }
    }

    match hits.len() {
        0 => TestResolution::NotFound,
        1 => TestResolution::Found {
            file: hits[0].clone(),
            test_fn: None,
        },
        _ => TestResolution::Ambiguous(hits),
    }
}
