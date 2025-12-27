// src/llm/prompt.rs
//
// Minimal deterministic prompt for test generation.

use crate::context::types::{ContextSlice, SymbolResolution};
use crate::testgen::candidate::TestCandidate;

#[derive(Debug, Clone)]
pub struct LlmPrompt {
    pub system: String,
    pub user: String,
}

pub fn build_prompt(
    candidate: &TestCandidate,
    context: &ContextSlice,
) -> LlmPrompt {
    LlmPrompt {
        system: system_prompt(),
        user: user_prompt(candidate, context),
    }
}

fn system_prompt() -> String {
    r#"
You are an expert software engineer writing automated tests.

Rules:
- Output ONLY valid test code
- Do NOT use markdown or code fences
- Do NOT import new libraries
- Do NOT modify production code
- Use only existing project APIs
- Prefer behavior-based assertions
- The output must be directly executable
"#
    .trim()
    .to_string()
}

fn user_prompt(
    c: &TestCandidate,
    ctx: &ContextSlice,
) -> String {
    let mut out = String::new();

    /* === TARGET === */

    out.push_str("Target:\n");
    out.push_str(&format!(
        "- File: {}\n",
        ctx.diff_target.file.display()
    ));

    if let Some(sym) = &ctx.diff_target.symbol {
        out.push_str(&format!("- Symbol: {}\n", sym));
    }

    /* === SYMBOL CONTEXT === */

    match &ctx.symbol_resolution {
        SymbolResolution::Resolved(sym) => {
            out.push_str(&format!(
                "Resolved symbol: {} (line {})\n",
                sym.name, sym.line
            ));
        }
        SymbolResolution::Ambiguous(_) => {
            out.push_str("Symbol resolution: ambiguous\n");
        }
        SymbolResolution::NotFound => {
            out.push_str("Symbol not found in file\n");
        }
    }

    /* === SOURCE CODE === */

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

    /* === CONSTRAINTS === */

    out.push_str(
        "\nConstraints:\n\
- Only test observable behavior\n\
- Do not assert on implementation details\n\
- Do not import external libraries\n\
- Do not use randomness\n\
- Ensure test fails if behavior regresses\n",
    );

    /* === INSTRUCTION === */

    out.push_str("\nWrite the test.\n");

    out
}
