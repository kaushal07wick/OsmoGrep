use std::process::Command;

use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct GitCommit;

impl Tool for GitCommit {
    fn name(&self) -> &'static str {
        "git_commit"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "git_commit",
            "description": "Stage and commit changes",
            "parameters": {
                "type": "object",
                "properties": {
                    "message": { "type": "string" },
                    "add_all": { "type": "boolean" }
                },
                "required": ["message"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Dangerous
    }

    fn call(&self, args: Value) -> ToolResult {
        self.call_cancellable(args, &|| false)
    }

    fn call_cancellable(&self, args: Value, is_cancelled: &dyn Fn() -> bool) -> ToolResult {
        let message = args
            .get("message")
            .and_then(Value::as_str)
            .ok_or("missing message")?;
        let add_all = args.get("add_all").and_then(Value::as_bool).unwrap_or(true);
        let timeout = crate::process_runner::timeout_from_env("OSMOGREP_GIT_TIMEOUT_SECS", 120);

        let add_out = if add_all {
            let mut add = Command::new("git");
            add.arg("add").arg("-A");
            Some(crate::process_runner::run_command_cancellable(
                add,
                timeout,
                is_cancelled,
            )?)
        } else {
            None
        };

        if add_out.as_ref().is_some_and(|out| out.cancelled) || is_cancelled() {
            return Ok(json!({
                "error": "git commit cancelled before commit",
                "cancelled": true,
                "add_exit_code": add_out.as_ref().map(|o| o.exit_code),
                "add_stderr": add_out.as_ref().map(|o| String::from_utf8_lossy(&o.stderr).to_string()),
                "commit_exit_code": null,
                "commit_stdout": "",
                "commit_stderr": "cancelled"
            }));
        }

        let mut commit = Command::new("git");
        commit.arg("commit").arg("-m").arg(message);
        let commit_out =
            crate::process_runner::run_command_cancellable(commit, timeout, is_cancelled)?;

        Ok(json!({
            "add_exit_code": add_out.as_ref().map(|o| o.exit_code),
            "add_timed_out": add_out.as_ref().map(|o| o.timed_out),
            "add_cancelled": add_out.as_ref().map(|o| o.cancelled),
            "add_stderr": add_out.as_ref().map(|o| String::from_utf8_lossy(&o.stderr).to_string()),
            "commit_exit_code": commit_out.exit_code,
            "commit_stdout": String::from_utf8_lossy(&commit_out.stdout),
            "commit_stderr": String::from_utf8_lossy(&commit_out.stderr),
            "commit_timed_out": commit_out.timed_out,
            "commit_cancelled": commit_out.cancelled,
            "cancelled": commit_out.cancelled,
            "timed_out": commit_out.timed_out
        }))
    }
}
