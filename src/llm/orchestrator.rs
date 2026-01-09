use std::{path::{Path, PathBuf}, sync::{
    Arc, Mutex, atomic::{AtomicBool, Ordering}, mpsc::Sender
}};
use std::thread;
use crate::context::{full_suite_context::FailureInfo, types::FullContextSnapshot};
use crate::detectors::language::Language;
use crate::llm::backend::LlmBackend;
use crate::llm::prompt::{build_prompt, build_prompt_with_feedback};
use crate::state::{AgentEvent, AgentRunOptions, LogLevel, TestDecision, TestResult};
use crate::testgen::candidate::TestCandidate;
use crate::testgen::materialize::materialize_test;
use crate::testgen::runner::{run_test, TestRunRequest};
use crate::testgen::test_suite::run_test_suite_and_report;
use crate::testgen::cache::{SemanticCache, SemanticKey};
use crate::testgen::cache::{FullSuiteCache, FullSuiteCacheEntry};
use crate::testgen::materialize::materialize_full_suite_test;
use crate::context::full_suite_context::{
    build_full_suite_context,
};
use crate::llm::prompt::build_full_suite_prompt;
use crate::git::{git_create_sandbox_worktree};
use std::time::{Instant, SystemTime};
const MAX_LLM_RETRIES: usize = 3;
const MAX_ERROR_CHARS: usize = 4_000;

pub fn run_single_test_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    llm: LlmBackend,
    snapshot: Arc<FullContextSnapshot>,
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

//full test suite healing
// full test suite healing
pub fn run_full_suite_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    llm: LlmBackend,
    language: Language,
    run_options: AgentRunOptions,
    full_cache: Arc<Mutex<FullSuiteCache>>,
) {
    thread::spawn(move || {
        let cancelled = || cancel_flag.load(Ordering::SeqCst);

        // REPO ROOT
        let repo_root = match std::env::current_dir() {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(AgentEvent::Failed(format!("repo root error: {}", e)));
                return;
            }
        };

        let _ = tx.send(AgentEvent::SpinnerStart("Running full test suite…".into()));

        // RUN FULL SUITE
        let suite = match run_test_suite_and_report(language, &repo_root) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(AgentEvent::Failed(format!("suite run failed: {}", e)));
                return;
            }
        };

        // Always show report path
        let _ = tx.send(AgentEvent::Log(
            LogLevel::Success,
            format!("📄 Report generated → {}", suite.report_path.display())
        ));

        // Failures detected?
        let failures = suite.failures.clone();
        if failures.is_empty() {
            let _ = tx.send(AgentEvent::Log(LogLevel::Success, "All tests passing ✔".into()));
            let _ = tx.send(AgentEvent::SpinnerStop);
            let _ = tx.send(AgentEvent::Finished);
            return;
        }

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Warn,
            format!("{} failing tests detected", failures.len())
        ));

        // PROCESS EACH FAILURE
        for f in failures {
            if cancelled() {
                let _ = tx.send(AgentEvent::Failed("cancelled".into()));
                return;
            }

            // Construct failure info
            let failure = FailureInfo {
                test_file: f.file.clone(),
                test_name: format!("{}::{}", f.file, f.test),
                traceback: f.output.clone(),
            };

            let cache_key = failure.test_name.clone();
            let display_name = short_dir_path(&cache_key);

            // CACHE FAST-PATH
            if let Some(entry) = full_cache.lock().unwrap().get(&cache_key) {
                if entry.passed {
                    let _ = tx.send(AgentEvent::Log(
                        LogLevel::Info,
                        format!("Skipping {} (already fixed)", display_name)
                    ));
                    continue;
                }
            }

            let _ = tx.send(AgentEvent::Log(
                LogLevel::Info,
                format!("Processing test: {}", display_name)
            ));

            // BUILD CONTEXT
            let ctx = match build_full_suite_context(&repo_root, &failure) {
                Some(c) => c,
                None => {
                    let _ = tx.send(AgentEvent::Log(
                        LogLevel::Error,
                        format!("Context build failed for {}", display_name)
                    ));
                    continue;
                }
            };

            let mut last_test: Option<String> = None;
            let mut last_error: Option<String> = None;

            // RETRY LOOP
            for attempt in 1..=MAX_LLM_RETRIES {
                if cancelled() {
                    let _ = tx.send(AgentEvent::Failed("cancelled".into()));
                    return;
                }

                // Force reload only after attempt 1
                let force_flag = if attempt == 1 {
                    run_options.force_reload
                } else {
                    true
                };

                let _ = tx.send(AgentEvent::Log(
                    LogLevel::Info,
                    format!(
                        "LLM attempt {attempt}/{MAX_LLM_RETRIES} (force_reload={force_flag})"
                    )
                ));

                // fresh prompt
                let mut prompt = build_full_suite_prompt(&ctx);

                // Add feedback on retries
                if attempt > 1 {
                    if let (Some(prev), Some(err)) = (&last_test, &last_error) {
                        prompt.user.push_str("\n# Previous attempt code:\n");
                        prompt.user.push_str(prev);

                        prompt.user.push_str("\n# Failure output:\n");
                        prompt.user.push_str(err);

                        prompt.user.push_str("\n# Fix the above issues.\n");
                    }
                }

                // LLM CALL
                let out = match llm.run(prompt.clone(), force_flag) {
                    Ok(r) => r.text,
                    Err(e) => {
                        last_error = Some(e);
                        continue;
                    }
                };

                let cleaned = sanitize_llm_output(&out);
                last_test = Some(cleaned.clone());

                // WRITE FIXED TEST
                let write_path = match materialize_full_suite_test(
                    &repo_root,
                    &ctx.test_path,
                    &cleaned
                ) {
                    Ok(p) => p,
                    Err(e) => {
                        last_error = Some(format!("file write error: {}", e));
                        continue;
                    }
                };

                // RUN ONLY THIS TEST
                let result = run_test(TestRunRequest::Python {
                    test_path: write_path.clone(),
                });

                match result {
                    TestResult::Passed => {
                        let _ = tx.send(AgentEvent::Log(
                            LogLevel::Success,
                            format!("✅ Fixed: {}", display_name)
                        ));

                        full_cache.lock().unwrap().insert(FullSuiteCacheEntry {
                            test_name: cache_key.clone(),
                            test_path: write_path,
                            last_generated_test: cleaned,
                            passed: true,
                        });

                        break;
                    }
                    TestResult::Failed { output } => {
                        last_error = Some(trim_error(&output));

                        let _ = tx.send(AgentEvent::Log(
                            LogLevel::Warn,
                            format!("Retry needed: {}", display_name)
                        ));
                    }
                }
            }
        }

        // FINAL FULL SUITE VALIDATION
        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            "Re-running full suite for final validation…".into()
        ));

        match run_test_suite_and_report(language, &repo_root) {
            Ok(s) => {
                if s.passed {
                    let _ = tx.send(AgentEvent::Log(
                        LogLevel::Success,
                        "All tests passing ✔".into()
                    ));
                    let _ = tx.send(AgentEvent::SpinnerStop);
                    let _ = tx.send(AgentEvent::Finished);
                } else {
                    let _ = tx.send(AgentEvent::Failed(
                        format!("{} tests still failing (see report)", s.failures.len())
                    ));
                }
            }
            Err(e) => {
                let _ = tx.send(AgentEvent::Failed(format!("final suite error: {}", e)));
            }
        }
    });
}

pub fn run_parallel_agent_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    llm: LlmBackend,
    repo_root: PathBuf,
    snapshot: Arc<FullContextSnapshot>,
    candidates: Vec<TestCandidate>,
    language: Language,
    run_options: AgentRunOptions,
) {
    thread::spawn(move || {
        let cancelled = || cancel_flag.load(Ordering::SeqCst);
        let total = candidates.len();

        tx.send(AgentEvent::Log(
            LogLevel::Info,
            format!("Agent flow started – {total} subagents scheduled"),
        )).ok();

        tx.send(AgentEvent::SpinnerStart(
            format!("Running {total} subagents…"),
        )).ok();

        // SPAWN SUBAGENTS
        let handles: Vec<_> = candidates
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, cand)| {
                let tx2 = tx.clone();
                let cf2 = cancel_flag.clone();
                let llm2 = llm.clone();
                let snap2 = snapshot.clone();
                let opts = run_options.clone();
                let root2 = repo_root.clone();

                thread::spawn(move || {
                    run_single_subagent(
                        tx2, cf2, llm2, root2, snap2,
                        cand, language, opts, i, total
                    )
                })
            })
            .collect();

        // COLLECT RESULTS
        let mut validated_tests: Vec<(TestCandidate, String)> = Vec::new();

        for (i, h) in handles.into_iter().enumerate() {
            if cancelled() { break; }

            match h.join() {
                Ok((true, Some(test_code))) => {
                    let cand = candidates[i].clone();
                    tx.send(AgentEvent::Log(
                        LogLevel::Success,
                        format!("Validated ⇒ {}", short_dir_path(&cand.file)),
                    )).ok();

                    validated_tests.push((cand, test_code));
                }
                Ok((true, None)) => {
                    tx.send(AgentEvent::Log(
                        LogLevel::Warn,
                        "Subagent returned success but no test".into(),
                    )).ok();
                }
                Ok((false, _)) => {
                    tx.send(AgentEvent::Log(
                        LogLevel::Warn,
                        "Subagent failed".into(),
                    )).ok();
                }
                Err(_) => {
                    tx.send(AgentEvent::Log(
                        LogLevel::Error,
                        "Subagent thread panicked".into(),
                    )).ok();
                }
            }
        }

        // MATERIALIZE TESTS (REAL REPO)
        for (cand, code) in validated_tests {
            if cancelled() { break; }

            tx.send(AgentEvent::Log(
                LogLevel::Info,
                t("", &format!("Materializing test → {}", short_dir_path(&cand.file))),
            )).ok();

            match materialize_test(&repo_root, language, &cand, &code) {
                Ok(path) => {
                    tx.send(AgentEvent::Log(
                        LogLevel::Success,
                        t("", &format!("Wrote test {}", path.display())),
                    )).ok();
                }
                Err(e) => {
                    tx.send(AgentEvent::Log(
                        LogLevel::Error,
                        t("", &format!("Failed writing test: {}", e)),
                    )).ok();
                }
            }
        }

        // RUN FULL SUITE
        tx.send(AgentEvent::Log(
            LogLevel::Info,
            "Running full test suite…".into(),
        )).ok();

        match run_test_suite_and_report(language, &repo_root) {
            Ok(suite) => {
                if suite.passed {
                    tx.send(AgentEvent::Log(
                        LogLevel::Success,
                        "🏁 All tests passing".into(),
                    )).ok();
                } else {
                    tx.send(AgentEvent::Log(
                        LogLevel::Warn,
                        format!("{} tests failing", suite.failures.len()),
                    )).ok();
                }
            }
            Err(e) => {
                tx.send(AgentEvent::Log(
                    LogLevel::Error,
                    format!("Full suite error: {}", e),
                )).ok();
            }
        }

        tx.send(AgentEvent::SpinnerStop).ok();
        tx.send(AgentEvent::Finished).ok();
    });
}


fn run_single_subagent(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    llm: LlmBackend,
    repo_root: PathBuf,
    snapshot: Arc<FullContextSnapshot>,
    candidate: TestCandidate,
    language: Language,
    run_options: AgentRunOptions,
    index: usize,
    total: usize,
) -> (bool, Option<String>) {

    let started = Instant::now();
    let prefix = format!("[{}/{}]", index + 1, total);

    tx.send(AgentEvent::Log(
        LogLevel::Info,
        t(&prefix, &format!("Subagent start for {}", short_dir_path(&candidate.file))),
    )).ok();

    // SANDBOX WORKTREE
    let sandbox = git_create_sandbox_worktree();
    tx.send(AgentEvent::Log(
        LogLevel::Info,
        t(&prefix, &format!("Sandbox created at {}", sandbox.display())),
    )).ok();

    let cancelled = || cancel_flag.load(Ordering::SeqCst);

    // CARRY RETRY ERROR & LAST TEST ATTEMPT
    let mut last_test_code: Option<String> = None;
    let mut last_error: Option<String> = None;

    for attempt in 1..=MAX_LLM_RETRIES {
        if cancelled() {
            cleanup_sandbox(&sandbox);
            tx.send(AgentEvent::Log(LogLevel::Warn, t(&prefix, "Cancelled"))).ok();
            return (false, None);
        }

        tx.send(AgentEvent::Log(
            LogLevel::Info,
            t(&prefix, &format!("LLM attempt {attempt}/{MAX_LLM_RETRIES}")),
        )).ok();

        // FILE CONTEXT
        let file_ctx = match snapshot.code.files.iter().find(|f| f.path == candidate.file) {
            Some(c) => c,
            None => {
                cleanup_sandbox(&sandbox);
                tx.send(AgentEvent::Log(
                    LogLevel::Error,
                    t(&prefix, "ERROR: no file context found"),
                )).ok();
                return (false, None);
            }
        };

        // ENRICH CANDIDATE WITH EXISTING new_code
        let mut enriched = candidate.clone();
        if enriched.new_code.is_none() {
            enriched.new_code = candidate.old_code.clone();
        }

        // BUILD PROMPT
        let prompt = if let (Some(prev), Some(err)) = (&last_test_code, &last_error) {
            build_prompt_with_feedback(&enriched, file_ctx, &snapshot.tests, prev, err)
        } else {
            build_prompt(&enriched, file_ctx, &snapshot.tests)
        };

        // LLM EXECUTION
        let out = match llm.run(prompt, run_options.force_reload && attempt == 1) {
            Ok(r) => r.text,
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        };

        let cleaned = sanitize_llm_output(&out);
        if cleaned.is_empty() {
            last_error = Some("empty llm output".into());
            continue;
        }

        last_test_code = Some(cleaned.clone());

        // MATERIALIZE IN SANDBOX ONLY
        let test_path = match materialize_test(&sandbox, language, &candidate, &cleaned) {
            Ok(p) => p,
            Err(e) => {
                last_error = Some(e.to_string());
                continue;
            }
        };

        // RUN TEST IN SANDBOX
        let req = match language {
            Language::Python => TestRunRequest::Python { test_path },
            Language::Rust => TestRunRequest::Rust,
            _ => return (false, None),
        };

        match run_test(req) {
            TestResult::Passed => {
                cleanup_sandbox(&sandbox);

                let elapsed = started.elapsed().as_secs_f32();

                tx.send(AgentEvent::Log(
                    LogLevel::Success,
                    t(&prefix, &format!("Test passed in sandbox ({elapsed:.2}s)")),
                )).ok();

                // Return ONLY the cleaned test code
                return (true, Some(cleaned));
            }

            TestResult::Failed { output } => {
                last_error = Some(trim_error(&output));
            }
        }
    }

    // ALL RETRIES FAILED
    cleanup_sandbox(&sandbox);

    tx.send(AgentEvent::Log(
        LogLevel::Error,
        t(&prefix, "Exhausted retries"),
    )).ok();

    if let Some(err) = last_error {
        tx.send(AgentEvent::Log(
            LogLevel::Warn,
            t(&prefix, &format!("Final error: {}", err)),
        )).ok();
    }

    (false, None)
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
            s = s[i+1..].to_string();
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

pub fn short_dir_path(path: &str) -> String {
    let p = std::path::Path::new(path);

    // Extract only components (dir1, dir2, file)
    let components: Vec<_> = p.components().collect();
    if components.len() <= 2 {
        return path.to_string(); // nothing to compress
    }

    // Extract the last filename
    let file = match p.file_name().and_then(|s| s.to_str()) {
        Some(f) => f,
        None => path,
    };

    // Extract first directory (root)
    let first = match components[0].as_os_str().to_str() {
        Some(s) => s,
        None => path,
    };

    // Format: root/…/filename.py
    format!("{}/…/{}", first, file)
}


fn timestamp() -> String {
    static START: std::sync::OnceLock<SystemTime> = std::sync::OnceLock::new();
    let start = START.get_or_init(SystemTime::now);

    let elapsed = start.elapsed().unwrap_or_default();
    let secs = elapsed.as_secs_f32();
    format!("[{:.1}s]", secs)
}

fn t(prefix: &str, msg: &str) -> String {
    format!("{} {} {}", timestamp(), prefix, msg)
}


fn cleanup_sandbox(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}
