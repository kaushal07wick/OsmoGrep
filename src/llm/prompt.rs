// src/llm/prompt.rs
//
// LLM prompt construction.
//
// This module:
// - consumes TestCandidate, TestResolution, ContextSlice
// - produces a deterministic text prompt
//
// It does NOT know about indexing, UI, or execution.
//

use crate::testgen::candidate::{TestCandidate, TestType, TestTarget};
use crate::testgen::resolve::TestResolution;
use crate::state::RiskLevel;
use crate::context::types::ContextSlice;

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
   System prompt
   ============================================================ */

fn system_prompt() -> String {
    r#"
You are an expert software engineer who writes precise, minimal, high-signal tests.

ABSOLUTE RULES:
- Output ONLY valid test code
- Do NOT include explanations, markdown, or commentary
- Do NOT refactor production code
- Do NOT change behavior beyond what is required for the test
- Tests must be deterministic and minimal
- Prefer modifying existing tests over creating new ones
- Assume the code under test already compiles

Violating these rules is incorrect behavior.
"#
    .trim()
    .to_string()
}

/* ============================================================
   User prompt
   ============================================================ */

fn user_prompt(
    c: &TestCandidate,
    resolution: &TestResolution,
    context: &ContextSlice,
) -> String {
    let mut out = String::new();

    /* ---------- repository context ---------- */

    out.push_str("=== REPOSITORY CONTEXT ===\n");
    out.push_str("Languages: ");
    out.push_str(&context.repo_facts.languages.join(", "));
    out.push('\n');

    out.push_str("Test Frameworks: ");
    out.push_str(&context.repo_facts.test_frameworks.join(", "));
    out.push('\n');

    if !context.repo_facts.forbidden_deps.is_empty() {
        out.push_str("Forbidden deps: ");
        out.push_str(&context.repo_facts.forbidden_deps.join(", "));
        out.push('\n');
    }

    out.push('\n');

    out.push_str("Target Symbol:\n");
    out.push_str(&format!(
        "- {} ({}:{})\n",
        context.target.name,
        context.target.file.display(),
        context.target.line
    ));

    if !context.deps.is_empty() {
        out.push_str("\nRelated Symbols:\n");
        for s in &context.deps {
            out.push_str(&format!(
                "- {} ({}:{})\n",
                s.name,
                s.file.display(),
                s.line
            ));
        }
    }

    if !context.imports.is_empty() {
        out.push_str("\nImports:\n");
        for i in &context.imports {
            out.push_str("- ");
            out.push_str(&i.module);
            if !i.names.is_empty() {
                out.push_str(" [");
                out.push_str(&i.names.join(", "));
                out.push(']');
            }
            out.push('\n');
        }
    }

    out.push('\n');


    /* ---------- test target ---------- */

    out.push_str("=== TEST TARGET ===\n");
    out.push_str(&format!("Target: {}\n", format_target(&c.target)));
    out.push_str(&format!(
        "Test Type: {:?}\nRisk: {:?}\nDecision: {:?}\n\n",
        c.test_type, c.risk, c.decision
    ));

    /* ---------- intent ---------- */

    out.push_str("=== INTENT ===\n");
    out.push_str("Behavior to preserve:\n");
    out.push_str(&c.behavior);
    out.push_str("\n\n");

    out.push_str("Failure mode if incorrect:\n");
    out.push_str(&c.failure_mode);
    out.push_str("\n\n");

    /* ---------- code delta ---------- */

    if let Some(old) = &c.old_code {
        out.push_str("=== PREVIOUS CODE ===\n");
        out.push_str(old);
        out.push_str("\n\n");
    }

    if let Some(new) = &c.new_code {
        out.push_str("=== UPDATED CODE ===\n");
        out.push_str(new);
        out.push_str("\n\n");
    }

    /* ---------- test resolution ---------- */

    out.push_str("=== TEST RESOLUTION ===\n");

    match resolution {
        TestResolution::Found { file, test_fn } => {
            out.push_str(&format!("Existing test file: {}\n", file));
            if let Some(name) = test_fn {
                out.push_str(&format!("Relevant test function: {}\n", name));
            }
            out.push_str(
                "Modify the existing test. Do NOT create a new test unless strictly necessary.\n",
            );
        }

        TestResolution::Ambiguous(paths) => {
            out.push_str("Multiple possible test locations:\n");
            for p in paths {
                out.push_str(&format!("- {}\n", p));
            }
            out.push_str(
                "Choose the most appropriate location and add a regression test.\n",
            );
        }

        TestResolution::NotFound => {
            out.push_str(
                "No existing test was found.\nCreate a new regression test.\n",
            );
        }
    }

    /* ---------- constraints ---------- */

    out.push_str("\n=== CONSTRAINTS ===\n");
    out.push_str(&risk_constraints(&c.risk));
    out.push_str(&test_type_constraints(&c.test_type));

    /* ---------- output ---------- */

    out.push_str(
        "\n=== OUTPUT REQUIREMENTS ===\n\
         - Output ONLY valid test code\n\
         - No explanations\n\
         - No markdown\n\
         - No extra text\n",
    );

    out
}

/* ============================================================
   Helpers
   ============================================================ */

fn format_target(t: &TestTarget) -> String {
    match t {
        TestTarget::Symbol(s) => format!("Function {}", s),
        TestTarget::Module(m) => format!("Module {}", m),
        TestTarget::File(f) => format!("File {}", f),
    }
}

fn risk_constraints(risk: &RiskLevel) -> String {
    match risk {
        RiskLevel::High =>
            "- Include edge cases\n- Include failure assertions\n- Be strict\n".into(),
        RiskLevel::Medium =>
            "- Cover core behavior\n- Avoid over-testing\n".into(),
        RiskLevel::Low =>
            "- Minimal sanity check only\n".into(),
    }
}

fn test_type_constraints(tt: &TestType) -> String {
    match tt {
        TestType::Regression =>
            "- Assert previously working behavior still holds\n".into(),
        TestType::Unit =>
            "- Isolate the function\n- Avoid external dependencies\n".into(),
        TestType::Integration =>
            "- Use real components\n- Avoid mocks unless required\n".into(),
    }
}
