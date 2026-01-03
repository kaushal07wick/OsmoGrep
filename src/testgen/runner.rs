// src/testgen/runner.rs
//
// Executes generated tests with correct environment.

use std::process::Command;
use std::path::PathBuf;

use crate::state::TestResult;
use crate::detectors::language::Language;

#[derive(Debug, Clone)]
pub enum TestRunRequest {
    Python {
        test_path: PathBuf,
    },
    Rust,
}

#[derive(Debug)]
pub struct TestSuiteResult {
    pub passed: Vec<String>,
    pub failed: Vec<(String, String)>, // (test_name, output)
}


pub fn run_test(req: TestRunRequest) -> TestResult {
    let output = match req {
        TestRunRequest::Python { test_path } => {
            Command::new("python")
                .arg("-m")
                .arg("pytest")
                .arg(test_path)
                .env("PYTHONPATH", ".")
                .output()
        }

        TestRunRequest::Rust => {
            Command::new("cargo")
                .arg("test")
                .output()
        }
    };

    match output {
        Ok(out) if out.status.success() => TestResult::Passed,

        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);

            let mut combined = String::new();
            if !stdout.trim().is_empty() {
                combined.push_str(stdout.trim());
            }
            if !stderr.trim().is_empty() {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(stderr.trim());
            }

            TestResult::Failed { output: combined }
        }

        Err(e) => TestResult::Failed {
            output: e.to_string(),
        },
    }
}


pub fn run_full_test(language: Language) -> TestSuiteResult {
    match language {
        Language::Python => run_full_python_tests(),
        Language::Rust => run_full_rust_tests(),
        _ => TestSuiteResult {
            passed: vec![],
            failed: vec![(
                "unsupported-language".into(),
                "Full test suite not supported for this language".into(),
            )],
        },
    }
}


fn run_full_python_tests() -> TestSuiteResult {
    let output = Command::new("python")
    .arg("-m")
    .arg("pytest")
    .arg("-v")
    .arg("-rA")
    .env("PYTHONPATH", ".")
    .output();

    match output {
        Ok(out) => parse_pytest_output(&out),
        Err(e) => TestSuiteResult {
            passed: vec![],
            failed: vec![("pytest".into(), e.to_string())],
        },
    }
}

fn parse_pytest_output(out: &std::process::Output) -> TestSuiteResult {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    let mut passed = Vec::new();
    let mut failed = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();

        if let Some(name) = line.strip_suffix(" PASSED") {
            passed.push(name.to_string());
        } else if let Some(name) = line.strip_suffix(" FAILED") {
            failed.push((name.to_string(), String::new()));
        }
    }

    if !stderr.trim().is_empty() {
        for (_, output) in failed.iter_mut() {
            *output = stderr.trim().to_string();
        }
    }

    TestSuiteResult { passed, failed }
}

fn run_full_rust_tests() -> TestSuiteResult {
    let output = Command::new("cargo")
        .arg("test")
        .arg("--")
        .arg("--nocapture")
        .output();

    match output {
        Ok(out) => parse_cargo_test_output(&out),
        Err(e) => TestSuiteResult {
            passed: vec![],
            failed: vec![("cargo test".into(), e.to_string())],
        },
    }
}

fn parse_cargo_test_output(out: &std::process::Output) -> TestSuiteResult {
    let stdout = String::from_utf8_lossy(&out.stdout);

    let mut passed = Vec::new();
    let mut failed = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();

        if line.ends_with(" ... ok") {
            passed.push(line.to_string());
        } else if line.ends_with(" ... FAILED") {
            failed.push((line.to_string(), String::new()));
        }
    }

    TestSuiteResult { passed, failed }
}

