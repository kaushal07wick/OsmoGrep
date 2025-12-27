// src/testgen/candidate.rs

use crate::state::{DiffAnalysis, RiskLevel, TestDecision};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TestTarget {
    Symbol(String),
    Module(String),
    File(String),
}

#[derive(Debug, Clone)]
pub struct TestCandidate {
    pub id: String,

    // Source of truth
    pub diff: DiffAnalysis,
    pub file: String,
    pub symbol: Option<String>,
    pub target: TestTarget,

    // Classification
    pub decision: TestDecision,
    pub risk: RiskLevel,

    // Derived content (filled later)
    pub old_code: Option<String>,
    pub new_code: Option<String>,
}
