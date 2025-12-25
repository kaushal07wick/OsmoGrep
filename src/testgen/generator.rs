//! generator.rs
//!
//! Test candidate generation pipeline.


use crate::state::{ChangeSurface, DiffAnalysis, TestDecision};
use crate::testgen::candidate::TestCandidate;
use crate::testgen::summarizer;
use crate::context::types::TestIntent;


fn normalize_source(file: &str, src: &str) -> String {
    if file.ends_with(".rs") {
        normalize_rust(src)
    } else if file.ends_with(".py") {
        normalize_python(src)
    } else {
        src.trim().to_string()
    }
}

fn normalize_rust(src: &str) -> String {
    src.lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("///")
        })
        .map(|l| l.trim())
        .collect::<String>()
}

fn normalize_python(src: &str) -> String {
    src.lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with('#')
                && !t.starts_with("\"\"\"")
                && !t.starts_with("'''")
        })
        .map(|l| l.trim())
        .collect::<String>()
}

fn is_test_worthy(d: &DiffAnalysis) -> bool {
    let delta = match &d.delta {
        Some(d) => d,
        None => return false,
    };

    let old_norm = normalize_source(&d.file, &delta.old_source);
    let new_norm = normalize_source(&d.file, &delta.new_source);

    if old_norm == new_norm {
        return false;
    }

    // Ignore tiny cosmetic edits
    if old_norm.len().abs_diff(new_norm.len()) < 10 {
        return false;
    }

    true
}

fn decide_test(d: &DiffAnalysis) -> TestDecision {
    match d.surface {
        ChangeSurface::Contract
        | ChangeSurface::ErrorPath
        | ChangeSurface::Branching
        | ChangeSurface::State
        | ChangeSurface::Integration =>
            TestDecision::Yes,

        ChangeSurface::PureLogic =>
            TestDecision::Conditional,

        _ =>
            TestDecision::No,
    }
}


fn priority(d: &DiffAnalysis) -> u8 {
    let mut score = match d.surface {
        ChangeSurface::Contract => 40,
        ChangeSurface::ErrorPath => 35,
        ChangeSurface::Branching => 30,
        ChangeSurface::State => 30,
        ChangeSurface::Integration => 30,
        ChangeSurface::PureLogic => 20,
        _ => 0,
    };

    if d.symbol.is_some() {
        score += 10;
    }

    score
}


pub fn generate_test_candidates(
    diffs: &[DiffAnalysis],
) -> Vec<TestCandidate> {
    let mut scored = Vec::new();

    for d in diffs {
        if !is_test_worthy(d) {
            continue;
        }

        let delta = match &d.delta {
            Some(d) => d,
            None => continue,
        };

        let decision = decide_test(d);
        if matches!(decision, TestDecision::No) {
            continue;
        }

        let summary = summarizer::summarize(d);

        let test_intent = match d.surface {
            ChangeSurface::ErrorPath | ChangeSurface::Branching =>
                TestIntent::Guardrail,

            ChangeSurface::PureLogic | ChangeSurface::Contract =>
                TestIntent::Regression,

            _ =>
                TestIntent::Regression,
        };

        let mut candidate = TestCandidate {
            id: String::new(),
            diff: d.clone(),
            file: d.file.clone(),
            symbol: d.symbol.clone(),
            target: crate::testgen::candidate::TestTarget::File(d.file.clone()),

            decision,
            risk: summary.risk,
            test_intent,

            behavior: summary.behavior.clone(),

            old_code: Some(delta.old_source.clone()),
            new_code: Some(delta.new_source.clone()),
        };

        candidate.id = match &candidate.symbol {
            Some(sym) => format!(
                "{}::{}::{:?}",
                candidate.file,
                sym,
                candidate.test_intent
            ),
            None => format!(
                "{}::<file>::{:?}",
                candidate.file,
                candidate.test_intent
            ),
        };


        scored.push((priority(d), candidate));
    }

    scored.sort_by_key(|(p, _)| std::cmp::Reverse(*p));
    scored.into_iter().map(|(_, c)| c).collect()
}
