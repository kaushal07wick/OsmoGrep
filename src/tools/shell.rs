// src/tools/shell.rs

use std::process::Command;
use serde_json::{json, Value};
use super::{Tool, ToolResult, ToolSafety};
pub struct Shell;

impl Tool for Shell {
    fn name(&self) -> &'static str {
        "run_shell"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "run_shell",
            "description": "Run a shell command and return stdout, stderr, and exit code",
            "parameters": {
                "type": "object",
                "properties": {
                    "cmd": { "type": "string" }
                },
                "required": ["cmd"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Dangerous
    }

    fn call(&self, args: Value) -> ToolResult {
        let cmd = args
            .get("cmd")
            .and_then(Value::as_str)
            .ok_or("missing cmd")?;

        let pre_hook = crate::hooks::run_hook("pre_shell", &[("cmd", cmd)])
            .ok()
            .flatten();

        let out = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .map_err(|e| e.to_string())?;

        Ok(json!({
            "stdout": String::from_utf8_lossy(&out.stdout),
            "stderr": String::from_utf8_lossy(&out.stderr),
            "exit_code": out.status.code(),
            "hook": pre_hook
        }))
    }
}
