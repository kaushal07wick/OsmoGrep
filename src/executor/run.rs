//src/executor/run.rs
use std::process::Command;

use crate::state::TestResult;

pub fn run_single_test(cmd: &[&str]) -> TestResult {
    let output = Command::new(cmd[0])
        .args(&cmd[1..])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            TestResult::Passed
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            TestResult::Failed { output: stderr }
        }
        Err(e) => {
            TestResult::Failed {
                output: e.to_string(),
            }
        }
    }
}
