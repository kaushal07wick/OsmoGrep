use std::process::Command;

use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct GitDiff;

impl Tool for GitDiff {
    fn name(&self) -> &'static str {
        "git_diff"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "git_diff",
            "description": "Show git diff for working tree, optionally for a specific path",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
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
        let path = args.get("path").and_then(Value::as_str);
        let repo_root = args.get("_repo_root").and_then(Value::as_str);

        let mut cmd = Command::new("git");
        if let Some(repo_root) = repo_root {
            cmd.current_dir(repo_root);
        }
        cmd.arg("diff").arg("--");
        if let Some(path) = path {
            cmd.arg(path);
        }

        let timeout = crate::process_runner::timeout_from_env("OSMOGREP_GIT_TIMEOUT_SECS", 120);
        let out = crate::process_runner::run_command_cancellable(cmd, timeout, is_cancelled)?;

        Ok(json!({
            "exit_code": out.exit_code,
            "timed_out": out.timed_out,
            "cancelled": out.cancelled,
            "diff": truncate(&String::from_utf8_lossy(&out.stdout), 12000),
            "stderr": truncate(&String::from_utf8_lossy(&out.stderr), 2000)
        }))
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let tail: String = s.chars().take(n).collect();
        format!("{}\n...truncated...", tail)
    }
}
