use crate::state::{ChangeSurface, DiffAnalysis, RiskLevel, TestDecision, AgentState};
use crate::testgen::candidate::{TestCandidate, TestType, TestTarget};
use crate::testgen::resolve::{resolve_test, TestResolution};

pub fn generate_test_candidates(
    state: &AgentState,
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

        /* ---------- test intent ---------- */
        let test_type = match d.surface {
            ChangeSurface::Integration | ChangeSurface::State => TestType::Integration,
            _ => TestType::Regression, // ✅ default
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

        let mut candidate = TestCandidate {
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
        };

        /* ---------- 🔍 resolve existing tests ---------- */
        match resolve_test(state, &candidate) {
            TestResolution::Found { file, .. } => {
                // existing test → modify
                candidate.behavior =
                    format!("Update existing test in `{}` to reflect new behavior", file);
            }

            TestResolution::Ambiguous(paths) => {
                // ask user
                candidate.behavior = format!(
                    "Multiple existing tests found:\n{}\nUser input required",
                    paths.join("\n")
                );
            }

            TestResolution::NotFound => {
                // create new test
                candidate.behavior =
                    "Create a new regression test validating the updated behavior".into();
            }
        }

        out.push(candidate);
    }

    out
}

/* ============================================================
   Behavior + failure
   ============================================================ */

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
