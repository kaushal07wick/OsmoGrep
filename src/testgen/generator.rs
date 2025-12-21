//! generator.rs
//!
//! Test candidate generation pipeline.
//!
//! Hard guarantees:
//! - No UI / rendering / glue code
//! - No comment-only or formatting-only diffs
//! - No trivial helpers / parsers / serializers
//! - Only externally testable behavior reaches the agent

use crate::state::{ChangeSurface, DiffAnalysis, TestDecision};
use crate::testgen::candidate::{TestCandidate, TestType, TestTarget};
use crate::testgen::resolve::TestResolution;
use crate::testgen::summarizer;

/* ============================================================
   Normalization (NO SEMANTICS)
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
            !t.starts_with('#') && !t.starts_with("\"\"\"") && !t.starts_with("'''")
        })
        .map(|l| l.trim())
        .collect::<String>()
}

/* ============================================================
   HARD GATES (FACTS ONLY)
   ============================================================ */

fn is_ui_or_glue_code(d: &DiffAnalysis) -> bool {
    let file = d.file.to_lowercase();

    /* ---------- directory / module ---------- */

    if file.contains("/ui/")
        || file.contains("/terminal/")
        || file.contains("/render_")
        || file.contains("/cli/")
        || file.contains("/commands")
    {
        return true;
    }

    /* ---------- framework / infra ---------- */

    if file.contains("ratatui")
        || file.contains("crossterm")
        || file.contains("argparse")
        || file.contains("click")
        || file.contains("logging")
    {
        return true;
    }

    /* ---------- symbol-level ---------- */

    if let Some(sym) = &d.symbol {
        let s = sym.to_lowercase();

        // language-agnostic helpers
        let helper_prefixes = [
            "render_",
            "draw_",
            "format_",
            "parse_",
            "normalize_",
        ];

        if helper_prefixes.iter().any(|p| s.starts_with(p)) {
            return true;
        }

        // python-specific hard exclusions
        let python_helpers = [
            "__repr__",
            "__str__",
            "__eq__",
            "to_dict",
            "from_dict",
            "serialize",
            "deserialize",
            "validate",
            "validator",
            "schema",
        ];

        if python_helpers.iter().any(|p| s.contains(p)) {
            return true;
        }
    }

    false
}

fn is_test_worthy(d: &DiffAnalysis) -> bool {
    let delta = match &d.delta {
        Some(d) => d,
        None => return false,
    };

    let old_norm = normalize_source(&d.file, &delta.old_source);
    let new_norm = normalize_source(&d.file, &delta.new_source);

    // cosmetic-only changes
    if old_norm == new_norm {
        return false;
    }

    // extremely small diffs
    if old_norm.len().abs_diff(new_norm.len()) < 10 {
        return false;
    }

    // UI / glue / helpers
    if is_ui_or_glue_code(d) {
        return false;
    }

    true
}

/* ============================================================
   Decision logic (OWNED BY TESTGEN)
   ============================================================ */

fn decide_test(d: &DiffAnalysis) -> TestDecision {
    match d.surface {
        ChangeSurface::Contract
        | ChangeSurface::ErrorPath
        | ChangeSurface::Branching
        | ChangeSurface::State
        | ChangeSurface::Integration => TestDecision::Yes,

        ChangeSurface::PureLogic => TestDecision::Conditional,

        _ => TestDecision::No,
    }
}

/* ============================================================
   Priority (DETERMINISTIC)
   ============================================================ */

fn priority(d: &DiffAnalysis) -> u8 {
    let mut score = 0;

    score += match d.surface {
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

/* ============================================================
   Public API
   ============================================================ */

pub fn generate_test_candidates(
    diffs: &[DiffAnalysis],
    resolve: impl Fn(&TestCandidate) -> TestResolution,
) -> Vec<TestCandidate> {
    let mut scored = Vec::new();

    for d in diffs {
        // 🔒 HARD GATE
        if !is_test_worthy(d) {
            continue;
        }

        let delta = match &d.delta {
            Some(d) => d,
            None => continue,
        };

        let semantic = summarizer::summarize(d);
        let decision = decide_test(d);

        if matches!(decision, TestDecision::No) {
            continue;
        }

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

            decision,
            risk: semantic.risk,

            test_type,
            target,

            behavior: semantic.behavior.clone(),
            failure_mode: semantic.failure_mode.clone(),

            old_code: Some(delta.old_source.clone()),
            new_code: Some(delta.new_source.clone()),
        };

        match resolve(&candidate) {
            TestResolution::Found { file, .. } => {
                candidate.behavior =
                    format!("Update existing test `{}`", file);
            }
            TestResolution::Ambiguous(paths) => {
                candidate.decision = TestDecision::Conditional;
                candidate.behavior = format!(
                    "Multiple candidate tests found:\n{}",
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

        scored.push((priority(d), candidate));
    }

    scored.sort_by_key(|(p, _)| std::cmp::Reverse(*p));
    scored.into_iter().map(|(_, c)| c).collect()
}
