use std::io;
use std::path::PathBuf;

use crate::state::{AgentState, LogLevel};
use crate::testgen::candidate::TestCandidate;
use crate::testgen::resolve::{resolve_test, TestResolution};
use crate::testgen::file::materialize_test;
use crate::llm::prompt::{build_prompt, LlmPrompt};
use crate::llm::runner::{run_tests, TestRunResult};
use crate::llm::client::LlmClient;
use crate::logger::log;

/* ============================================================
   Public entry
   ============================================================ */

pub fn run_llm_test_flow(
    state: &mut AgentState,
    candidate: &TestCandidate,
    llm: &dyn LlmClient,
) -> io::Result<()> {
    log(state, LogLevel::Info, "Resolving existing tests…");

    let resolution = resolve_test(state, candidate);

    log_resolution(state, &resolution);

    /* ---------- PROMPT ---------- */

    let prompt = build_prompt(candidate, &resolution);

    log(state, LogLevel::Info, "Sending prompt to LLM…");

    let test_code = llm.generate(prompt)?;

    /* ---------- WRITE TEST ---------- */

    log(state, LogLevel::Info, "Writing test to filesystem…");

    let test_path =
        materialize_test(state, candidate, &resolution, &test_code)?;

    log(
        state,
        LogLevel::Success,
        format!("Test written to {}", test_path.display()),
    );

    /* ---------- RUN TESTS ---------- */

    log(state, LogLevel::Info, "Running tests…");

    let result = run_tests(state, &test_path)?;

    report_result(state, &result);

    Ok(())
}

/* ============================================================
   Logging helpers
   ============================================================ */

fn log_resolution(state: &mut AgentState, r: &TestResolution) {
    match r {
        TestResolution::Found { file, test_fn } => {
            log(
                state,
                LogLevel::Success,
                format!("Existing test found: {}", file),
            );

            if let Some(name) = test_fn {
                log(
                    state,
                    LogLevel::Info,
                    format!("Target test function: {}", name),
                );
            }
        }

        TestResolution::Ambiguous(paths) => {
            log(
                state,
                LogLevel::Warn,
                "Multiple possible test files found:",
            );
            for p in paths {
                log(state, LogLevel::Warn, format!(" - {}", p));
            }
        }

        TestResolution::NotFound => {
            log(
                state,
                LogLevel::Info,
                "No existing test found, creating new regression test",
            );
        }
    }
}

fn report_result(state: &mut AgentState, r: &TestRunResult) {
    match r {
        TestRunResult::Passed { output } => {
            log(state, LogLevel::Success, "Tests passed");
            if !output.is_empty() {
                log(state, LogLevel::Info, output);
            }
        }

        TestRunResult::Failed { output } => {
            log(state, LogLevel::Error, "Tests failed");
            log(state, LogLevel::Error, output);
        }
    }
}
