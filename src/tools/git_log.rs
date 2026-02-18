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
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(20)
            .min(200);

        let out = Command::new("git")
            .arg("log")
            .arg(format!("-n{}", limit))
            .arg("--pretty=format:%h|%an|%ad|%s")
            .arg("--date=short")
            .output()
            .map_err(|e| e.to_string())?;

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
            "exit_code": out.status.code().unwrap_or(-1),
            "commits": lines,
            "stderr": String::from_utf8_lossy(&out.stderr)
        }))
    }
}
