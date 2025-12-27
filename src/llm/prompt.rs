// src/llm/prompt.rs
//
// Deterministic LLM prompt construction.

use crate::context::types::{
    ContextSlice,
    SymbolResolution,
};
use crate::testgen::candidate::TestCandidate;
use crate::testgen::resolve::TestResolution;

#[derive(Debug, Clone)]
pub struct LlmPrompt {
    pub system: String,
    pub user: String,
}

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

fn system_prompt() -> String {
    r#"
You are an expert software engineer generating high-quality automated tests.

Rules:
- Output ONLY valid test code
- Do NOT include explanations, comments, or markdown
- Do NOT modify production code
- Do NOT add new dependencies
- Follow existing project testing conventions
- Assume the code compiles
- Prefer correctness, clarity, and determinism
"#.trim().to_string()
}

fn user_prompt(
    c: &TestCandidate,
    _resolution: &TestResolution,
    ctx: &ContextSlice,
) -> String {
    let mut out = String::new();

    /* ---------- repository facts ---------- */

    if !ctx.repo_facts.languages.is_empty() {
        out.push_str("Languages:\n");
        out.push_str(&ctx.repo_facts.languages.join(", "));
        out.push('\n');
    }

    if !ctx.repo_facts.test_frameworks.is_empty() {
        out.push_str("Test frameworks:\n");
        out.push_str(&ctx.repo_facts.test_frameworks.join(", "));
        out.push('\n');
    }

    if !ctx.repo_facts.forbidden_deps.is_empty() {
        out.push_str("Forbidden dependencies:\n");
        out.push_str(&ctx.repo_facts.forbidden_deps.join(", "));
        out.push('\n');
    }

    /* ---------- diff target ---------- */

    out.push_str("\nDiff target:\n");
    out.push_str(&format!(
        "- File: {}\n",
        ctx.diff_target.file.display()
    ));

    if let Some(sym) = &ctx.diff_target.symbol {
        out.push_str(&format!("- Symbol: {}\n", sym));
    }

    /* ---------- symbol resolution ---------- */

    out.push_str("\nSymbol resolution:\n");

    match &ctx.symbol_resolution {
        SymbolResolution::Resolved(sym) => {
            out.push_str(&format!(
                "- Resolved symbol: {} (line {})\n",
                sym.name, sym.line
            ));

            if let Some((cls, method)) = sym.name.split_once('.') {
                out.push_str(&format!(
                    "- Hierarchy: method `{}` on `{}`\n",
                    method, cls
                ));
            } else {
                out.push_str("- Hierarchy: top-level symbol\n");
            }
        }

        SymbolResolution::Ambiguous(matches) => {
            out.push_str("- Ambiguous symbol matches:\n");
            for s in matches {
                out.push_str(&format!("  - {} (line {})\n", s.name, s.line));
            }
            out.push_str(
                "Use ONLY the diff to infer intended behavior.\n",
            );
        }

        SymbolResolution::NotFound => {
            out.push_str(
                "- Symbol not found in file.\n\
Write tests strictly from observable diff behavior.\n",
            );
        }
    }

    /* ---------- local context ---------- */

    if !ctx.local_symbols.is_empty() {
        out.push_str("\nOther nearby symbols (non-authoritative):\n");
        for s in ctx.local_symbols.iter().take(6) {
            out.push_str(&format!("- {} (line {})\n", s.name, s.line));
        }
    }

    if !ctx.imports.is_empty() {
        out.push_str("\nImports in file:\n");
        for i in ctx.imports.iter().take(6) {
            out.push_str(&format!("- {}\n", i.module));
        }
    }

    /* ---------- test intent ---------- */

    out.push_str("\nTest intent:\n");
    out.push_str(&format!(
        "- Intent: {:?}\n- Risk: {:?}\n",
        c.test_intent,
        c.risk
    ));

    /* ---------- behavior contract ---------- */

    if !c.behavior.is_empty() {
        out.push_str("\nExpected behavior (from diff analysis):\n");
        out.push_str(&c.behavior);
        out.push('\n');
    }

    /* ---------- constraints ---------- */

    out.push_str(
        "\nConstraints:\n\
- Test only observable behavior\n\
- Do not rely on internal state or private helpers\n\
- Do not assert on implementation details\n\
- Prefer invariants and output correctness\n"
    );

    /* ---------- previous / new code ---------- */

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

    /* ---------- numeric & stability rules ---------- */

    out.push_str(
        "\nNumeric stability rules:\n\
- Avoid exact equality for floating-point comparisons\n\
- Prefer tolerance-based or relative comparisons\n\
- Assume small numerical drift is acceptable\n\
- Avoid asserting on internal representations\n"
    );

    /* ---------- style reference ---------- */

    out.push_str(
        "\nReference tests (STYLE ONLY — do NOT copy behavior):\n\
\n\
def test_tanh(self):\n\
    helper_test_op([(45,65)], lambda x: x.tanh(), Tensor.tanh, atol=1e-6, grad_atol=1e-6)\n\
\n\
def test_hardtanh(self):\n\
    helper_test_op([(45,65)], lambda x: torch.nn.functional.hardtanh(x,-5, 5), lambda x: x.hardtanh(-5, 5), atol=1e-6)\n\
\n\
Notes:\n\
- These illustrate structure, not semantics\n\
- Generated tests MUST NOT import torch or external libs\n"
    );

    /* ---------- execution check ---------- */

    out.push_str(
        "\nBefore finalizing, mentally verify:\n\
- pytest can discover the test\n\
- the test runs deterministically\n\
- the test fails if behavior regresses\n"
    );

    out.push_str("\nWrite the test now.\n");

    out
}
