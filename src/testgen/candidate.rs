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

impl TestCandidate {
    pub fn from_test_failure(
        file: String,
        test_name: String,
    ) -> Self {
        TestCandidate {
            id: format!("failure::{}::{}", file, test_name),

            diff: DiffAnalysis {
                file: file.clone(),
                symbol: None,
                surface: crate::state::ChangeSurface::Unknown,
                delta: None,
                summary: None,
            },

            file: file.clone(),

            // symbol intentionally None — resolved later via context lookup
            symbol: None,

            target: TestTarget::File(file),

            decision: TestDecision::Yes,
            risk: RiskLevel::High,

            old_code: None,
            new_code: None,
        }
    }
}
