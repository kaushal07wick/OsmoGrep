// src/llm/prompt.rs
//
// Deterministic, diff-driven prompt construction.
// No repo-wide context. No guessing.

use crate::context::types::FileContext;
use crate::context::types::SymbolResolution;
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

/* ============================================================
   SYSTEM PROMPT
   ============================================================ */

fn system_prompt() -> String {
    r#"
You generate ONE unit test.

Rules (MANDATORY):
- Output ONLY valid test code.
- Output EXACTLY one test.
- Test name MUST start with `test_`.
- Imports are allowed.
- No comments, no explanations, no markdown.
- Do NOT modify production code.
- Do NOT define helper functions.
- Use only public APIs visible from the code.

Behavioral constraints:
- The test MUST reflect the code change shown.
- The test MUST fail on the old implementation.
- The test MUST pass on the new implementation.
- Do NOT assume behavior not proven by the diff.
- If behavior is ambiguous, assert the minimal observable guarantee
  (e.g. no crash, stable output, correct type, finite value).
"#
    .trim()
    .to_string()
}

/* ============================================================
   USER PROMPT
   ============================================================ */

fn user_prompt(
    c: &TestCandidate,
    ctx: &FileContext,
) -> String {
    let mut out = String::new();

    // ---- target ----
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

    // ---- diff context ----
    if let Some(old) = &c.old_code {
        out.push_str("\nOld code:\n");
        out.push_str(old);
        out.push('\n');
    }

    if let Some(new) = &c.new_code {
        out.push_str("\nNew code:\n");
        out.push_str(new);
        out.push('\n');
    }

    // ---- instruction ----
    out.push_str(
        "\nTask:\n\
Write ONE test that captures the real behavioral change.\n\
The test must:\n\
- Call the real public API\n\
- Assert observable runtime behavior\n\
- Fail on the old code\n\
- Pass on the new code\n\
- Avoid assumptions not implied by the diff\n",
    );

    out
}
