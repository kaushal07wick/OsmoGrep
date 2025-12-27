// src/testgen/generator.rs
//
// Test candidate generation pipeline.

use crate::state::{ChangeSurface, DiffAnalysis, TestDecision};
use crate::testgen::candidate::{TestCandidate, TestTarget};
use crate::testgen::summarizer;

/* ============================================================
   Helpers
   ============================================================ */

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
        | ChangeSurface::State => TestDecision::Yes,

        ChangeSurface::PureLogic => TestDecision::Conditional,

        _ => TestDecision::No,
    }
}

fn priority(d: &DiffAnalysis) -> u8 {
    let mut score = match d.surface {
        ChangeSurface::Contract => 40,
        ChangeSurface::ErrorPath => 35,
        ChangeSurface::Branching => 30,
        ChangeSurface::State => 30,
        ChangeSurface::PureLogic => 20,
        _ => 0,
    };

    if d.symbol.is_some() {
        score += 10;
    }

    score
}

/* ============================================================
   Public API
   ============================================================ */

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

        let candidate = TestCandidate {
            id: format!(
                "{}::{}",
                d.file,
                d.symbol.as_deref().unwrap_or("<file>")
            ),

            diff: d.clone(),
            file: d.file.clone(),
            symbol: d.symbol.clone(),
            target: TestTarget::File(d.file.clone()),

            decision,
            risk: summary.risk,

            old_code: Some(delta.old_source.clone()),
            new_code: Some(delta.new_source.clone()),
        };

        scored.push((priority(d), candidate));
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, c)| c).collect()

}
