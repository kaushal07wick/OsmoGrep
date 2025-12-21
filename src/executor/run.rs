use std::process::Command;
use std::time::Instant;

use crate::logger::log;
use crate::state::{AgentState, LogLevel};

pub fn run_single_test(
    state: &mut AgentState,
    cmd: &[&str],
) {
    log(state, LogLevel::Info, format!("Running test: {:?}", cmd));

    let start = Instant::now();

    let output = Command::new(cmd[0])
        .args(&cmd[1..])
        .output();

    match output {
        Ok(out) => {
            let duration = start.elapsed();

            if out.status.success() {
                log(
                    state,
                    LogLevel::Success,
                    format!("Test passed in {:?}", duration),
                );
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                log(
                    state,
                    LogLevel::Error,
                    format!("Test failed:\n{}", stderr),
                );
            }
        }
        Err(e) => {
            log(
                state,
                LogLevel::Error,
                format!("Failed to execute test: {}", e),
            );
        }
    }
}
