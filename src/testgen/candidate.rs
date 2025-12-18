use crate::state::{RiskLevel, TestDecision};

#[derive(Debug, Clone)]
pub enum TestType {
    Unit,
    Integration,
    Regression,
}

#[derive(Debug, Clone)]
pub enum TestTarget {
    Symbol(String),     // function / method / impl
    Module(String),
    File(String),
}

#[derive(Debug, Clone)]
pub struct TestCandidate {
    /* identity (references state.rs) */
    pub file: String,
    pub symbol: Option<String>,

    /* inherited decision */
    pub decision: TestDecision,
    pub risk: RiskLevel,

    /* planning */
    pub test_type: TestType,
    pub target: TestTarget,

    /* intent */
    pub behavior: String,       // what must hold true
    pub failure_mode: String,   // what breaks if wrong

    /* grounding (from SymbolDelta) */
    pub old_code: Option<String>,
    pub new_code: Option<String>,
}
