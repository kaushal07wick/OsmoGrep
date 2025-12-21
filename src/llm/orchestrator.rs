//! orchestrator.rs
//!
//! Async agent execution runner.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::thread;
use std::time::Instant;

use crate::detectors::{framework::TestFramework, language::Language};
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
    language: Language,
    framework: Option<TestFramework>,
    candidate: TestCandidate,
) {
    thread::spawn(move || {
        let started = Instant::now();

        /* ================= HARD PRE-FLIGHT ================= */

        let abort = |msg: &str| {
            let _ = tx.send(AgentEvent::Log(LogLevel::Warn, msg.into()));
            let _ = tx.send(AgentEvent::SpinnerStop);
            let _ = tx.send(AgentEvent::Failed(msg.into()));
        };

        if matches!(candidate.decision, TestDecision::No) {
            abort("Change is not AI-test-worthy. Agent aborted.");
            return;
        }

        if candidate.old_code.is_none() || candidate.new_code.is_none() {
            abort("No executable code delta found. Agent aborted.");
            return;
        }

        if !matches!(language, Language::Rust | Language::Python) {
            abort("Unsupported language for test execution.");
            return;
        }

        /* ================= HELPERS ================= */

        let cancelled = || cancel_flag.load(Ordering::SeqCst);

        let cancel_and_exit = |reason: &str| {
            let _ = tx.send(AgentEvent::Log(LogLevel::Warn, reason.into()));
            let _ = tx.send(AgentEvent::SpinnerStop);
            let _ = tx.send(AgentEvent::Failed("Execution cancelled".into()));
        };

        /* ================= START ================= */

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

        /* ================= RESOLVE ================= */

        let resolution = resolve_test(&language, &candidate);
        if cancelled() {
            cancel_and_exit("Cancelled during test resolution");
            return;
        }

        /* ================= PROMPT ================= */

        let prompt = build_prompt(
            &candidate,
            &resolution,
            Some(&language),
            framework.as_ref(),
        );

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

        /* ================= MATERIALIZE ================= */

        let path = match materialize_test(language, &candidate, &resolution, &code) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(AgentEvent::SpinnerStop);
                let _ = tx.send(AgentEvent::Failed(e.to_string()));
                return;
            }
        };

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Success,
            format!("Test written to {}", path.display()),
        ));
        let _ = tx.send(AgentEvent::GeneratedTest(code));

        if cancelled() {
            cancel_and_exit("Cancelled before test execution");
            return;
        }

        /* ================= TEST EXECUTION ================= */

        let cmd: Vec<&str> = match language {
            Language::Rust => vec!["cargo", "test", "--quiet"],
            Language::Python => vec!["pytest", "-q"],
            _ => unreachable!(),
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

        /* ================= FINISH ================= */

        let elapsed = started.elapsed().as_secs_f32();
        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            format!("Finished in {:.2}s", elapsed),
        ));

        let _ = tx.send(AgentEvent::SpinnerStop);
        let _ = tx.send(AgentEvent::Finished);
    });
}
