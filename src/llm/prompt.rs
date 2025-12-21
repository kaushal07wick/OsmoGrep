//! src/llm/prompt.rs
//!
//! Deterministic LLM prompt construction.
//!
//! Inputs:
//! - TestCandidate
//! - TestResolution
//! - ContextSlice
//!
//! Output:
//! - Minimal, high-signal prompt
//!
//! This module knows NOTHING about indexing, UI, or execution.

use crate::context::types::ContextSlice;
use crate::state::RiskLevel;
use crate::testgen::candidate::{TestCandidate, TestTarget, TestType};
use crate::testgen::resolve::TestResolution;

/* ============================================================
   Prompt model
   ============================================================ */

#[derive(Debug)]
pub struct LlmPrompt {
    pub system: String,
    pub user: String,
}

/* ============================================================
   Public API
   ============================================================ */

pub fn build_prompt(
    candidate: &TestCandidate,
    resolution: &TestResolution,
    context: &ContextSlice,
) -> LlmPrompt {
    LlmPrompt {
        system: system_prompt(),
        user: user_prompt(candidate, resolution, context),
    }
}

/* ============================================================
   System prompt (strict + short)
   ============================================================ */

fn system_prompt() -> String {
    r#"
You write minimal, correct, deterministic tests.

RULES:
- Output ONLY valid test code
- NO explanations, markdown, or extra text
- Do NOT refactor production code
- Do NOT change behavior beyond the test
- Prefer updating existing tests
- Assume code compiles
"#
    .trim()
    .to_string()
}

/* ============================================================
   User prompt (signal-first)
   ============================================================ */

fn user_prompt(
    c: &TestCandidate,
    resolution: &TestResolution,
    ctx: &ContextSlice,
) -> String {
    let mut out = String::new();

    /* ---------- context (only if useful) ---------- */

    if !ctx.repo_facts.languages.is_empty() {
        out.push_str("Languages: ");
        out.push_str(&ctx.repo_facts.languages.join(", "));
        out.push('\n');
    }

    if !ctx.repo_facts.test_frameworks.is_empty() {
        out.push_str("Test frameworks: ");
        out.push_str(&ctx.repo_facts.test_frameworks.join(", "));
        out.push('\n');
    }

    if !ctx.repo_facts.forbidden_deps.is_empty() {
        out.push_str("Forbidden deps: ");
        out.push_str(&ctx.repo_facts.forbidden_deps.join(", "));
        out.push('\n');
    }

    /* ---------- target ---------- */

    out.push_str("\nTARGET:\n");
    out.push_str(&format!(
        "- {} ({}:{})\n",
        ctx.target.name,
        ctx.target.file.display(),
        ctx.target.line
    ));

    if !ctx.deps.is_empty() {
        out.push_str("\nRELATED SYMBOLS:\n");
        for s in ctx.deps.iter().take(5) {
            out.push_str(&format!(
                "- {} ({}:{})\n",
                s.name,
                s.file.display(),
                s.line
            ));
        }
    }

    /* ---------- intent ---------- */

    out.push_str("\nINTENT:\n");
    out.push_str(&format!(
        "- Test type: {:?}\n- Risk: {:?}\n- Decision: {:?}\n",
        c.test_type, c.risk, c.decision
    ));

    out.push_str("\nBehavior:\n");
    out.push_str(&c.behavior);
    out.push('\n');

    out.push_str("\nFailure mode:\n");
    out.push_str(&c.failure_mode);
    out.push('\n');

    /* ---------- code delta ---------- */

    if let Some(old) = &c.old_code {
        out.push_str("\nBEFORE:\n");
        out.push_str(old);
        out.push('\n');
    }

    if let Some(new) = &c.new_code {
        out.push_str("\nAFTER:\n");
        out.push_str(new);
        out.push('\n');
    }

    /* ---------- test resolution ---------- */

    out.push_str("\nTEST LOCATION:\n");

    match resolution {
        TestResolution::Found { file, test_fn } => {
            out.push_str(&format!("- Modify {}\n", file));
            if let Some(name) = test_fn {
                out.push_str(&format!("- Target test: {}\n", name));
            }
        }
        TestResolution::Ambiguous(paths) => {
            out.push_str("- Choose best location from:\n");
            for p in paths {
                out.push_str(&format!("  - {}\n", p));
            }
        }
        TestResolution::NotFound => {
            out.push_str("- Create a new regression test\n");
        }
    }

    /* ---------- constraints ---------- */

    out.push_str("\nCONSTRAINTS:\n");
    out.push_str(&risk_constraints(&c.risk));
    out.push_str(&test_type_constraints(&c.test_type));

    /* ---------- output ---------- */

    out.push_str(
        "\nOUTPUT:\n- Valid test code only\n- No extra text\n",
    );

    out
}

/* ============================================================
   Helpers
   ============================================================ */

fn risk_constraints(risk: &RiskLevel) -> &str {
    match risk {
        RiskLevel::High =>
            "- Include edge cases\n- Assert failures\n",
        RiskLevel::Medium =>
            "- Cover core behavior\n",
        RiskLevel::Low =>
            "- Minimal sanity test\n",
    }
}

fn test_type_constraints(tt: &TestType) -> &str {
    match tt {
        TestType::Regression =>
            "- Assert previously working behavior\n",
        TestType::Unit =>
            "- Isolate the function\n",
        TestType::Integration =>
            "- Use real components\n",
    }
}
