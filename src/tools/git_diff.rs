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
        let path = args.get("path").and_then(Value::as_str);

        let mut cmd = Command::new("git");
        cmd.arg("diff").arg("--");
        if let Some(path) = path {
            cmd.arg(path);
        }

        let out = cmd.output().map_err(|e| e.to_string())?;

        Ok(json!({
            "exit_code": out.status.code().unwrap_or(-1),
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
