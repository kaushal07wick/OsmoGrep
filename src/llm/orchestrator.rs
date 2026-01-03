// src/llm/orchestrator.rs

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::thread;

use crate::context::types::ContextSnapshot;
use crate::detectors::language::Language;
use crate::llm::backend::LlmBackend;
use crate::llm::prompt::{build_prompt, build_prompt_with_feedback};
use crate::state::{AgentEvent, LogLevel, TestDecision, TestResult};
use crate::testgen::candidate::TestCandidate;
use crate::testgen::materialize::materialize_test;
use crate::testgen::runner::{run_test, TestRunRequest};
use crate::testgen::cache::{SemanticCache, SemanticKey};

const MAX_LLM_RETRIES: usize = 3;
const MAX_ERROR_CHARS: usize = 4_000;

pub fn run_llm_test_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    llm: LlmBackend,
    snapshot: ContextSnapshot,
    candidate: TestCandidate,
    language: Language,
    semantic_cache: Arc<SemanticCache>,
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

        // ---- semantic cache lookup ----
        let semantic_key = SemanticKey::from_candidate(&candidate);
        let cache_key = semantic_key.to_cache_key();

        if let Some(test_path) = semantic_cache.get(&cache_key) {
            let request = match language {
                Language::Python => TestRunRequest::Python { test_path },
                Language::Rust => TestRunRequest::Rust,
                _ => {
                    fail("Unsupported language");
                    return;
                }
            };

            let result = run_test(request);
            let _ = tx.send(AgentEvent::TestFinished(result.clone()));

            match result {
                TestResult::Passed => {
                    let _ = tx.send(AgentEvent::Log(
                        LogLevel::Success,
                        "Cached test passed".into(),
                    ));
                }
                TestResult::Failed { output } => {
                    let _ = tx.send(AgentEvent::Log(
                        LogLevel::Error,
                        "Cached test failed".into(),
                    ));
                    let _ = tx.send(AgentEvent::Log(LogLevel::Error, output));
                }
            }

            let _ = tx.send(AgentEvent::Finished);
            return;
        }

        // ---- LLM generation with feedback loop ----
        let mut last_test_code: Option<String> = None;
        let mut last_failure: Option<String> = None;

        for attempt in 1..=MAX_LLM_RETRIES {
            if cancelled() {
                fail("Agent cancelled.");
                return;
            }

            let _ = tx.send(AgentEvent::Log(
                LogLevel::Info,
                format!("LLM attempt {attempt}"),
            ));

            let prompt = match (&last_test_code, &last_failure) {
                (Some(code), Some(err)) => {
                    build_prompt_with_feedback(&candidate, file_ctx, code, err)
                }
                _ => build_prompt(&candidate, file_ctx),
            };

            let raw = match llm.run(prompt) {
                Ok(s) => s,
                Err(e) => {
                    last_failure = Some(e);
                    continue;
                }
            };

            let test_code = sanitize_llm_output(&raw);
            if test_code.is_empty() {
                last_failure = Some("LLM produced empty output".into());
                continue;
            }

            last_test_code = Some(test_code.clone());

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
                    last_failure = Some(format!("Failed to write test: {e}"));
                    continue;
                }
            };

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

            match result {
                TestResult::Passed => {
                    semantic_cache.insert(cache_key, test_path);
                    let _ = tx.send(AgentEvent::Log(
                        LogLevel::Success,
                        "Test passed — cached".into(),
                    ));
                    let _ = tx.send(AgentEvent::Finished);
                    return;
                }

                TestResult::Failed { output } => {
                    last_failure = Some(trim_error(&output));
                    let _ = tx.send(AgentEvent::Log(
                        LogLevel::Warn,
                        "Test failed — retrying with feedback".into(),
                    ));
                }
            }
        }

        fail("Exhausted retries — test could not be fixed");
    });
}

fn trim_error(s: &str) -> String {
    if s.len() > MAX_ERROR_CHARS {
        s[..MAX_ERROR_CHARS].to_string()
    } else {
        s.to_string()
    }
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
