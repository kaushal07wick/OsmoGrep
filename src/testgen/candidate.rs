use crate::state::{RiskLevel, TestDecision};

/// Type of test being generated.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TestType {
    Unit,
    Integration,
    Regression,
}

/// What the test is scoped to.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TestTarget {
    Symbol(String),   // function / method / impl
    Module(String),
    File(String),
}

/// Fully-specified test generation plan.
///
/// Contract between:
/// diff analysis → planning → LLM generation.
#[derive(Debug, Clone)]
pub struct TestCandidate {
    /// Stable deterministic identity (UI, logging, retries).
    pub id: String,

    /* identity */
    pub file: String,
    pub symbol: Option<String>,

    /* inherited decision */
    pub decision: TestDecision,
    pub risk: RiskLevel,

    /* planning */
    pub test_type: TestType,
    pub target: TestTarget,

    /* intent */
    pub behavior: String,
    pub failure_mode: String,

    /* grounding */
    pub old_code: Option<String>,
    pub new_code: Option<String>,
}

impl TestCandidate {
    /// Compute a canonical, deterministic ID.
    ///
    /// Properties:
    /// - stable across runs
    /// - human-readable
    /// - safe for logs / UI keys
    /// - no hashing required (yet)
    pub fn compute_id(
        file: &str,
        symbol: &Option<String>,
        decision: &TestDecision,
    ) -> String {
        match symbol {
            Some(sym) => format!("{file}::{sym}::{decision:?}"),
            None => format!("{file}::<file>::{decision:?}"),
        }
    }
}
