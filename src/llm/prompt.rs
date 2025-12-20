use crate::testgen::candidate::{TestCandidate, TestType, TestTarget};
use crate::state::RiskLevel;
use crate::detectors::language::Language;
use crate::detectors::framework::TestFramework;
use crate::testgen::resolve::TestResolution;

#[derive(Debug)]
pub struct LlmPrompt {
    pub system: String,
    pub user: String,
}

pub fn build_prompt(
    candidate: &TestCandidate,
    resolution: &TestResolution,
    language: Option<&Language>,
    framework: Option<&TestFramework>,
) -> LlmPrompt {
    LlmPrompt {
        system: system_prompt(),
        user: user_prompt(candidate, resolution, language, framework),
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
- Do NOT include explanations, comments, or markdown
- Do NOT refactor production code
- Do NOT change behavior beyond what is required for the test
- Tests must be deterministic, readable, and minimal
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
    language: Option<&Language>,
    framework: Option<&TestFramework>,
) -> String {
    let mut out = String::new();

    /* ---------- CONTEXT ---------- */
    out.push_str("=== CONTEXT ===\n");
    out.push_str(&format!("File: {}\n", c.file));

    if let Some(sym) = &c.symbol {
        out.push_str(&format!("Symbol: {}\n", sym));
    }

    if let Some(lang) = language {
        out.push_str(&format!("Language: {}\n", format!("{:?}", lang)));
    }

    if let Some(fw) = framework {
        out.push_str(&format!("Test Framework: {}\n", format!("{:?}", fw)));
    }

    out.push_str(&format!(
        "Test Type: {:?}\nRisk: {:?}\nDecision: {:?}\n",
        c.test_type, c.risk, c.decision
    ));

    out.push_str(&format!("Target: {}\n\n", format_target(&c.target)));

    /* ---------- INTENT ---------- */
    out.push_str("=== INTENT ===\n");
    out.push_str(&format!("Behavior to preserve:\n{}\n\n", c.behavior));
    out.push_str(&format!(
        "Failure mode if incorrect:\n{}\n\n",
        c.failure_mode
    ));

    /* ---------- CODE CONTEXT ---------- */
    if let Some(old) = &c.old_code {
        out.push_str("=== OLD CODE ===\n");
        out.push_str("```\n");
        out.push_str(old);
        out.push_str("\n```\n\n");
    }

    if let Some(new) = &c.new_code {
        out.push_str("=== NEW CODE ===\n");
        out.push_str("```\n");
        out.push_str(new);
        out.push_str("\n```\n\n");
    }

    /* ---------- TEST RESOLUTION ---------- */
    out.push_str("=== TEST RESOLUTION ===\n");

    match resolution {
        TestResolution::Found { file, test_fn } => {
            out.push_str(&format!(
                "Existing test file: {}\n",
                file
            ));

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

    /* ---------- LANGUAGE-SPECIFIC RULES ---------- */
    if matches!(language, Some(Language::Rust)) {
        out.push_str(
            "\n=== RUST TESTING RULES ===\n\
             - Prefer #[cfg(test)] mod tests in the same file\n\
             - Only create files under tests/ if clearly appropriate\n",
        );
    }

    /* ---------- CONSTRAINTS ---------- */
    out.push_str("\n=== CONSTRAINTS ===\n");
    out.push_str(&risk_constraints(&c.risk));
    out.push_str(&test_type_constraints(&c.test_type));

    /* ---------- OUTPUT ---------- */
    out.push_str(
        "\n=== OUTPUT REQUIREMENTS ===\n\
         - Output ONLY valid test code\n\
         - No explanations\n\
         - No markdown\n\
         - No comments outside the test code\n",
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
