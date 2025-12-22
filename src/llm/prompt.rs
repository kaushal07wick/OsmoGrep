// src/llm/prompt.rs
//
// Deterministic LLM prompt construction.
//
// INVARIANTS:
// - Test code ONLY
// - Diff-driven
// - Symbol-exact
// - No guessing
// - No production code changes

use crate::context::types::{
    ContextSlice,
    TestMatchKind,
    SymbolResolution,
};
use crate::testgen::candidate::TestCandidate;
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
   System prompt (HARD constraints)
   ============================================================ */

fn system_prompt() -> String {
    r#"
You generate tests.

Rules:
- Output ONLY valid test code
- No explanations, comments, or markdown
- Do NOT modify production code
- Do NOT add new dependencies
- Follow existing project test style
- Assume the code compiles
"#.trim().to_string()
}

/* ============================================================
   User prompt (FACTS → CONSTRAINTS → COMMAND)
   ============================================================ */

fn user_prompt(
    c: &TestCandidate,
    resolution: &TestResolution,
    ctx: &ContextSlice,
) -> String {
    let mut out = String::new();

    /* ---------- repository facts ---------- */

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
        out.push_str("Forbidden dependencies: ");
        out.push_str(&ctx.repo_facts.forbidden_deps.join(", "));
        out.push('\n');
    }

    /* ---------- diff target (AUTHORITATIVE) ---------- */

    out.push_str("\nDiff target:\n");
    out.push_str(&format!(
        "- File: {}\n",
        ctx.diff_target.file.display()
    ));

    if let Some(sym) = &ctx.diff_target.symbol {
        out.push_str(&format!("- Symbol (from diff): {}\n", sym));
    }

    /* ---------- symbol resolution (STRICT) ---------- */

    out.push_str("\nSymbol resolution:\n");

    match &ctx.symbol_resolution {
        SymbolResolution::Resolved(sym) => {
            out.push_str(&format!(
                "- Resolved: {} (line {})\n",
                sym.name, sym.line
            ));
        }

        SymbolResolution::Ambiguous(matches) => {
            out.push_str("- Ambiguous symbol match:\n");
            for s in matches {
                out.push_str(&format!(
                    "  - {} (line {})\n",
                    s.name, s.line
                ));
            }
            out.push_str(
                "Use ONLY the diff to determine correct test behavior.\n",
            );
        }

        SymbolResolution::NotFound => {
            out.push_str(
                "- Symbol not found in file.\n\
                 Write tests strictly from diff-visible behavior.\n",
            );
        }
    }

    /* ---------- local file context ---------- */

    if !ctx.local_symbols.is_empty() {
        out.push_str("\nOther symbols in same file:\n");
        for s in ctx.local_symbols.iter().take(8) {
            out.push_str(&format!(
                "- {} (line {})\n",
                s.name, s.line
            ));
        }
    }

    if !ctx.imports.is_empty() {
        out.push_str("\nImports in file:\n");
        for i in ctx.imports.iter().take(8) {
            out.push_str(&format!("- {}\n", i.module));
        }
    }

    /* ---------- intent ---------- */

    out.push_str("\nTest intent:\n");
    out.push_str(&format!(
        "- Intent: {:?}\n- Risk: {:?}\n",
        c.test_intent,
        c.risk
    ));

    out.push_str("\nBehavior to validate:\n");
    out.push_str(&c.behavior);
    out.push('\n');

    /* ---------- code delta (AUTHORITATIVE) ---------- */

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

    /* ---------- test placement (DETERMINISTIC) ---------- */

    out.push_str("\nTest placement:\n");

    match ctx.test_context.match_kind {
        TestMatchKind::Filename | TestMatchKind::Content => {
            out.push_str(&format!(
                "- Update existing test file: {}\n",
                ctx.test_context.recommended_location.display()
            ));
        }
        TestMatchKind::None => {
            out.push_str(&format!(
                "- Create new test file at: {}\n",
                ctx.test_context.recommended_location.display()
            ));
        }
    }

    match resolution {
        TestResolution::Found { test_fn, .. } => {
            if let Some(name) = test_fn {
                out.push_str(&format!(
                    "- Target test function: {}\n",
                    name
                ));
            }
        }
        TestResolution::Ambiguous(_) => {
            out.push_str(
                "- Multiple test files exist; choose the most relevant one.\n",
            );
        }
        TestResolution::NotFound => {}
    }

    /* ---------- final command ---------- */

    out.push_str("\nWrite the test now.\n");

    out
}
