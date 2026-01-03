// src/testgen/runner.rs
//
// Executes generated tests with correct environment.

use std::process::Command;
use std::env;

use crate::state::TestResult;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum TestRunRequest {
    Python {
        test_path: PathBuf,
    },
    Rust,
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
