//! orchestrator.rs
//!
//! Async agent execution runner.
//!
//! Guarantees:
//! - Runs fully in background thread
//! - Never blocks UI loop
//! - Cooperative cancellation
//! - Deterministic event ordering

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::thread;
use std::time::Instant;

use crate::detectors::{framework::TestFramework, language::Language};
use crate::llm::ollama::Ollama;
use crate::llm::prompt::build_prompt;
use crate::state::{AgentEvent, LogLevel};
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

        // ---------- helpers ----------
        let cancelled = || cancel_flag.load(Ordering::SeqCst);

        let cancel_and_exit = |reason: &str| {
            let _ = tx.send(AgentEvent::Log(
                LogLevel::Warn,
                format!(" {}", reason),
            ));
            let _ = tx.send(AgentEvent::SpinnerStop);
            let _ = tx.send(AgentEvent::Failed("Execution cancelled".into()));
        };

        // ---------- start ----------
        let _ = tx.send(AgentEvent::SpinnerStart(
            " AI generating tests…".into(),
        ));

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            " Agent execution started".into(),
        ));

        if cancelled() {
            cancel_and_exit("Cancellation detected before execution");
            return;
        }

        // ---------- resolve ----------
        let resolution = resolve_test(&language, &candidate);

        if cancelled() {
            cancel_and_exit("Cancelled during test resolution");
            return;
        }

        // ---------- prompt ----------
        let prompt = build_prompt(
            &candidate,
            &resolution,
            Some(&language),
            framework.as_ref(),
        );

        if cancelled() {
            cancel_and_exit("Cancelled before LLM invocation");
            return;
        }

        // ---------- LLM ----------
        let code = match Ollama::run(prompt) {
            Ok(code) => code,
            Err(e) => {
                let _ = tx.send(AgentEvent::SpinnerStop);
                let _ = tx.send(AgentEvent::Failed(e.to_string()));
                return;
            }
        };

        if cancelled() {
            cancel_and_exit("Cancelled after LLM response");
            return;
        }

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Success,
            " AI returned generated test".into(),
        ));

        // ---------- materialize ----------
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
            format!(" Test written to {}", path.display()),
        ));

        let _ = tx.send(AgentEvent::GeneratedTest(code));


        // ---------- finish ----------
        let elapsed = started.elapsed().as_secs_f32();

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            format!(" Finished in {:.2}s", elapsed),
        ));

        let _ = tx.send(AgentEvent::SpinnerStop);
        let _ = tx.send(AgentEvent::Finished);
    });
}
