use std::{path::PathBuf, sync::{
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
use crate::git::{git_create_sandbox_worktree, git_compute_patch};

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
            let display_name = short_test_name(&cache_key);

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
        let _ = tx.send(AgentEvent::SpinnerStart(format!("Running {total} subagents…")));

        let results = candidates
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let tx_clone = tx.clone();
                let cf = cancel_flag.clone();
                let llm_clone = llm.clone();
                let snap = snapshot.clone();
                let opts = run_options.clone();
                let root = repo_root.clone();

                thread::spawn(move || {
                    run_single_subagent(
                        tx_clone,
                        cf,
                        llm_clone,
                        root,
                        snap.clone(),  // Arc clone is cheap
                        c,
                        language,
                        opts,
                        i,
                        total,
                    )
                })
            })
            .collect::<Vec<_>>();

        let mut patches = Vec::new();

        for handle in results {
            if cancelled() { break; }
            if let Ok((ok, patch_opt)) = handle.join() {
                if ok {
                    if let Some(p) = patch_opt {
                        patches.push(p);
                    }
                }
            }
        }

        for (i, patch) in patches.iter().enumerate() {
            if cancelled() { break; }

            let tmp = std::env::temp_dir().join(format!("_subpatch_{i}.diff"));
            if std::fs::write(&tmp, patch).is_ok() {
                let status = std::process::Command::new("git")
                    .current_dir(&repo_root)
                    .arg("apply")
                    .arg(tmp.to_str().unwrap())
                    .status();

                match status {
                    Ok(s) if s.success() => {
                        let _ = tx.send(AgentEvent::Log(LogLevel::Success,
                            format!("Patch {i} applied")));
                    }
                    _ => {
                        let _ = tx.send(AgentEvent::Log(LogLevel::Warn,
                            format!("Patch {i} conflict, skipping")));
                    }
                }
            }
        }

        match run_test_suite_and_report(language, &repo_root) {
            Ok(suite) => {
                if suite.passed {
                    let _ = tx.send(AgentEvent::Log(LogLevel::Success,
                        "All tests passing ✔".into()));
                } else {
                    let _ = tx.send(AgentEvent::Log(LogLevel::Warn,
                        format!("{} tests still failing", suite.failures.len())));
                }
            }
            Err(e) => {
                let _ = tx.send(AgentEvent::Log(LogLevel::Error,
                    format!("Final suite error: {}", e)));
            }
        }

        let _ = tx.send(AgentEvent::SpinnerStop);
        let _ = tx.send(AgentEvent::Finished);
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

    let cancelled = || cancel_flag.load(Ordering::SeqCst);
    let prefix = format!("[{}/{}]", index + 1, total);

    let _ = tx.send(AgentEvent::Log(LogLevel::Info,
        format!("{prefix} Subagent starting ({})", candidate.file)));

    let sandbox = git_create_sandbox_worktree();

    let mut last_test: Option<String> = None;
    let mut last_error: Option<String> = None;

    for attempt in 1..=MAX_LLM_RETRIES {
        if cancelled() {
            let _ = tx.send(AgentEvent::Log(LogLevel::Warn,
                format!("{prefix} Cancelled")));
            return (false, None);
        }

        let _ = tx.send(AgentEvent::Log(LogLevel::Info,
            format!("{prefix} LLM attempt {attempt}/{MAX_LLM_RETRIES}")));

        let file_ctx = match snapshot.code.files.iter()
            .find(|f| f.path == candidate.file)
        {
            Some(c) => c,
            None => {
                let _ = tx.send(AgentEvent::Log(LogLevel::Error,
                    format!("{prefix} No file context")));
                return (false, None);
            }
        };

        let prompt = match (&last_test, &last_error) {
            (Some(code), Some(err)) => build_prompt_with_feedback(
                &candidate, file_ctx, &snapshot.tests, code, err
            ),
            _ => build_prompt(&candidate, file_ctx, &snapshot.tests),
        };

        let out = match llm.run(prompt, run_options.force_reload && attempt == 1) {
            Ok(r) => r.text,
            Err(e) => {
                last_error = Some(e.clone());
                continue;
            }
        };

        let cleaned = sanitize_llm_output(&out);
        if cleaned.is_empty() {
            last_error = Some("empty llm output".into());
            continue;
        }

        last_test = Some(cleaned.clone());

        let test_path = match materialize_test(&sandbox, language, &candidate, &cleaned) {
            Ok(p) => p,
            Err(e) => {
                last_error = Some(e.to_string());
                continue;
            }
        };

        let req = match language {
        Language::Python => TestRunRequest::Python { test_path },
        Language::Rust => TestRunRequest::Rust,
        Language::Unknown => {
            let _ = tx.send(AgentEvent::Log(
                LogLevel::Error,
                format!("{prefix} Unsupported language"),
            ));
            return (false, None);
        }
    };


        let result = run_test(req);

        match result {
            TestResult::Passed => {
                let _ = tx.send(AgentEvent::Log(LogLevel::Success,
                    format!("{prefix} Test passed")));
                let patch = git_compute_patch(&sandbox);
                return (true, Some(patch));
            }
            TestResult::Failed { output } => {
                last_error = Some(trim_error(&output));
            }
        }
    }

    let _ = tx.send(AgentEvent::Log(LogLevel::Error,
        format!("{prefix} Exhausted retries")));

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

fn short_test_name(name: &str) -> String {
    const MAX: usize = 80;
    if name.len() <= MAX {
        return name.to_string();
    }
    let start = &name[..40];
    let end = &name[name.len() - 30..];
    format!("{}..{}", start, end)
}