//! generator.rs
//!
//! Test candidate generation pipeline.

use crate::state::{ChangeSurface, DiffAnalysis, TestDecision};
use crate::testgen::candidate::{TestCandidate, TestType, TestTarget};
use crate::testgen::resolve::TestResolution;
use crate::testgen::summarizer;

pub fn generate_test_candidates(
    diffs: &[DiffAnalysis],
    resolve: impl Fn(&TestCandidate) -> TestResolution,
) -> Vec<TestCandidate> {
    let mut out = Vec::new();

    for d in diffs {
        // Skip if no actual code delta
        let delta = match &d.delta {
            Some(d) => d,
            None => continue,
        };

        // Semantic interpretation happens HERE
        let semantic = summarizer::summarize(d);

        // Decide test type
        let test_type = match d.surface {
            ChangeSurface::Integration | ChangeSurface::State =>
                TestType::Integration,
            _ =>
                TestType::Regression,
        };

        let target = match &d.symbol {
            Some(sym) => TestTarget::Symbol(sym.clone()),
            None => TestTarget::File(d.file.clone()),
        };

        let mut candidate = TestCandidate {
            id: String::new(),

            file: d.file.clone(),
            symbol: d.symbol.clone(),

            decision: TestDecision::Yes, // now owned by testgen
            risk: semantic.risk,

            test_type,
            target,

            behavior: semantic.behavior,
            failure_mode: semantic.failure_mode,

            old_code: Some(delta.old_source.clone()),
            new_code: Some(delta.new_source.clone()),
        };

        // Resolution against existing tests
        match resolve(&candidate) {
            TestResolution::Found { file, .. } => {
                candidate.behavior =
                    format!("Update existing test `{}` to reflect new behavior", file);
            }
            TestResolution::Ambiguous(paths) => {
                candidate.behavior = format!(
                    "Multiple candidate tests found:\n{}\nUser input required",
                    paths.join("\n")
                );
            }
            TestResolution::NotFound => {}
        }

        candidate.id = TestCandidate::compute_id(
            &candidate.file,
            &candidate.symbol,
            &candidate.decision,
        );

        out.push(candidate);
    }

    out
}
