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
            "Behavior remains correct across all logical branches".into(),

        ChangeSurface::Contract =>
            "Public API behavior remains backward compatible".into(),

        ChangeSurface::ErrorPath =>
            "Errors are raised and handled predictably".into(),

        ChangeSurface::State =>
            "State transitions remain valid across operations".into(),

        ChangeSurface::Integration =>
            "End-to-end behavior remains correct across components".into(),

        ChangeSurface::Observability =>
            "Instrumentation changes do not affect runtime behavior".into(),

        ChangeSurface::Cosmetic =>
            "Non-functional changes do not affect execution".into(),

        ChangeSurface::PureLogic =>
            "Previously valid behavior continues to work as expected".into(),
    }
}


fn failure_statement(d: &DiffAnalysis) -> String {
    match d.surface {
        ChangeSurface::Contract =>
            "Callers may observe breaking API behavior".into(),

        ChangeSurface::ErrorPath =>
            "Unhandled errors may surface at runtime".into(),

        ChangeSurface::State =>
            "Invalid state transitions may break flows".into(),

        ChangeSurface::Integration =>
            "Cross-component interaction may fail".into(),

        _ =>
            "Unexpected regression in existing behavior".into(),
    }
}


fn infer_risk(d: &DiffAnalysis) -> RiskLevel {
    match d.surface {
        ChangeSurface::Contract
        | ChangeSurface::State
        | ChangeSurface::Integration =>
            RiskLevel::High,

        ChangeSurface::Branching
        | ChangeSurface::ErrorPath =>
            RiskLevel::Medium,

        ChangeSurface::Observability
        | ChangeSurface::Cosmetic
        | ChangeSurface::PureLogic =>
            RiskLevel::Low,
    }
}
