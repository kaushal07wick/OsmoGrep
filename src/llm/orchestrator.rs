// src/orchestrator.rs
//
// TEMPORARY: Context preview runner.
// Builds ContextSlice and prints FULL context.
// LLM + execution are intentionally disabled.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::thread;

use crate::context::{engine::ContextEngine, types::ContextSlice};
use crate::context::types::{IndexHandle, IndexStatus};
use crate::state::{AgentEvent, LogLevel, TestDecision};
use crate::testgen::candidate::TestCandidate;

pub fn run_llm_test_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    context_index: IndexHandle,
    candidate: TestCandidate,
) {
    thread::spawn(move || {
        let cancelled = || cancel_flag.load(Ordering::SeqCst);

        let fail = |msg: &str| {
            let _ = tx.send(AgentEvent::Log(LogLevel::Warn, msg.into()));
            let _ = tx.send(AgentEvent::Failed(msg.into()));
        };

        /* ---------- pre-flight ---------- */

        if matches!(candidate.decision, TestDecision::No) {
            fail("Change is not AI-test-worthy.");
            return;
        }

        /* ---------- wait for context ---------- */

        let (facts, symbols) = loop {
            if cancelled() {
                fail("Cancelled while waiting for context index");
                return;
            }

            match context_index.status.read().unwrap().clone() {
                IndexStatus::Ready => {
                    let facts = context_index.facts.read().unwrap().clone();
                    let symbols = context_index.symbols.read().unwrap().clone();

                    match (facts, symbols) {
                        (Some(f), Some(s)) => break (f, s),
                        _ => {
                            fail("Context index incomplete");
                            return;
                        }
                    }
                }

                IndexStatus::Failed(err) => {
                    fail(&format!("Context indexing failed: {}", err));
                    return;
                }

                IndexStatus::Indexing => {
                    thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        };

        /* ---------- build context slice ---------- */

        let ctx_engine = ContextEngine::new(&facts, &symbols);
        let ctx_slice = ctx_engine.slice_for(&candidate.target);

        /* ---------- dump FULL context and exit ---------- */

        let text = debug_context(&ctx_slice);

        let _ = std::fs::write(".osmogrep_context.txt", &text);
        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            "Full context written to .osmogrep_context.txt".into(),
        ));

        // HARD STOP: no LLM, no execution
        let _ = tx.send(AgentEvent::Finished);
    });
}

/* ============================================================
   FULL Context Printer (deterministic, explicit)
   ============================================================ */

fn debug_context(ctx: &ContextSlice) -> String {
    let mut s = String::new();

    s.push_str("=== CONTEXT SLICE ===\n\n");

    /* ---------- TARGET ---------- */

    s.push_str("TARGET:\n");
    s.push_str(&format!(
        "- {} ({}:{})\n\n",
        ctx.target.name,
        ctx.target.file.display(),
        ctx.target.line
    ));

    /* ---------- LOCAL STRUCTURE ---------- */

    s.push_str("LOCAL STRUCTURE:\n");

    if ctx.deps.is_empty() && ctx.imports.is_empty() {
        s.push_str("- None\n\n");
    } else {
        if !ctx.deps.is_empty() {
            s.push_str("- Related symbols:\n");
            for d in &ctx.deps {
                s.push_str(&format!(
                    "  - {} ({}:{})\n",
                    d.name,
                    d.file.display(),
                    d.line
                ));
            }
        }

        if !ctx.imports.is_empty() {
            s.push_str("- Imports:\n");
            for i in &ctx.imports {
                s.push_str(&format!("  - {}\n", i.module));
            }
        }

        s.push('\n');
    }

    /* ---------- REPO FACTS ---------- */

    s.push_str("REPO FACTS:\n");
    s.push_str(&format!(
        "- Languages: {}\n",
        ctx.repo_facts.languages.join(", ")
    ));
    s.push_str(&format!(
        "- Test frameworks: {}\n",
        ctx.repo_facts.test_frameworks.join(", ")
    ));
    if !ctx.repo_facts.forbidden_deps.is_empty() {
        s.push_str(&format!(
            "- Forbidden deps: {}\n",
            ctx.repo_facts.forbidden_deps.join(", ")
        ));
    }
    s.push('\n');

    /* ---------- EXECUTION MODEL ---------- */

    s.push_str("EXECUTION MODEL:\n");
    match ctx.execution_model {
        Some(m) => s.push_str(&format!("- {:?}\n\n", m)),
        None => s.push_str("- Unknown\n\n"),
    }

    /* ---------- TEST SEMANTICS ---------- */

    s.push_str("TEST SEMANTICS:\n");

    match ctx.test_intent {
        Some(t) => s.push_str(&format!("- Test intent: {:?}\n", t)),
        None => s.push_str("- Test intent: Unknown\n"),
    }

    match ctx.assertion_style {
        Some(a) => s.push_str(&format!("- Assertion style: {:?}\n", a)),
        None => s.push_str("- Assertion style: Unknown\n"),
    }

    if ctx.failure_modes.is_empty() {
        s.push_str("- Failure modes: None\n");
    } else {
        s.push_str("- Failure modes:\n");
        for f in &ctx.failure_modes {
            s.push_str(&format!("  - {:?}\n", f));
        }
    }

    s.push('\n');

    /* ---------- TEST ECOSYSTEM ---------- */

    s.push_str("TEST CONTEXT:\n");

    match &ctx.test_context {
        None => {
            s.push_str("- No tests directory found\n\n");
        }
        Some(tc) => {
            if tc.existing_tests.is_empty() {
                s.push_str("- Existing tests: None\n");
            } else {
                s.push_str("- Existing tests:\n");
                for p in &tc.existing_tests {
                    s.push_str(&format!("  - {}\n", p.display()));
                }
            }

            if let Some(p) = &tc.recommended_location {
                s.push_str(&format!(
                    "- Recommended test location: {}\n",
                    p.display()
                ));
            }

            if let Some(o) = &tc.oracle {
                s.push_str(&format!("- Oracle: {:?}\n", o));
            }

            if let Some(io) = &tc.io_contract {
                s.push_str("- IO Contract:\n");
                for i in &io.input_notes {
                    s.push_str(&format!("  input: {}\n", i));
                }
                for o in &io.output_notes {
                    s.push_str(&format!("  output: {}\n", o));
                }
            }

            s.push('\n');
        }
    }

    s
}
