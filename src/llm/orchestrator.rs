use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::thread;

use crate::context::types::ContextSnapshot;
use crate::detectors::language::Language;
use crate::llm::backend::LlmBackend;
use crate::llm::prompt::build_prompt;
use crate::state::{AgentEvent, LogLevel, TestDecision, TestResult};
use crate::testgen::candidate::TestCandidate;
use crate::testgen::materialize::materialize_test;
use crate::testgen::runner::{run_test, TestRunRequest};

const MAX_LLM_RETRIES: usize = 3;

pub fn run_llm_test_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    llm: LlmBackend,
    snapshot: ContextSnapshot,
    candidate: TestCandidate,
    language: Language,
) {
    thread::spawn(move || {
        let cancelled = || cancel_flag.load(Ordering::SeqCst);

        let fail = |msg: &str| {
            let _ = tx.send(AgentEvent::Log(LogLevel::Error, msg.into()));
            let _ = tx.send(AgentEvent::Failed(msg.into()));
        };

        if matches!(candidate.decision, TestDecision::No) {
            fail("Change is not test-worthy.");
            return;
        }

        let file_ctx = match snapshot
            .files
            .iter()
            .find(|f| f.path == candidate.diff.file)
        {
            Some(c) => c,
            None => {
                fail("No context found for selected diff file.");
                return;
            }
        };

        let prompt = build_prompt(&candidate, file_ctx);

        let mut final_code: Option<String> = None;
        let mut last_error: Option<String> = None;

        for attempt in 1..=MAX_LLM_RETRIES {
            if cancelled() {
                fail("Agent cancelled.");
                return;
            }

            let _ = tx.send(AgentEvent::Log(
                LogLevel::Info,
                format!("LLM attempt {attempt}"),
            ));

            let raw = match llm.run(prompt.clone()) {
                Ok(s) => s,
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };

            let cleaned = sanitize_llm_output(&raw);

            if cleaned.trim().is_empty() {
                last_error = Some("Empty test output".into());
                continue;
            }

            final_code = Some(cleaned);
            break;
        }

        let Some(test_code) = final_code else {
            fail(&format!(
                "Failed to generate valid test: {:?}",
                last_error
            ));
            return;
        };

        let repo_root =
            std::env::current_dir().expect("osmogrep must run in repo root");

        let test_path = match materialize_test(
            &repo_root,
            language,
            &candidate,
            &test_code,
        ) {
            Ok(p) => p,
            Err(e) => {
                fail(&format!("Failed to write test: {e}"));
                return;
            }
        };

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            format!("Test written to {}", test_path.display()),
        ));

        let request = match language {
            Language::Python => TestRunRequest::Python {
                test_path: test_path.clone(),
            },
            Language::Rust => TestRunRequest::Rust,
            _ => {
                fail("Unsupported language");
                return;
            }
        };

        let result = run_test(request);
        let _ = tx.send(AgentEvent::TestFinished(result.clone()));

        let _ = tx.send(AgentEvent::Finished);
    });
}

fn sanitize_llm_output(raw: &str) -> String {
    let mut s = raw.trim().to_string();

    if s.starts_with("```") {
        if let Some(i) = s.find('\n') {
            s = s[i + 1..].to_string();
        }
        if let Some(i) = s.rfind("```") {
            s = s[..i].to_string();
        }
    }

    if s.starts_with("python\n") {
        s = s.trim_start_matches("python\n").to_string();
    }

    s.trim().to_string()
}
