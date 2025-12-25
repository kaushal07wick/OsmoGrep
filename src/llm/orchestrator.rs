// src/orchestrator.rs

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::thread;
use crate::llm::prompt::build_prompt;
use crate::testgen::resolve::TestResolution;

use crate::context::engine::ContextEngine;
use crate::context::types::{
    ContextSlice,
    IndexHandle,
    IndexStatus,
    SymbolResolution,
};
use crate::state::{AgentEvent, LogLevel, TestDecision};
use crate::testgen::candidate::TestCandidate;

pub fn run_llm_test_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    context_index: IndexHandle,
    candidate: TestCandidate,
    model: String,
) {
    thread::spawn(move || {
        let cancelled = || cancel_flag.load(Ordering::SeqCst);

        let fail = |msg: &str| {
            let _ = tx.send(AgentEvent::Log(LogLevel::Error, msg.into()));
            let _ = tx.send(AgentEvent::Failed(msg.into()));
        };

        if matches!(candidate.decision, TestDecision::No) {
            fail("Change is not AI-test-worthy.");
            return;
        }

        let (repo_root, facts, test_roots) = loop {
            if cancelled() {
                fail("Cancelled while waiting for context index.");
                return;
            }

            let status = {
                context_index.status.read().unwrap().clone()
            };

            match status {
                IndexStatus::Ready => {
                    let facts_guard = context_index.facts.read().unwrap();
                    let test_roots_guard =
                        context_index.test_roots.read().unwrap();

                    let repo_root = context_index.repo_root.clone();

                    match facts_guard.clone() {
                        Some(f) => break (repo_root, f, test_roots_guard.clone()),
                        None => {
                            fail("Repository facts missing.");
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


        let symbols_guard = context_index.symbols.read().unwrap();
        let symbols = symbols_guard.clone().unwrap_or_default();


        let ctx_engine = ContextEngine::new(
            &repo_root,
            &facts,
            &symbols, // ignored internally by design
            &test_roots,
        );

        let ctx_slice = ctx_engine.slice_from_diff(&candidate.diff);

        if ctx_slice.diff_target.file.as_os_str().is_empty() {
            fail("Diff target could not be resolved to a file.");
            return;
        }

        let context_text = debug_context(&ctx_slice);
        let _ = std::fs::write(".osmogrep_context.txt", &context_text);

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            "Context written to .osmogrep_context.txt".into(),
        ));

        /* ---------- build + dump PROMPT ---------- */

        let prompt = build_prompt(
            &candidate,
            &TestResolution::NotFound,
            &ctx_slice,
        );

        let mut prompt_text = String::new();

        prompt_text.push_str("=== SYSTEM PROMPT ===\n");
        prompt_text.push_str(&prompt.system);
        prompt_text.push_str("\n\n=== USER PROMPT ===\n");
        prompt_text.push_str(&prompt.user);

        let _ = std::fs::write(".osmogrep_prompt.txt", &prompt_text);

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            "Prompt written to .osmogrep_prompt.txt".into(),
        ));


        let _ = tx.send(AgentEvent::Finished);
    });
}


fn debug_context(ctx: &ContextSlice) -> String {
    let mut s = String::new();

    s.push_str("=== CONTEXT SLICE ===\n\n");

    /* ---------- DIFF TARGET ---------- */

    s.push_str("DIFF TARGET:\n");
    s.push_str(&format!(
        "- File: {}\n",
        ctx.diff_target.file.display()
    ));
    if let Some(sym) = &ctx.diff_target.symbol {
        s.push_str(&format!("- Symbol (from diff): {}\n", sym));
    }
    s.push('\n');

    /* ---------- SYMBOL RESOLUTION ---------- */

    s.push_str("SYMBOL RESOLUTION:\n");
    match &ctx.symbol_resolution {
        SymbolResolution::Resolved(sym) => {
            s.push_str(&format!(
                "- Resolved: {} (line {})\n\n",
                sym.name, sym.line
            ));
        }
        SymbolResolution::Ambiguous(matches) => {
            s.push_str("- Ambiguous matches:\n");
            for m in matches {
                s.push_str(&format!(
                    "  - {} (line {})\n",
                    m.name, m.line
                ));
            }
            s.push('\n');
        }
        SymbolResolution::NotFound => {
            s.push_str("- NOT FOUND in file\n\n");
        }
    }


    s.push_str("LOCAL STRUCTURE (same file):\n");
    if ctx.local_symbols.is_empty() && ctx.imports.is_empty() {
        s.push_str("- None\n\n");
    } else {
        for d in &ctx.local_symbols {
            s.push_str(&format!(
                "  - {} (line {})\n",
                d.name,
                d.line
            ));
        }
        for i in &ctx.imports {
            s.push_str(&format!("  - import {}\n", i.module));
        }
        s.push('\n');
    }

    s.push_str("TEST CONTEXT:\n");
    s.push_str(&format!(
        "- Match kind: {:?}\n",
        ctx.test_context.match_kind
    ));

    if ctx.test_context.existing_tests.is_empty() {
        s.push_str("- Existing tests: None\n");
    } else {
        for p in &ctx.test_context.existing_tests {
            s.push_str(&format!("  - {}\n", p.display()));
        }
    }

    s.push_str(&format!(
        "- Recommended location: {}\n",
        ctx.test_context.recommended_location.display()
    ));

    s
}
