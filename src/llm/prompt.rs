// src/llm/prompt.rs
//
// Deterministic LLM prompt construction.

use crate::context::types::{
    ContextSlice,
    SymbolResolution,
};
use crate::testgen::candidate::TestCandidate;
use crate::testgen::resolve::TestResolution;


#[derive(Debug)]
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
You are an extremely smart and robust software engineer that generates accurate tests.

Rules:
- Output ONLY valid test code
- No explanations, comments, or markdown
- Do NOT modify production code
- Do NOT add new dependencies
- Follow existing project test style
- Assume the code compiles
- Reference the test samples and code style properly
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
        out.push_str("Languages: ");
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

    /* ---------- diff target---------- */

    out.push_str("\nDiff target:\n");
    out.push_str(&format!(
        "- File: {}\n",
        ctx.diff_target.file.display()
    ));

    if let Some(sym) = &ctx.diff_target.symbol {
        out.push_str(&format!("- Symbol (from diff): {}\n", sym));
    }

    /* ---------- symbol resolution + hierarchy ---------- */

    out.push_str("\nSymbol resolution:\n");

    match &ctx.symbol_resolution {
        SymbolResolution::Resolved(sym) => {
            out.push_str(&format!(
                "- Resolved: {} (line {})\n",
                sym.name, sym.line
            ));

            if let Some((cls, method)) = sym.name.split_once('.') {
                out.push_str(&format!(
                    "- Hierarchy: method `{}` on class `{}`\n",
                    method, cls
                ));
            } else {
                out.push_str("- Hierarchy: top-level function\n");
            }
        }

        SymbolResolution::Ambiguous(matches) => {
            out.push_str("- Ambiguous symbol matches:\n");
            for s in matches {
                out.push_str(&format!(
                    "  - {} (line {})\n",
                    s.name, s.line
                ));
            }
            out.push_str(
                "Use ONLY the diff to infer correct behavior.\n",
            );
        }

        SymbolResolution::NotFound => {
            out.push_str(
                "- Symbol not found in file.\n\
Write tests strictly from diff-visible behavior.\n",
            );
        }
    }

    /* ---------- bounded local context ---------- */

    if !ctx.local_symbols.is_empty() {
        out.push_str("\nOther symbols in same file (non-authoritative):\n");
        for s in ctx.local_symbols.iter().take(6) {
            out.push_str(&format!(
                "- {} (line {})\n",
                s.name, s.line
            ));
        }
    }

    if !ctx.imports.is_empty() {
        out.push_str("\nImports in file:\n");
        for i in ctx.imports.iter().take(6) {
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

    /* ---------- behavior---------- */

    out.push_str("\nBehavior to validate:\n");
    out.push_str("- Loss is computed correctly for valid class indices\n");
    out.push_str("- ignore_index entries do not contribute to loss\n");
    out.push_str("- Behavior is consistent across label dtypes\n");

    if !c.behavior.is_empty() {
        out.push_str("\nAdditional notes from diff analysis:\n");
        out.push_str(&c.behavior);
        out.push('\n');
    }

    /* ---------- input constraints---------- */

    out.push_str(
        "\nInput constraints:\n\
- Logits shape: (N, C) or broadcast-compatible\n\
- Labels shape: (N,) or compatible with logits\n\
- Labels may be integer tensor or equivalent dtype\n\
- ignore_index may appear multiple times\n"
    );

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

    /* ---------- numeric stability rules ---------- */

    out.push_str(
        "\nNumeric stability rules:\n\
- Do NOT use exact equality for floating-point assertions\n\
- Prefer pytest.approx or tolerance-based comparisons\n\
- Prefer relative differences or invariants over absolute values when possible\n\
- Assume small numerical drift is acceptable\n\
- Avoid asserting on internal tensor representations\n"
    );

    /* ---------- testing guardrails ---------- */

    out.push_str(
        "\nTesting guardrails:\n\
- Test ONLY externally observable behavior\n\
- Do NOT test private helpers or intermediate tensors\n\
- Do NOT assert on implementation details or execution paths\n\
- Focus on inputs → outputs → invariants\n"
    );

    /* ---------- reference tests (STYLE ONLY) ---------- */

    out.push_str(
        "\nReference tests from this repository (STYLE ONLY — do NOT copy behavior blindly):\n\
\n\
def test_tanh(self):\n\
    helper_test_op([(45,65)], lambda x: x.tanh(), Tensor.tanh, atol=1e-6, grad_atol=1e-6)\n\
    helper_test_op([(45,65)], lambda x: x.tanh(), Tensor.tanh, atol=1e-6, grad_atol=1e-6, a=-100)\n\
    helper_test_op([()], lambda x: x.tanh(), Tensor.tanh, atol=1e-6, grad_atol=1e-6)\n\
\n\
def test_hardtanh(self):\n\
    for val in range(10, 30, 5):\n\
        helper_test_op([(45,65)], lambda x: torch.nn.functional.hardtanh(x,-val, val), lambda x: x.hardtanh(-val, val), atol=1e-6, grad_atol=1e-6)\n\
        helper_test_op([()], lambda x: torch.nn.functional.hardtanh(x,-val, val), lambda x: x.hardtanh(-val, val), atol=1e-6, grad_atol=1e-6)\n\
\n\
def test_topo_sort(self):\n\
    helper_test_op([(45,65)], lambda x: (x+x)*x, lambda x: x.add(x).mul(x), atol=1e-6, grad_atol=1e-6)\n\
    helper_test_op([()], lambda x: (x+x)*x, lambda x: x.add(x).mul(x), atol=1e-6, grad_atol=1e-6)\n\
\n\
IMPORTANT:\n\
- Reference tests may use torch\n\
- GENERATED tests MUST NOT import or use torch\n\
\n\
Notes:\n\
- These demonstrate test structure, tolerance usage, and gradient checks\n\
- They do NOT define expected behavior for the current diff\n"
    );

    /* ---------- simulated pytest run ---------- */

    out.push_str(
        "\nBefore writing the final test, mentally simulate:\n\
- pytest discovers the test\n\
- test executes without errors\n\
- all assertions pass\n\
- test fails if behavior regresses\n"
    );

    /* ---------- final command ---------- */

    out.push_str("\nWrite the test now.\n");

    out
}
