use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::state::AgentState;
use crate::testgen::runner::run_full_test;

#[derive(Debug, Clone)]
pub struct TestCaseResult {
    pub path: PathBuf,
    pub passed: bool,
    pub output: String,
}

pub fn run_full_test_suite(
    repo_root: &Path,
    state: &AgentState,
) -> io::Result<Vec<TestCaseResult>> {
    let language = state.lifecycle.language.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Other,
            "language not detected; cannot run test suite",
        )
    })?;

    let suite = run_full_test(language);

    let mut results = Vec::new();

    for name in suite.passed {
        results.push(TestCaseResult {
            path: repo_root.join(&name),
            passed: true,
            output: String::new(),
        });
    }

    for (name, output) in suite.failed {
        results.push(TestCaseResult {
            path: repo_root.join(&name),
            passed: false,
            output,
        });
    }

    assert!(
    !results.is_empty(),
    "test suite produced no test results"
    );

    Ok(results)
}

pub fn write_test_suite_report(
    repo_root: &Path,
    results: &[TestCaseResult],
) -> io::Result<PathBuf> {
    let report_path = repo_root.join("run.md");
    let mut file = File::create(&report_path)?;

    writeln!(file, "# Test Suite Report\n")?;

    for r in results {
        let status = if r.passed { "PASSED" } else { "FAILED" };
        writeln!(file, "## {}", r.path.display())?;
        writeln!(file, "**Status:** {}\n", status)?;

        if !r.output.trim().is_empty() {
            writeln!(file, "```")?;
            writeln!(file, "{}", r.output)?;
            writeln!(file, "```")?;
        }
    }

    Ok(report_path)
}
