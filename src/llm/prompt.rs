// src/llm/prompt.rs
//
// Deterministic, contract-first test generation with retry feedback support.

use crate::context::types::{
    FileContext, SymbolResolution, TestContextSnapshot,
};
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
    test_ctx: &TestContextSnapshot,
) -> LlmPrompt {
    LlmPrompt {
        system: system_prompt(),
        user: user_prompt(candidate, file_ctx, test_ctx),
    }
}

/// Retry attempt (includes previous test + execution failure)
pub fn build_prompt_with_feedback(
    candidate: &TestCandidate,
    file_ctx: &FileContext,
    test_ctx: &TestContextSnapshot,
    previous_test: &str,
    failure_feedback: &str,
) -> LlmPrompt {
    let mut user = user_prompt(candidate, file_ctx, test_ctx);

    user.push_str("\nPrevious generated test:\n");
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

You MUST follow this process internally:
1. Read the provided testing context.
2. Infer the existing testing conventions used in the repository.
3. Write a test that matches those conventions as closely as possible.

FORMAT:
- Output ONLY valid test code.
- Output EXACTLY one test.
- No comments, no markdown, no explanations.
- Do NOT modify production code.
- Use only public APIs.

QUALITY:
- Assert real, observable behavior.
- Do NOT compute expected values by calling the function under test.
- If behavior involves masking, filtering, or ignore_index:
  - Prove excluded elements do NOT affect the result.
  - Include a check that would fail if exclusion were removed.
- Ensure numeric outputs are finite when applicable.

DEVIATION WARNING:
If your test does not match the observed test style, helpers, or framework
used elsewhere in the repository, it will be considered incorrect.

Write a regression-grade test a senior engineer would accept.
"#
    .trim()
    .to_string()
}


fn user_prompt(
    candidate: &TestCandidate,
    ctx: &FileContext,
    test_ctx: &TestContextSnapshot,
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

    // ---- test context ----
    if test_ctx.exists {
        out.push_str("\nExisting test environment:\n");

        if let Some(fw) = test_ctx.framework {
            out.push_str(&format!(
                "- Observed test framework: {:?}\n",
                fw
            ));
        }

        if let Some(style) = test_ctx.style {
            out.push_str(&format!(
                "- Observed test style: {:?}\n",
                style
            ));
        }

        if !test_ctx.helpers.is_empty() {
            out.push_str(
                "- Helper functions observed in existing tests:\n",
            );
            for h in &test_ctx.helpers {
                out.push_str(&format!("  - {h}\n"));
            }
        }

        if !test_ctx.references.is_empty() {
            out.push_str(
                "- External reference libraries used in tests:\n",
            );
            for r in &test_ctx.references {
                out.push_str(&format!("  - {r}\n"));
            }
        }

        out.push_str(
            "\nInstruction:\n\
            Infer how tests are written in this repository based on the information above.\n\
            Match the same structure, conventions, and level of abstraction.\n\
            Prefer reusing existing helpers instead of reimplementing logic.\n",
        );
    } else {
        out.push_str(
            "\nNo existing tests were detected.\n\
            Write a test using standard conventions for this language.\n",
        );
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
