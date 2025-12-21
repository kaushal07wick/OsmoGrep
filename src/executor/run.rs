// src/executor/run.rs
use std::process::Command;

use crate::state::TestResult;

pub fn run_single_test(cmd: &[&str]) -> TestResult {
    let output = Command::new(cmd[0])
        .args(&cmd[1..])
        .output();

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

            TestResult::Failed {
                output: combined,
            }
        }

        Err(e) => TestResult::Failed {
            output: e.to_string(),
        },
    }
}
