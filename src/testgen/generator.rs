use crate::state::{ChangeSurface, DiffAnalysis, RiskLevel, TestDecision};
use crate::testgen::candidate::{TestCandidate, TestType, TestTarget};

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

        // 🔑 Regression is the default intent
        let test_type = match d.surface {
            ChangeSurface::Integration | ChangeSurface::State => TestType::Integration,
            _ => TestType::Regression,
        };

        let target = match &d.symbol {
            Some(sym) => TestTarget::Symbol(sym.clone()),
            None => TestTarget::File(d.file.clone()),
        };

        let (old_code, new_code) = d
            .delta
            .as_ref()
            .map(|delta| {
                (
                    Some(delta.old_source.clone()),
                    Some(delta.new_source.clone()),
                )
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
            "Existing behavior remains correct across all logical branches".into(),

        ChangeSurface::Contract =>
            "Public API continues to behave as before for valid inputs".into(),

        ChangeSurface::ErrorPath =>
            "Error handling behavior remains stable and predictable".into(),

        ChangeSurface::State =>
            "State changes do not break previously valid flows".into(),

        ChangeSurface::Integration =>
            "End-to-end behavior remains correct when components interact".into(),

        ChangeSurface::Observability =>
            "Logging and metrics changes do not affect functional behavior".into(),

        ChangeSurface::Cosmetic =>
            "Non-functional changes do not affect runtime behavior".into(),

        _ =>
            "Previously working behavior continues to work as expected".into(),
    }
}


fn failure_statement(d: &DiffAnalysis) -> String {
    match d.risk {
        RiskLevel::High =>
            "Existing functionality breaks or causes runtime failures".into(),

        RiskLevel::Medium =>
            "Behavior changes unexpectedly under certain conditions".into(),

        RiskLevel::Low =>
            "Minor regression or unintended side effect".into(),
    }
}
