use crate::testgen::candidate::{TestCandidate, TestType, TestTarget};
use crate::state::{RiskLevel, TestDecision, Language, TestFramework};
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
    let system = system_prompt();
    let user = user_prompt(candidate, resolution, language, framework);

    LlmPrompt { system, user }
}

/* ============================================================
   System prompt (stable, reused)
   ============================================================ */

fn system_prompt() -> String {
    r#"
You are an expert software engineer focused on writing correct, minimal, high-signal tests.

Rules:
- Prefer regression tests unless explicitly instructed otherwise
- Do NOT refactor production code
- Do NOT change behavior unless instructed
- Tests must be deterministic and readable
- Modify existing tests when possible; avoid duplication
- Only output valid test code, nothing else
"#
    .trim()
    .to_string()
}

/* ============================================================
   User prompt (fully derived from TestCandidate)
   ============================================================ */

fn user_prompt(
    c: &TestCandidate,
    resolution: &TestResolution,
    language: Option<&Language>,
    framework: Option<&TestFramework>,
) -> String {
    let mut out = String::new();

    /* ---------- CONTEXT ---------- */
    out.push_str("CONTEXT\n");
    out.push_str(&format!("File: {}\n", c.file));

    if let Some(sym) = &c.symbol {
        out.push_str(&format!("Symbol: {}\n", sym));
    }

    if let Some(lang) = language {
        out.push_str(&format!("Language: {:?}\n", lang));
    }

    if let Some(fw) = framework {
        out.push_str(&format!("Test Framework: {:?}\n", fw));
    }

    out.push_str(&format!(
        "Test Type: {:?}\nRisk: {:?}\nDecision: {:?}\n",
        c.test_type, c.risk, c.decision
    ));

    out.push_str(&format!("Target: {}\n\n", format_target(&c.target)));

    out.push_str(
        "Decision meaning:\n\
         - Yes: test must be written or updated\n\
         - Conditional: write test only if meaningful for this change\n\n",
    );

    /* ---------- INTENT ---------- */
    out.push_str("INTENT\n");
    out.push_str(&format!(
        "Behavior to preserve:\n{}\n\n",
        c.behavior
    ));
    out.push_str(&format!(
        "Failure mode if incorrect:\n{}\n\n",
        c.failure_mode
    ));

    /* ---------- CODE CONTEXT ---------- */
    if let Some(old) = &c.old_code {
        out.push_str("OLD CODE\n```\n");
        out.push_str(old);
        out.push_str("\n```\n\n");
    }

    if let Some(new) = &c.new_code {
        out.push_str("NEW CODE\n```\n");
        out.push_str(new);
        out.push_str("\n```\n\n");
    }

    /* ---------- TEST RESOLUTION ---------- */
    out.push_str("TEST STATUS\n");

    match resolution {
        TestResolution::Found { file, test_fn } => {
            out.push_str(&format!(
                "An existing test was found at: {}\n",
                file
            ));

            if let Some(name) = test_fn {
                out.push_str(&format!(
                    "Relevant test function: {}\n",
                    name
                ));
            }

            out.push_str(
                "Modify the existing test. Do NOT create a new test unless strictly necessary.\n",
            );
        }

        TestResolution::Ambiguous(paths) => {
            out.push_str("Multiple possible test locations were found:\n");
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
            "\nRust testing rules:\n\
             - Prefer #[cfg(test)] mod tests in the same file\n\
             - Only create files under tests/ if clearly appropriate\n",
        );
    }

    /* ---------- CONSTRAINTS ---------- */
    out.push_str("\nCONSTRAINTS\n");
    out.push_str(&risk_constraints(c.risk));
    out.push_str(&test_type_constraints(c.test_type));

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

fn risk_constraints(risk: RiskLevel) -> String {
    match risk {
        RiskLevel::High => {
            "- Include edge cases\n- Include failure assertions\n- Be strict\n".into()
        }
        RiskLevel::Medium => {
            "- Cover core behavior\n- Avoid over-testing\n".into()
        }
        RiskLevel::Low => {
            "- Minimal sanity check only\n".into()
        }
    }
}

fn test_type_constraints(tt: TestType) -> String {
    match tt {
        TestType::Regression => {
            "- Assert previously working behavior still holds\n".into()
        }
        TestType::Unit => {
            "- Isolate the function\n- Avoid external dependencies\n".into()
        }
        TestType::Integration => {
            "- Use real components\n- Avoid mocks unless required\n".into()
        }
    }
}
