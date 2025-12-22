//! resolve.rs
//!
//! Test resolution engine for Osmogrep.
//!
//! Responsibilities:
//! - Determine whether a test already exists for a given TestCandidate
//! - Classify resolution outcome as Found / Ambiguous / NotFound
//!
//! Guarantees:
//! - Read-only access
//! - Deterministic results
//!
//! Non-responsibilities:
//! - No test generation
//! - No filesystem crawling
//! - No AST parsing
//! - No UI logic

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

    match resolve_from_context(c, ctx) {
        Some(res) => res,
        None => TestResolution::NotFound,
    }
}

/* ============================================================
   Core resolution logic
   ============================================================ */

fn resolve_from_context(
    c: &TestCandidate,
    ctx: &TestContext,
) -> Option<TestResolution> {
    let symbol = c.symbol.as_ref()?;

    let mut hits = Vec::new();

    for path in &ctx.existing_tests {
        let content = std::fs::read_to_string(path).ok()?;

        // pytest / unittest style
        let fn_name = format!("test_{}", symbol);

        if content.contains(&fn_name) {
            return Some(TestResolution::Found {
                file: path.display().to_string(),
                test_fn: Some(fn_name),
            });
        }

        // fallback: symbol referenced
        if content.contains(symbol) {
            hits.push(path.display().to_string());
        }
    }

    match hits.len() {
        0 => None,
        1 => Some(TestResolution::Found {
            file: hits[0].clone(),
            test_fn: None,
        }),
        _ => Some(TestResolution::Ambiguous(hits)),
    }
}
