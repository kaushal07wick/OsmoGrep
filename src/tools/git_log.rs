use std::process::Command;

use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct GitLog;

impl Tool for GitLog {
    fn name(&self) -> &'static str {
        "git_log"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "git_log",
            "description": "Show recent commit history",
            "parameters": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer" }
                },
                "required": [],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }

    fn call(&self, args: Value) -> ToolResult {
        self.call_cancellable(args, &|| false)
    }

    fn call_cancellable(&self, args: Value, is_cancelled: &dyn Fn() -> bool) -> ToolResult {
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(20)
            .min(200);
        let repo_root = args.get("_repo_root").and_then(Value::as_str);

        let mut cmd = Command::new("git");
        if let Some(repo_root) = repo_root {
            cmd.current_dir(repo_root);
        }
        cmd.arg("log")
            .arg(format!("-n{}", limit))
            .arg("--pretty=format:%h|%an|%ad|%s")
            .arg("--date=short");

        let timeout = crate::process_runner::timeout_from_env("OSMOGREP_GIT_TIMEOUT_SECS", 120);
        let out = crate::process_runner::run_command_cancellable(cmd, timeout, is_cancelled)?;

        let lines = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|line| {
                let mut parts = line.splitn(4, '|');
                json!({
                    "sha": parts.next().unwrap_or(""),
                    "author": parts.next().unwrap_or(""),
                    "date": parts.next().unwrap_or(""),
                    "subject": parts.next().unwrap_or(""),
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "exit_code": out.exit_code,
            "timed_out": out.timed_out,
            "cancelled": out.cancelled,
            "commits": lines,
            "stderr": String::from_utf8_lossy(&out.stderr)
        }))
    }
}
