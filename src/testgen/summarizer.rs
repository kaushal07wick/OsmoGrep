//! summarizer.rs
//!
//! Semantic interpretation layer for test generation.
//!
//! Converts raw DiffAnalysis + SymbolDelta into
//! human-meaningful test intent.


use crate::state::{
    ChangeSurface,
    DiffAnalysis,
    RiskLevel,
};

#[derive(Debug, Clone)]
pub struct SemanticSummary {
    pub behavior: String,
    pub failure_mode: String,
    pub risk: RiskLevel,
}

pub fn summarize(diff: &DiffAnalysis) -> SemanticSummary {
    let behavior = behavior_statement(diff);
    let failure_mode = failure_statement(diff);
    let risk = infer_risk(diff);

    SemanticSummary {
        behavior,
        failure_mode,
        risk,
    }
}

fn behavior_statement(d: &DiffAnalysis) -> String {
    match d.surface {
        ChangeSurface::Branching =>
            "All control-flow paths are expected to produce consistent outputs for the same inputs".into(),

        ChangeSurface::Contract =>
            "The external API contract is preserved, including inputs, outputs, and side-effects".into(),

        ChangeSurface::ErrorPath =>
            "Error conditions produce stable, deterministic, and documented failure modes".into(),

        ChangeSurface::State =>
            "Internal state transitions remain consistent and preserve invariants across operations".into(),

        ChangeSurface::Cosmetic =>
            "No functional behavior changes; execution semantics remain identical".into(),

        ChangeSurface::PureLogic =>
            "Logical expressions maintain equivalent evaluation results for all valid inputs".into(),

        ChangeSurface::Unknown =>
            "Behavioral impact cannot be classified; full-suite validation required".into(),
    }
}
fn failure_statement(d: &DiffAnalysis) -> String {
    match d.surface {
        ChangeSurface::Contract =>
            "External callers may receive different outputs, experience signature incompatibility, or encounter altered side-effects".into(),

        ChangeSurface::ErrorPath =>
            "Execution may raise unintended exceptions or fail to raise expected errors under invalid conditions".into(),

        ChangeSurface::State =>
            "State corruption or inconsistent transitions may occur, leading to nondeterministic behavior".into(),

        _ =>
            "Core logic may produce incorrect results or regress from previously validated behavior".into(),
    }
}


fn infer_risk(d: &DiffAnalysis) -> RiskLevel {
    match d.surface {
        ChangeSurface::Contract
        | ChangeSurface::State =>
            RiskLevel::High,

        ChangeSurface::Branching
        | ChangeSurface::ErrorPath =>
            RiskLevel::Medium,

        | ChangeSurface::Cosmetic
        | ChangeSurface::PureLogic 
        | ChangeSurface::Unknown =>
            RiskLevel::Low,
    }
}
