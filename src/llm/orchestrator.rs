use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::thread;

use crate::context::engine::ContextEngine;
use crate::context::types::{ContextSlice, IndexHandle, IndexStatus, SymbolResolution};
use crate::detectors::language::Language;
use crate::executor::run::run_single_test;
use crate::llm::{ollama::Ollama, prompt::build_prompt};
use crate::state::{AgentEvent, LogLevel, TestDecision};
use crate::testgen::candidate::TestCandidate;
use crate::testgen::file::materialize_test;
use crate::testgen::resolve::TestResolution;

pub fn run_llm_test_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    context_index: IndexHandle,
    candidate: TestCandidate,
    language: Language,
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

        /* ================= CONTEXT ================= */

        let (repo_root, facts, test_roots) = loop {
            if cancelled() {
                fail("Cancelled while waiting for context index.");
                return;
            }

            match context_index.status.read().unwrap().clone() {
                IndexStatus::Ready => {
                    let facts = context_index.facts.read().unwrap().clone();
                    let roots = context_index.test_roots.read().unwrap().clone();
                    match facts {
                        Some(f) => break (context_index.repo_root.clone(), f, roots),
                        None => {
                            fail("Repository facts missing.");
                            return;
                        }
                    }
                }
                IndexStatus::Failed(err) => {
                    fail(&format!("Context indexing failed: {err}"));
                    return;
                }
                IndexStatus::Indexing => {
                    thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        };

        let symbols = context_index
            .symbols
            .read()
            .unwrap()
            .clone()
            .unwrap_or_default();

        let ctx_engine = ContextEngine::new(
            &repo_root,
            &facts,
            &symbols,
            &test_roots,
        );

        let ctx_slice = ctx_engine.slice_from_diff(&candidate.diff);

        if ctx_slice.diff_target.file.as_os_str().is_empty() {
            fail("Diff target could not be resolved to a file.");
            return;
        }

        /* ================= CONTEXT DUMP ================= */

        let context_text = debug_context(&ctx_slice);
        let _ = std::fs::write(".osmogrep_context.txt", &context_text);

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            "Context written to .osmogrep_context.txt".into(),
        ));

        /* ================= PROMPT ================= */

        let prompt = build_prompt(
            &candidate,
            &TestResolution::NotFound,
            &ctx_slice,
        );

        let prompt_text = format!(
            "=== SYSTEM PROMPT ===\n{}\n\n=== USER PROMPT ===\n{}",
            prompt.system, prompt.user
        );

        let _ = std::fs::write(".osmogrep_prompt.txt", &prompt_text);

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            "Prompt prepared. Calling LLM…".into(),
        ));

        /* ================= LLM EXECUTION ================= */

        let llm_handle = {
            let tx = tx.clone();
            let model = model.clone();
            let prompt = prompt.clone();

            thread::spawn(move || {
                let _ = tx.send(AgentEvent::Log(
                    LogLevel::Info,
                    "LLM generation started".into(),
                ));

                let result = Ollama::run(prompt, &model);

                match result {
                    Ok(text) => {
                        let _ = tx.send(AgentEvent::Log(
                            LogLevel::Success,
                            "LLM generation completed".into(),
                        ));
                        Ok(text)
                    }
                    Err(e) => {
                        let _ = tx.send(AgentEvent::Log(
                            LogLevel::Error,
                            format!("LLM error: {e}"),
                        ));
                        Err(e)
                    }
                }
            })
        };

        let generated_test = match llm_handle.join() {
            Ok(Ok(out)) => out,
            _ => {
                fail("LLM execution failed.");
                return;
            }
        };

        /* ================= MATERIALIZE TEST ================= */

        let test_path = match materialize_test(
            language,
            &candidate,
            &TestResolution::NotFound,
            &generated_test,
        ) {
            Ok(p) => p,
            Err(e) => {
                fail(&format!("Failed to write test: {e}"));
                return;
            }
        };

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            format!("Test written to {}", test_path.display()),
        ));

        /* ================= RUN TEST ================= */

        let cmd: Vec<&str> = match language {
            Language::Python => vec!["pytest", test_path.to_str().unwrap()],
            Language::Rust => vec!["cargo", "test"],
            _ => {
                fail("Unsupported language");
                return;
            }
        };

        match run_single_test(&cmd) {
            crate::state::TestResult::Passed => {
                let _ = tx.send(AgentEvent::TestFinished(
                    crate::state::TestResult::Passed,
                ));
            }
            crate::state::TestResult::Failed { output } => {
                let _ = tx.send(AgentEvent::TestFinished(
                    crate::state::TestResult::Failed { output },
                ));
            }
        }

        let _ = tx.send(AgentEvent::Finished);
    });
}

/* ================= CONTEXT DUMP ================= */

fn debug_context(ctx: &ContextSlice) -> String {
    let mut s = String::new();

    s.push_str("=== CONTEXT SLICE ===\n\n");

    s.push_str("DIFF TARGET:\n");
    s.push_str(&format!(
        "- File: {}\n",
        ctx.diff_target.file.display()
    ));
    if let Some(sym) = &ctx.diff_target.symbol {
        s.push_str(&format!("- Symbol (from diff): {}\n", sym));
    }
    s.push('\n');

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
                s.push_str(&format!("  - {} (line {})\n", m.name, m.line));
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
            s.push_str(&format!("  - {} (line {})\n", d.name, d.line));
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
