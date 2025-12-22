//! resolve.rs
//!
//! Test resolution engine.
//!
//! Responsibilities:
//! - Detect existing tests for a TestCandidate
//! - Classify as Found / Ambiguous / NotFound
//!
//! Guarantees:
//! - Read-only
//! - Deterministic
//! - No crawling beyond provided TestContext

use crate::testgen::candidate::TestCandidate;
use crate::context::types::TestContext;

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
    c: &TestCandidate,
    ctx: Option<&TestContext>,
) -> TestResolution {
    let ctx = match ctx {
        Some(c) => c,
        None => return TestResolution::NotFound,
    };

    resolve_from_context(c, ctx)
}

/* ============================================================
   Core resolution logic
   ============================================================ */

fn resolve_from_context(
    c: &TestCandidate,
    ctx: &TestContext,
) -> TestResolution {
    let symbol = match &c.symbol {
        Some(s) => s,
        None => return TestResolution::NotFound,
    };

    let test_fn = format!("test_{}", symbol);
    let mut symbol_hits = Vec::new();

    for path in &ctx.existing_tests {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue, // ✅ never abort resolution
        };

        // Strong signal: explicit test function
        if content.contains(&test_fn) {
            return TestResolution::Found {
                file: path.display().to_string(),
                test_fn: Some(test_fn),
            };
        }

        // Weak signal: symbol referenced
        if content.contains(symbol) {
            symbol_hits.push(path.display().to_string());
        }
    }

    match symbol_hits.len() {
        0 => TestResolution::NotFound,
        1 => TestResolution::Found {
            file: symbol_hits[0].clone(),
            test_fn: None,
        },
        _ => TestResolution::Ambiguous(symbol_hits),
    }
}
