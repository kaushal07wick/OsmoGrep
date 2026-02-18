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
        let message = args
            .get("message")
            .and_then(Value::as_str)
            .ok_or("missing message")?;
        let add_all = args
            .get("add_all")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        let add_out = if add_all {
            Some(
                Command::new("git")
                    .arg("add")
                    .arg("-A")
                    .output()
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };

        let commit_out = Command::new("git")
            .arg("commit")
            .arg("-m")
            .arg(message)
            .output()
            .map_err(|e| e.to_string())?;

        Ok(json!({
            "add_exit_code": add_out.as_ref().and_then(|o| o.status.code()),
            "add_stderr": add_out.as_ref().map(|o| String::from_utf8_lossy(&o.stderr).to_string()),
            "commit_exit_code": commit_out.status.code(),
            "commit_stdout": String::from_utf8_lossy(&commit_out.stdout),
            "commit_stderr": String::from_utf8_lossy(&commit_out.stderr),
        }))
    }
}
