use crate::state::{RiskLevel, TestDecision, DiffAnalysis};
use crate::context::types::TestIntent;

/* ============================================================
   Test Target (LEGACY, will be removed later)
   ============================================================ */

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TestTarget {
    Symbol(String),
    Module(String),
    File(String),
}

/* ============================================================
   Test Candidate (DIFF-FIRST, AUTHORITATIVE)
   ============================================================ */

#[derive(Debug, Clone)]
pub struct TestCandidate {
    /* --------------------------------------------------------
       Stable identity
       -------------------------------------------------------- */

    pub id: String,

    /* --------------------------------------------------------
       AUTHORITATIVE DIFF (NON-OPTIONAL)
       -------------------------------------------------------- */

    /// The exact diff this test is derived from.
    /// This is the SINGLE source of truth.
    pub diff: DiffAnalysis,

    /* --------------------------------------------------------
       Target identity (DERIVED, NON-AUTHORITATIVE)
       -------------------------------------------------------- */

    pub file: String,
    pub symbol: Option<String>,
    pub target: TestTarget,

    /* --------------------------------------------------------
       Decision & risk
       -------------------------------------------------------- */

    pub decision: TestDecision,
    pub risk: RiskLevel,
    pub test_intent: TestIntent,

    /* --------------------------------------------------------
       Behavioral intent
       -------------------------------------------------------- */

    pub behavior: String,

    /* --------------------------------------------------------
       Grounding (symbol delta)
       -------------------------------------------------------- */

    pub old_code: Option<String>,
    pub new_code: Option<String>,
}
