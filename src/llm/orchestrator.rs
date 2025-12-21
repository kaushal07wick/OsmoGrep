//! orchestrator.rs
//!
//! Async agent execution runner.
//!
//! Owns:
//! - cancellation
//! - prompt construction
//! - LLM invocation
//! - test materialization
//! - test execution
//!
//! Does NOT index repos or manage UI state.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::thread;
use std::time::Instant;

use crate::context::engine::ContextEngine;
use crate::context::types::IndexHandle;
use crate::executor::run::run_single_test;
use crate::llm::ollama::Ollama;
use crate::llm::prompt::build_prompt;
use crate::state::{AgentEvent, LogLevel, TestDecision, TestResult};
use crate::testgen::candidate::TestCandidate;
use crate::testgen::file::materialize_test;
use crate::testgen::resolve::resolve_test;

pub fn run_llm_test_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    context_index: IndexHandle,
    candidate: TestCandidate,
) {
    thread::spawn(move || {
        let started = Instant::now();

        /* ================= Abort helpers ================= */

        let abort = |msg: &str| {
            let _ = tx.send(AgentEvent::Log(LogLevel::Warn, msg.into()));
            let _ = tx.send(AgentEvent::SpinnerStop);
            let _ = tx.send(AgentEvent::Failed(msg.into()));
        };

        let cancelled = || cancel_flag.load(Ordering::SeqCst);

        let cancel_and_exit = |reason: &str| {
            let _ = tx.send(AgentEvent::Log(LogLevel::Warn, reason.into()));
            let _ = tx.send(AgentEvent::SpinnerStop);
            let _ = tx.send(AgentEvent::Failed(reason.into()));
        };

        /* ================= Pre-flight ================= */

        if matches!(candidate.decision, TestDecision::No) {
            abort("Change is not AI-test-worthy. Agent aborted.");
            return;
        }

        if candidate.old_code.is_none() || candidate.new_code.is_none() {
            abort("No executable code delta found. Agent aborted.");
            return;
        }

        /* ================= Wait for context ================= */

        let (facts, symbols) = loop {
            if cancelled() {
                cancel_and_exit("Cancelled while waiting for context index");
                return;
            }

            let status = context_index.status.read().unwrap().clone();

            match status {
                crate::context::types::IndexStatus::Ready => {
                    let facts = context_index.facts.read().unwrap().clone();
                    let symbols = context_index.symbols.read().unwrap().clone();

                    match (facts, symbols) {
                        (Some(f), Some(s)) => break (f, s),
                        _ => abort("Context index incomplete"),
                    }
                }

                crate::context::types::IndexStatus::Failed(err) => {
                    abort(&format!("Context indexing failed: {}", err));
                    return;
                }

                crate::context::types::IndexStatus::Indexing => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        };

        let ctx_engine = ContextEngine::new(&facts, &symbols);
        let ctx_slice = ctx_engine.slice_for(&candidate.target);

        /* ================= Start ================= */

        let _ = tx.send(AgentEvent::SpinnerStart(" AI generating tests…".into()));
        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            format!(
                "Agent running for {}{}",
                candidate.file,
                candidate
                    .symbol
                    .as_ref()
                    .map(|s| format!(" :: {}", s))
                    .unwrap_or_default()
            ),
        ));

        if cancelled() {
            cancel_and_exit("Cancelled before execution");
            return;
        }

        /* ================= Resolve ================= */

        let resolution = resolve_test(&facts.languages[0], &candidate);

        if cancelled() {
            cancel_and_exit("Cancelled during test resolution");
            return;
        }

        /* ================= Prompt ================= */

        let prompt = build_prompt(&candidate, &resolution, &ctx_slice);

        /* ================= LLM ================= */

        let code = match Ollama::run(prompt) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(AgentEvent::SpinnerStop);
                let _ = tx.send(AgentEvent::Failed(e.to_string()));
                return;
            }
        };

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Success,
            "AI returned generated test".into(),
        ));

        /* ================= Materialize ================= */

        let language = facts.languages[0];

        let path = match materialize_test(language, &candidate, &resolution, &code) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(AgentEvent::SpinnerStop);
                let _ = tx.send(AgentEvent::Failed(e.to_string()));
                return;
            }
        };

        let _ = tx.send(AgentEvent::GeneratedTest(code.clone()));
        let _ = tx.send(AgentEvent::Log(
            LogLevel::Success,
            format!("Test written to {}", path.display()),
        ));

        if cancelled() {
            cancel_and_exit("Cancelled before test execution");
            return;
        }

        /* ================= Execute ================= */

        let cmd: Vec<&str> = match language {
            crate::detectors::language::Language::Rust => {
                vec!["cargo", "test", "--quiet"]
            }
            crate::detectors::language::Language::Python => {
                vec!["pytest", "-q"]
            }
            _ => {
                abort("Unsupported language for execution");
                return;
            }
        };

        let _ = tx.send(AgentEvent::TestStarted);
        let result = run_single_test(&cmd);
        let _ = tx.send(AgentEvent::TestFinished(result.clone()));

        match result {
            TestResult::Passed => {
                let _ = tx.send(AgentEvent::Log(
                    LogLevel::Success,
                    "🧪 Test passed".into(),
                ));
            }

            TestResult::Failed { output } => {
                let _ = tx.send(AgentEvent::Log(
                    LogLevel::Error,
                    format!("🧪 Test failed:\n{}", output),
                ));
                let _ = tx.send(AgentEvent::SpinnerStop);
                let _ = tx.send(AgentEvent::Failed("Test execution failed".into()));
                return;
            }
        }

        /* ================= Finish ================= */

        let elapsed = started.elapsed().as_secs_f32();
        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            format!("Finished in {:.2}s", elapsed),
        ));

        let _ = tx.send(AgentEvent::SpinnerStop);
        let _ = tx.send(AgentEvent::Finished);
    });
}
