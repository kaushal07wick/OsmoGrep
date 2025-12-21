//! orchestrator.rs
//!
//! TEMPORARY: Context preview runner.
//! Builds ContextSlice and shows it in a single panel.
//! LLM + execution are intentionally disabled.

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
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        };

        /* ---------- build context slice ---------- */

        let ctx_engine = ContextEngine::new(&facts, &symbols);
        let ctx_slice = ctx_engine.slice_for(&candidate.target);

        // TEMP: dump context for inspection
        if let Ok(text) = std::fs::write(
            ".osmogrep_context.txt",
            debug_context(&ctx_slice),
        ) {
            let _ = tx.send(AgentEvent::Log(
                LogLevel::Info,
                "Context written to .osmogrep_context.txt".into(),
            ));
        }
return;


    });
}

/* ============================================================
   Debug formatting
   ============================================================ */

fn debug_context(ctx: &ContextSlice) -> String {
    let mut s = String::new();

    s.push_str("=== CONTEXT SLICE ===\n\n");

    s.push_str(&format!(
        "TARGET:\n- {} ({}:{})\n\n",
        ctx.target.name,
        ctx.target.file.display(),
        ctx.target.line
    ));

    if !ctx.deps.is_empty() {
        s.push_str("RELATED SYMBOLS:\n");
        for d in &ctx.deps {
            s.push_str(&format!(
                "- {} ({}:{})\n",
                d.name,
                d.file.display(),
                d.line
            ));
        }
        s.push('\n');
    }

    if !ctx.imports.is_empty() {
        s.push_str("IMPORTS:\n");
        for i in &ctx.imports {
            s.push_str(&format!("- {}\n", i.module));
        }
        s.push('\n');
    }

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

    s
}
