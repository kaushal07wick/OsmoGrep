use crate::state::{RiskLevel, TestDecision};
use crate::context::types::{
    AssertionStyle,
    FailureMode,
    TestIntent,
    TestOracle,
};

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
    Symbol(String),   // function / method
    Module(String),
    File(String),
}

/// Fully-specified test generation plan.
///
/// This is the *single contract* between:
/// context slicing → planning → LLM generation.
#[derive(Debug, Clone)]
pub struct TestCandidate {
    /* ======================================================
       Stable identity
       ====================================================== */

    /// Deterministic identity (UI, logs, retries).
    pub id: String,

    /* ======================================================
       Target identity
       ====================================================== */

    /// Source file under test.
    pub file: String,

    /// Symbol under test (if any).
    pub symbol: Option<String>,

    /// Scope of the test.
    pub target: TestTarget,

    /* ======================================================
       Decision & risk (from diff analysis)
       ====================================================== */

    /// Whether a test should be generated.
    pub decision: TestDecision,

    /// Risk level of the change.
    pub risk: RiskLevel,

    /* ======================================================
       Test classification
       ====================================================== */

    /// Structural type of test.
    pub test_type: TestType,

    /// Semantic intent of the test.
    pub test_intent: TestIntent,

    /* ======================================================
       Behavioral intent (human-readable, LLM-facing)
       ====================================================== */

    /// What behavior is being validated.
    ///
    /// Example:
    /// - "ignore_index values must not affect loss computation"
    /// - "tanh must match reference implementation"
    pub behavior: String,

    /* ======================================================
       Assertion & failure semantics
       ====================================================== */

    /// Preferred assertion strategy.
    pub assertion_style: AssertionStyle,

    /// Failure modes the test must guard against.
    pub failure_modes: Vec<FailureMode>,

    /// What the test should validate against.
    pub oracle: TestOracle,

    /* ======================================================
       Grounding context (optional but powerful)
       ====================================================== */

    /// Code before the change (if applicable).
    pub old_code: Option<String>,

    /// Code after the change (if applicable).
    pub new_code: Option<String>,
}

impl TestCandidate {
    /* ======================================================
       Identity helpers
       ====================================================== */

    /// Compute a canonical, deterministic ID.
    ///
    /// Properties:
    /// - stable across runs
    /// - human-readable
    /// - safe for logs / UI keys
    pub fn compute_id(
        file: &str,
        symbol: &Option<String>,
        intent: &TestIntent,
    ) -> String {
        match symbol {
            Some(sym) => format!("{file}::{sym}::{intent:?}"),
            None => format!("{file}::<file>::{intent:?}"),
        }
    }
}
