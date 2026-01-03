// src/llm/prompt.rs
//
// Deterministic, contract-first test generation.
use crate::context::types::{FileContext, SymbolResolution};
use crate::testgen::candidate::TestCandidate;

#[derive(Debug, Clone)]
pub struct LlmPrompt {
    pub system: String,
    pub user: String,
}

pub fn build_prompt(
    candidate: &TestCandidate,
    file_ctx: &FileContext,
) -> LlmPrompt {
    LlmPrompt {
        system: system_prompt(),
        user: user_prompt(candidate, file_ctx),
    }
}

fn system_prompt() -> String {
    r#"
You generate ONE unit test.

STRICT RULES:
- Output ONLY valid test code.
- Output EXACTLY one test function.
- Test name MUST start with `test_`.
- Imports are allowed.
- No comments, no explanations, no markdown.
- Do NOT define helper functions.
- Do NOT modify production code.
- Use only public APIs visible from the code.

BEHAVIORAL RULES:
- The test MUST reflect the real behavior of the code.
- The test MUST pass on the current implementation.
- Do NOT assume behavior not implied by the code.

CRITICAL CONSTRAINTS:
- If no prior test exists, do NOT assume previous behavior.
- Do NOT assert exceptions unless the code explicitly introduces them.
- If the change fixes a crash or dtype issue, assert correctness, not failure.
- If the function computes a numeric value, compute the expected value independently.
- Do NOT reimplement the production logic inside the test.
- Prefer observable guarantees: correctness, stability, finite output.

You are writing a regression-grade test a senior engineer would accept.
"#
    .trim()
    .to_string()
}


fn user_prompt(
    candidate: &TestCandidate,
    ctx: &FileContext,
) -> String {
    let mut out = String::new();

    // ---- target location ----
    out.push_str("Target file:\n");
    out.push_str(ctx.path.to_string_lossy().as_ref());
    out.push('\n');

    out.push_str("Target symbol:\n");
    match &ctx.symbol_resolution {
        SymbolResolution::Resolved(sym) => {
            out.push_str(&format!("{} (line {})\n", sym.name, sym.line));
        }
        SymbolResolution::Ambiguous(_) => {
            out.push_str("Ambiguous symbol\n");
        }
        SymbolResolution::NotFound => {
            out.push_str("Symbol not resolved\n");
        }
    }

    // ---- available imports ----
    if !ctx.imports.is_empty() {
        out.push_str("\nAvailable imports:\n");
        for imp in &ctx.imports {
            out.push_str("- ");
            out.push_str(&imp.module);
            out.push('\n');
        }
    }

    // ---- code context (ONLY when provided) ----
    if let Some(old) = &candidate.old_code {
        out.push_str("\nOld code:\n");
        out.push_str(old);
        out.push('\n');
    }

    if let Some(new) = &candidate.new_code {
        out.push_str("\nCurrent code:\n");
        out.push_str(new);
        out.push('\n');
    }

    // ---- task ----
    out.push_str(
        "\nTask:\n\
        Write ONE test that verifies the correct behavior of the current code.\n\
        The test must:\n\
        - Call the real public API\n\
        - Assert observable runtime behavior\n\
        - Be correct for edge cases implied by the change\n\
        - Avoid assumptions not proven by the code\n",
            );

    out
}
