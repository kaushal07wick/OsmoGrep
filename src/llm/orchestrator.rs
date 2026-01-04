// src/llm/orchestrator.rs

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::thread;
use crate::testgen::test_suite::run_full_test_suite;
use crate::context::types::FullContextSnapshot;
use crate::detectors::language::Language;
use crate::llm::backend::LlmBackend;
use crate::llm::client::LlmRunResult;
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
    snapshot: FullContextSnapshot,
    candidate: TestCandidate,
    language: Language,
    semantic_cache: Arc<SemanticCache>,
    force_reload: bool,
) {
    thread::spawn(move || {
        let check_cancel = || cancel_flag.load(Ordering::SeqCst);

        let cancel_and_exit = || {
            let _ = tx.send(AgentEvent::SpinnerStop);
            let _ = tx.send(AgentEvent::Failed(
                "Agent cancelled".to_string(),
            ));
        };

        let fail = |msg: &str| {
            let _ = tx.send(AgentEvent::SpinnerStop);
            let _ = tx.send(AgentEvent::Log(LogLevel::Error, msg.to_string()));
            let _ = tx.send(AgentEvent::Failed(msg.to_string()));
        };

        // ---- eligibility ----
        if matches!(candidate.decision, TestDecision::No) {
            fail("Change is not test-worthy.");
            return;
        }

        let file_ctx = match snapshot
            .code
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

        // ---- start spinner (once) ----
        let _ = tx.send(AgentEvent::SpinnerStart(
            "Generating & running tests…".to_string(),
        ));

        // ---- semantic cache ----
        let semantic_key = SemanticKey::from_candidate(&candidate);
        let cache_key = semantic_key.to_cache_key();

        if !force_reload {
            if let Some(test_path) = semantic_cache.get(&cache_key) {
                if check_cancel() {
                    cancel_and_exit();
                    return;
                }

                let request = match language {
                    Language::Python => TestRunRequest::Python { test_path },
                    Language::Rust => TestRunRequest::Rust,
                    _ => {
                        fail("Unsupported language");
                        return;
                    }
                };

                let _ = tx.send(AgentEvent::TestStarted);
                let result = run_test(request);
                let _ = tx.send(AgentEvent::TestFinished(result.clone()));

                match result {
                    TestResult::Passed => {
                        let _ = tx.send(AgentEvent::Log(
                            LogLevel::Success,
                            "Cached test passed".to_string(),
                        ));
                        
                        let _ = tx.send(AgentEvent::SpinnerStop);
                        let _ = tx.send(AgentEvent::Finished);
                    }
                    TestResult::Failed { output } => {
                        fail(&output);
                    }
                }
                return;
            }
        }


        // ---- LLM retry loop ----
        let mut last_test_code: Option<String> = None;
        let mut last_failure: Option<String> = None;

        for attempt in 1..=MAX_LLM_RETRIES {
            if check_cancel() {
                cancel_and_exit();
                return;
            }

            if attempt > 1 {
                let _ = tx.send(AgentEvent::Log(
                    LogLevel::Warn,
                    "Retrying with feedback".to_string(),
                ));
            }

            let _ = tx.send(AgentEvent::Log(
                LogLevel::Info,
                format!("LLM attempt {attempt}/{MAX_LLM_RETRIES}"),
            ));

            let test_ctx = &snapshot.tests;

            let prompt = match (&last_test_code, &last_failure) {
                (Some(code), Some(err)) => {
                    build_prompt_with_feedback(
                        &candidate,
                        file_ctx,
                        test_ctx,
                        code,
                        err,
                    )
                }
                _ => build_prompt(
                    &candidate,
                    file_ctx,
                    test_ctx,
                ),
            };

            // ---- LLM run ----
            let llm_result: LlmRunResult = match llm.run(prompt, force_reload) {
                Ok(r) => r,
                Err(e) => {
                    last_failure = Some(e);
                    continue;
                }
            };

            if check_cancel() {
                cancel_and_exit();
                return;
            }

            if let Some(cached) = llm_result.cached_tokens {
                let _ = tx.send(AgentEvent::Log(
                    LogLevel::Info,
                    format!(
                        "🟢 LLM cache hit: {} tokens (hash={})",
                        cached, llm_result.prompt_hash
                    ),
                ));
            }

            let test_code = sanitize_llm_output(&llm_result.text);
            if test_code.is_empty() {
                last_failure = Some("LLM produced empty output".to_string());
                continue;
            }

            last_test_code = Some(test_code.clone());
            let _ = tx.send(AgentEvent::GeneratedTest(test_code.clone()));

            let repo_root = match std::env::current_dir() {
                Ok(p) => p,
                Err(e) => {
                    fail(&format!("Failed to get repo root: {e}"));
                    return;
                }
            };

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

            let test_path_for_cache = test_path.clone();

            let request = match language {
                Language::Python => TestRunRequest::Python { test_path },
                Language::Rust => TestRunRequest::Rust,
                _ => {
                    fail("Unsupported language");
                    return;
                }
            };

            let _ = tx.send(AgentEvent::TestStarted);
            let result = run_test(request);
            let _ = tx.send(AgentEvent::TestFinished(result.clone()));

            if check_cancel() {
                cancel_and_exit();
                return;
            }

            match result {
                TestResult::Passed => {
                    semantic_cache.insert(cache_key, test_path_for_cache);
                    let _ = tx.send(AgentEvent::Log(
                        LogLevel::Success,
                        "Test passed — cached".to_string(),
                    ));
                    let _ = tx.send(AgentEvent::SpinnerStop);
                    let _ = tx.send(AgentEvent::Finished);
                    return;
                }
                TestResult::Failed { output } => {
                    last_failure = Some(trim_error(&output));
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
