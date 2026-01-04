// src/testgen/runner.rs
// Executes test commands and returns raw output + timing.
// No parsing. No interpretation. No intelligence.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use crate::detectors::language::Language;
use crate::state::TestResult;

#[derive(Debug, Clone)]
pub enum TestRunRequest {
    Python { test_path: PathBuf },
    Rust,
}

#[derive(Debug, Clone)]
pub enum TestOutcome {
    Pass,
    Fail,
    Skip,
    Warning,
}

#[derive(Debug, Clone)]
pub struct TestCaseResult {
    pub file: String,
    pub name: String,
    pub outcome: TestOutcome,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TestSuiteResult {
    /// Intentionally empty.
    /// Parsing and interpretation happen in suite.rs only.
    pub cases: Vec<TestCaseResult>,

    /// Wall-clock duration of the test run.
    pub duration_ms: u64,

    /// Full raw stdout + stderr, unparsed.
    pub raw_output: String,
}

pub fn run_test(req: TestRunRequest) -> TestResult {
    let output = match req {
        TestRunRequest::Python { test_path } => {
            Command::new("python")
                .arg("-m")
                .arg("pytest")
                .arg(test_path)
                .env(
                    "PYTHONPATH",
                    std::env::current_dir().unwrap_or_else(|_| ".".into()),
                )
                .output()
        }
        TestRunRequest::Rust => Command::new("cargo")
            .arg("test")
            .output(),
    };

    match output {
        Ok(out) if out.status.success() => TestResult::Passed,
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);

            let mut combined = String::new();

            if !stdout.trim().is_empty() {
                combined.push_str("=== STDOUT ===\n");
                combined.push_str(stdout.trim());
                combined.push('\n');
            }

            if !stderr.trim().is_empty() {
                combined.push_str("=== STDERR ===\n");
                combined.push_str(stderr.trim());
            }

            TestResult::Failed { output: combined }
        }
        Err(e) => TestResult::Failed {
            output: e.to_string(),
        },
    }
}

pub fn run_full_test_async<F>(language: Language, on_done: F)
where
    F: FnOnce(TestSuiteResult) + Send + 'static,
{
    std::thread::spawn(move || {
        let suite = match language {
            Language::Python => run_full_python_tests(),
            Language::Rust => run_full_rust_tests(),
            _ => TestSuiteResult {
                cases: Vec::new(),
                duration_ms: 0,
                raw_output: "unsupported language".into(),
            },
        };

        on_done(suite);
    });
}


fn run_full_python_tests() -> TestSuiteResult {
    let start = Instant::now();

    let output = Command::new("python")
        .arg("-m")
        .arg("pytest")
        .arg("-vv")
        .arg("-rA")
        .arg("--durations=0")
        .env(
            "PYTHONPATH",
            std::env::current_dir().unwrap_or_else(|_| ".".into()),
        )
        .output();

    let duration_ms = start.elapsed().as_millis() as u64;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);

            let raw_output = if stderr.trim().is_empty() {
                stdout.to_string()
            } else {
                format!("{stdout}\n{stderr}")
            };

            TestSuiteResult {
                cases: Vec::new(),
                duration_ms,
                raw_output,
            }
        }
        Err(e) => TestSuiteResult {
            cases: Vec::new(),
            duration_ms,
            raw_output: e.to_string(),
        },
    }
}

fn run_full_rust_tests() -> TestSuiteResult {
    let start = Instant::now();

    let output = Command::new("cargo")
        .arg("test")
        .arg("--")
        .arg("--nocapture")
        .output();

    let duration_ms = start.elapsed().as_millis() as u64;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);

            let mut raw_output = String::new();

            if !stdout.trim().is_empty() {
                raw_output.push_str("=== STDOUT ===\n");
                raw_output.push_str(stdout.trim());
                raw_output.push('\n');
            }

            if !stderr.trim().is_empty() {
                raw_output.push_str("=== STDERR ===\n");
                raw_output.push_str(stderr.trim());
            }

            TestSuiteResult {
                cases: Vec::new(),
                duration_ms,
                raw_output,
            }
        }
        Err(e) => TestSuiteResult {
            cases: Vec::new(),
            duration_ms,
            raw_output: e.to_string(),
        },
    }
}
