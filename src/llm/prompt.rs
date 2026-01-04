// src/llm/prompt.rs
//
// Deterministic, contract-first test generation with retry feedback support.

use crate::context::types::{FileContext, SymbolResolution};
use crate::testgen::candidate::TestCandidate;

#[derive(Debug, Clone)]
pub struct LlmPrompt {
    pub system: String,
    pub user: String,
}

/// First attempt (no failure feedback)
pub fn build_prompt(
    candidate: &TestCandidate,
    file_ctx: &FileContext,
) -> LlmPrompt {
    LlmPrompt {
        system: system_prompt(),
        user: user_prompt(candidate, file_ctx),
    }
}

/// Retry attempt (includes previous test + execution failure)
pub fn build_prompt_with_feedback(
    candidate: &TestCandidate,
    ctx: &FileContext,
    previous_test: &str,
    failure_feedback: &str,
) -> LlmPrompt {
    let mut user = user_prompt(candidate, ctx);

    user.push_str("\n\nPrevious generated test:\n");
    user.push_str(previous_test);
    user.push('\n');

    user.push_str("\nTest execution FAILED with the following error:\n");
    user.push_str(failure_feedback);
    user.push('\n');

    user.push_str(
    "\nFix the test so that it PASSES on the current implementation.\n\
     RULES:\n\
     - Modify ONLY the test code\n\
     - Do NOT change production code\n\
     - Preserve the original test intent\n\
     - Do NOT weaken assertions\n\
     - Do NOT add conditionals to bypass failure\n\
     - Output EXACTLY ONE corrected test\n\
     - All imports must appear at the top of the test\n",
    );


    LlmPrompt {
        system: system_prompt(),
        user,
    }
}

fn system_prompt() -> String {
    r#"
You generate EXACTLY ONE unit test.

FORMAT RULES:
- Output ONLY valid test code.
- Output EXACTLY one test function named `test_*`.
- Imports are allowed.
- Do NOT define helper functions, local functions, or lambdas.
- No comments, no markdown, no explanations.
- Do NOT modify production code.
- Use only public APIs.

BEHAVIOR RULES:
- The test MUST pass on the current implementation.
- Do NOT assume behavior not implied by the code.

TEST QUALITY RULES:
- Assert real, observable behavior.
- Do NOT compute expected values by calling the function under test.
- If masking / ignore_index / filtering exists:
  - Prove ignored elements do NOT affect the result.
  - Include an assertion that would FAIL if masking were removed.
- If special values or dtypes are involved:
  - Use the effective value used in comparisons (after casting or normalization).
- For numeric results:
  - Assert correctness AND that an incorrect aggregation differs.
  - Ensure the result is finite.

Write a strict, regression-grade test a senior engineer would accept.
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

    // ---- code context ----
    if let Some(old) = &candidate.old_code {
        out.push_str("\nOld code:\n");
        out.push_str(old);
        out.push('\n');
    }

    if let Some(new) = &candidate.new_code {
        debug_assert!(
            !new.trim().is_empty(),
            "TestCandidate created without new_code"
        );

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
         - Assert that incorrect or previous behavior does NOT occur\n\
         - Be correct for edge cases implied by the change\n\
         - Avoid assumptions not proven by the code\n",
    );

    out
}
