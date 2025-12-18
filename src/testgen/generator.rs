use crate::logger;
use crate::state::{ChangeSurface, DiffAnalysis, LogLevel, RiskLevel, TestDecision};
use crate::testgen::candidate::{TestCandidate, TestType, TestTarget};
use crate::logger::log;
use crate::state::AgentState;

pub fn generate_test_candidates(
    diffs: &[DiffAnalysis],
) -> Vec<TestCandidate> {
    let mut out = Vec::new();

    for d in diffs {
        if matches!(d.test_required, TestDecision::No) {
            continue;
        }
        if matches!(d.test_required, TestDecision::Conditional) && d.delta.is_none() {
            continue;
        }

        let test_type = match d.surface {
            ChangeSurface::Integration | ChangeSurface::State => TestType::Integration,
            _ => TestType::Unit,
        };

        let target = match &d.symbol {
            Some(sym) => TestTarget::Symbol(sym.clone()),
            None => TestTarget::File(d.file.clone()),
        };
        let (old_code, new_code) = d
            .delta
            .as_ref()
            .map(|delta| {
                (Some(delta.old_source.clone()), Some(delta.new_source.clone()))
            })
            .unwrap_or((None, None));

        let behavior = behavior_statement(d);
        let failure = failure_statement(d);

        out.push(TestCandidate {
            file: d.file.clone(),
            symbol: d.symbol.clone(),
            decision: d.test_required.clone(),
            risk: d.risk.clone(),
            test_type,
            target,
            behavior,
            failure_mode: failure,
            old_code,
            new_code,
        });
    }

    out
}


fn behavior_statement(d: &DiffAnalysis) -> String {
    match d.surface {
        ChangeSurface::Branching =>
            "All logical branches produce correct outcomes".into(),

        ChangeSurface::Contract =>
            "Public API contract remains valid for valid inputs".into(),

        ChangeSurface::ErrorPath =>
            "Errors are handled without panics or silent failures".into(),

        ChangeSurface::State =>
            "State transitions remain consistent and correct".into(),

        ChangeSurface::Integration =>
            "Components interact correctly under real conditions".into(),

        _ =>
            "Modified behavior remains correct".into(),
    }
}

fn failure_statement(d: &DiffAnalysis) -> String {
    match d.risk {
        RiskLevel::High =>
            "Critical behavior regression or runtime failure".into(),
        RiskLevel::Medium =>
            "Incorrect logic under specific conditions".into(),
        RiskLevel::Low =>
            "Minor or cosmetic regression".into(),
    }
}
