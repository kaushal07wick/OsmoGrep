// Executes test commands and returns raw output + timing.

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
    pub cases: Vec<TestCaseResult>,
    pub duration_ms: u64,
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


/// Runs the entire test suite synchronously and returns raw output.
pub fn run_test_suite(language: Language) -> TestSuiteResult {
    match language {
        Language::Python => run_python_test_suite(),
        Language::Rust => run_rust_test_suite(),
        _ => TestSuiteResult {
            cases: Vec::new(),
            duration_ms: 0,
            raw_output: "unsupported language".into(),
        },
    }
}
fn run_python_test_suite() -> TestSuiteResult {
    let start = Instant::now();
    let cwd = std::env::current_dir().unwrap();

    // Merge PYTHONPATH properly
    let existing_pp = std::env::var("PYTHONPATH").unwrap_or_default();
    let merged_pp = if existing_pp.is_empty() {
        cwd.display().to_string()
    } else {
        format!("{}:{}", cwd.display(), existing_pp)
    };

    // run pytest with consistent verbosity + no warning noise
    let output = Command::new("python")
        .arg("-m")
        .arg("pytest")
        .arg("-vv")
        .arg("-rA")
        .arg("--durations=0")
        .arg("--maxfail=0")
        .arg("--disable-warnings")
        .env("PYTHONPATH", merged_pp)
        .output();

    let duration_ms = start.elapsed().as_millis() as u64;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);

            // Merge clean, never drop stderr
            let raw_output = format!("{}\n{}", stdout, stderr);

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

fn run_rust_test_suite() -> TestSuiteResult {
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
