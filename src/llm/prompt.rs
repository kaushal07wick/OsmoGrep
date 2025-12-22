//! src/llm/prompt.rs
//!
//! Deterministic LLM prompt construction.
//!
//! Output:
//! - Test code ONLY

use crate::context::types::ContextSlice;
use crate::testgen::candidate::{
    TestCandidate,
};

use crate::context::types::{
    AssertionStyle,
    FailureMode,
};

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
   System prompt (hard rules)
   ============================================================ */

fn system_prompt() -> String {
    r#"
You generate tests.

Rules:
- Output ONLY valid test code
- No explanations, markdown, or commentary
- Do NOT modify production code
- Do NOT add new dependencies
- Follow existing project test style
- Assume code compiles
"#
    .trim()
    .to_string()
}

/* ============================================================
   User prompt (facts → constraints → command)
   ============================================================ */

fn user_prompt(
    c: &TestCandidate,
    resolution: &TestResolution,
    ctx: &ContextSlice,
) -> String {
    let mut out = String::new();

    /* ---------- repository context ---------- */

    if !ctx.repo_facts.languages.is_empty() {
        out.push_str("Language: ");
        out.push_str(&ctx.repo_facts.languages.join(", "));
        out.push('\n');
    }

    if !ctx.repo_facts.test_frameworks.is_empty() {
        out.push_str("Test framework: ");
        out.push_str(&ctx.repo_facts.test_frameworks.join(", "));
        out.push('\n');
    }

    if !ctx.repo_facts.forbidden_deps.is_empty() {
        out.push_str("Forbidden dependencies: ");
        out.push_str(&ctx.repo_facts.forbidden_deps.join(", "));
        out.push('\n');
    }

    /* ---------- target ---------- */

    out.push_str("\nTarget:\n");
    out.push_str(&format!(
        "{} ({}:{})\n",
        ctx.target.name,
        ctx.target.file.display(),
        ctx.target.line
    ));

    if !ctx.deps.is_empty() {
        out.push_str("Related symbols:\n");
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

    out.push_str("\nTest intent:\n");
    out.push_str(&format!(
        "- Type: {:?}\n- Risk: {:?}\n- Semantic intent: {:?}\n",
        c.test_type, c.risk, c.test_intent
    ));

    out.push_str("\nBehavior to validate:\n");
    out.push_str(&c.behavior);
    out.push('\n');

    /* ---------- assertion strategy ---------- */

    out.push_str("\nAssertion style:\n");
    out.push_str(match c.assertion_style {
        AssertionStyle::Exact => "Exact equality\n",
        AssertionStyle::Approximate => "Approximate / tolerance\n",
        AssertionStyle::Sanity => "Sanity / invariants only\n",
    });

    /* ---------- failure modes (THIS IS THE FIX) ---------- */

    if !c.failure_modes.is_empty() {
        out.push_str("\nMust guard against:\n");
        for fm in &c.failure_modes {
            out.push_str("- ");
            out.push_str(match fm {
                FailureMode::Panic => "panic\n",
                FailureMode::RuntimeError => "runtime error\n",
                FailureMode::NaN => "NaN values\n",
                FailureMode::Inf => "infinite values\n",
                FailureMode::WrongShape => "wrong shape\n",
            });
        }
    }

    /* ---------- oracle ---------- */

    out.push_str("\nOracle:\n");
    out.push_str(&format!("{:?}\n", c.oracle));

    /* ---------- code delta ---------- */

    if let Some(old) = &c.old_code {
        out.push_str("\nPrevious code:\n");
        out.push_str(old);
        out.push('\n');
    }

    if let Some(new) = &c.new_code {
        out.push_str("\nCurrent code:\n");
        out.push_str(new);
        out.push('\n');
    }

    /* ---------- test placement ---------- */

    match resolution {
        TestResolution::Found { file, test_fn } => {
            out.push_str(&format!("\nUpdate test file: {}\n", file));
            if let Some(name) = test_fn {
                out.push_str(&format!("Target test: {}\n", name));
            }
        }
        TestResolution::Ambiguous(paths) => {
            out.push_str("\nChoose test location:\n");
            for p in paths {
                out.push_str(&format!("- {}\n", p));
            }
        }
        TestResolution::NotFound => {
            out.push_str("\nCreate a new test in the appropriate test directory.\n");
        }
    }

    /* ---------- final command ---------- */

    out.push_str("\nWrite the test now.\n");

    out
}
