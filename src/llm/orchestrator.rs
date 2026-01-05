use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::thread;
use std::time::Duration;

use crate::context::types::FullContextSnapshot;
use crate::detectors::language::Language;
use crate::llm::backend::LlmBackend;
use crate::llm::prompt::{build_prompt, build_prompt_with_feedback};
use crate::state::{AgentEvent, AgentRunOptions, LogLevel, TestDecision, TestResult};
use crate::testgen::candidate::TestCandidate;
use crate::testgen::materialize::materialize_test;
use crate::testgen::runner::{run_test, TestRunRequest};
use crate::testgen::test_suite::run_test_suite_and_report;
use crate::testgen::cache::{SemanticCache, SemanticKey};

const MAX_LLM_RETRIES: usize = 3;
const MAX_ERROR_CHARS: usize = 4_000;

/// ------------------------------
/// Single-test repair flow
/// ------------------------------
pub fn run_single_test_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    llm: LlmBackend,
    snapshot: FullContextSnapshot,
    candidate: TestCandidate,
    language: Language,
    semantic_cache: Arc<SemanticCache>,
    run_options: AgentRunOptions,
) {
    thread::spawn(move || {
        let cancelled = || cancel_flag.load(Ordering::SeqCst);

        let fail = |msg: &str| {
            let _ = tx.send(AgentEvent::SpinnerStop);
            let _ = tx.send(AgentEvent::Log(LogLevel::Error, msg.to_string()));
            let _ = tx.send(AgentEvent::Failed(msg.to_string()));
        };

        // ---- eligibility ----
        if matches!(candidate.decision, TestDecision::No) {
            fail("Change is not test-worthy");
            return;
        }

        let file_ctx = match snapshot
            .code
            .files
            .iter()
            .find(|f| f.path == candidate.file)
        {
            Some(c) => c,
            None => {
                fail("No context found for selected file");
                return;
            }
        };

        let _ = tx.send(AgentEvent::SpinnerStart(
            "Generating & running tests…".into(),
        ));

        let cache_key = SemanticKey::from_candidate(&candidate).to_cache_key();

        // ---- cache fast path ----
        if !run_options.force_reload {
            if let Some(test_path) = semantic_cache.get(&cache_key) {
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
                        let _ = tx.send(AgentEvent::SpinnerStop);
                        let _ = tx.send(AgentEvent::Finished);
                        return;
                    }
                    TestResult::Failed { .. } => {
                        let _ = tx.send(AgentEvent::Log(
                            LogLevel::Warn,
                            "Cached test failed — regenerating via LLM".into(),
                        ));
                    }
                }
            }
        }

        // ---- LLM retry loop ----
        let mut last_test_code: Option<String> = None;
        let mut last_failure: Option<String> = None;

        for attempt in 1..=MAX_LLM_RETRIES {
            if cancelled() {
                fail("Agent cancelled");
                return;
            }

            let _ = tx.send(AgentEvent::Log(
                LogLevel::Info,
                format!("LLM attempt {attempt}/{MAX_LLM_RETRIES}"),
            ));

            let prompt = match (&last_test_code, &last_failure) {
                (Some(code), Some(err)) => build_prompt_with_feedback(
                    &candidate,
                    file_ctx,
                    &snapshot.tests,
                    code,
                    err,
                ),
                _ => build_prompt(&candidate, file_ctx, &snapshot.tests),
            };

            let llm_result = match llm.run(prompt, run_options.force_reload) {
                Ok(r) => r,
                Err(e) => {
                    last_failure = Some(e);
                    continue;
                }
            };

            let test_code = sanitize_llm_output(&llm_result.text);
            if test_code.is_empty() {
                last_failure = Some("LLM produced empty output".into());
                continue;
            }

            last_test_code = Some(test_code.clone());
            let _ = tx.send(AgentEvent::GeneratedTest(test_code.clone()));

            let repo_root = match std::env::current_dir() {
                Ok(p) => p,
                Err(e) => {
                    fail(&format!("Repo root error: {e}"));
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
                    last_failure = Some(e.to_string());
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

            match result {
                TestResult::Passed => {
                    semantic_cache.insert(cache_key, test_path_for_cache);
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

/// ------------------------------
/// Full-suite orchestration
/// ------------------------------
pub fn run_full_suite_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    llm: LlmBackend,
    snapshot: FullContextSnapshot,
    language: Language,
    semantic_cache: Arc<SemanticCache>,
    run_options: AgentRunOptions,
) {
    thread::spawn(move || {
        let repo_root = match std::env::current_dir() {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(AgentEvent::Failed(e.to_string()));
                return;
            }
        };

        let _ = tx.send(AgentEvent::SpinnerStart(
            "Running full test suite…".into(),
        ));

        let mut rounds = 0;

        loop {
            if cancel_flag.load(Ordering::SeqCst) {
                let _ = tx.send(AgentEvent::Failed("Agent cancelled".into()));
                return;
            }

            rounds += 1;

            let suite = match run_test_suite_and_report(language, &repo_root) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(AgentEvent::Failed(e.to_string()));
                    return;
                }
            };

            let _ = tx.send(AgentEvent::TestSuiteReport(
                suite.report_path.clone(),
            ));

            let failure_count = suite.failures.len();

            if failure_count == 0 {
                let _ = tx.send(AgentEvent::Log(
                    LogLevel::Success,
                    "✅ Full test suite passed (0 failures)".into(),
                ));

                let _ = tx.send(AgentEvent::Log(
                    LogLevel::Info,
                    format!("📄 Report: {}", suite.report_path.display()),
                ));

                let _ = tx.send(AgentEvent::SpinnerStop);
                let _ = tx.send(AgentEvent::Finished);
                return;
            }

            let _ = tx.send(AgentEvent::Log(
                LogLevel::Warn,
                format!("❌ {} test failure(s) detected", failure_count),
            ));

            if !run_options.unbounded && rounds > MAX_LLM_RETRIES {
                let _ = tx.send(AgentEvent::Failed(
                    "Full-suite repair exhausted retry limit".into(),
                ));
                return;
            }

            let failing = &suite.failures[0];

            let _ = tx.send(AgentEvent::Log(
                LogLevel::Info,
                format!("🛠 Fixing {}::{}", failing.file, failing.test),
            ));

            let candidate = TestCandidate::from_test_failure(
                failing.file.clone(),
                failing.test.clone(),
            );

            run_single_test_flow(
                tx.clone(),
                cancel_flag.clone(),
                llm.clone(),
                snapshot.clone(),
                candidate,
                language,
                semantic_cache.clone(),
                run_options.clone(),
            );

            // TEMPORARY: allow single-test flow to complete
            // (safe given current state-machine reentry)
            thread::sleep(Duration::from_millis(800));
        }
    });
}

/// ------------------------------
/// helpers
/// ------------------------------
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
