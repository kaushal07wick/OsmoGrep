use crate::state::{RiskLevel, TestDecision, DiffAnalysis};
use crate::context::types::TestIntent;


#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TestTarget {
    Symbol(String),
    Module(String),
    File(String),
}


#[derive(Debug, Clone)]
pub struct TestCandidate {
    pub id: String,
    pub diff: DiffAnalysis,
    pub file: String,
    pub symbol: Option<String>,
    pub target: TestTarget,
    pub decision: TestDecision,
    pub risk: RiskLevel,
    pub test_intent: TestIntent,
    pub behavior: String,
    pub old_code: Option<String>,
    pub new_code: Option<String>,
}
