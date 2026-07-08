// src/tools/shell.rs

use super::{Tool, ToolResult, ToolSafety};
use serde_json::{json, Value};
use std::process::Command;
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
        let root = std::env::current_dir().map_err(|e| e.to_string())?;

        let pre_hook = crate::hooks::run_hook("pre_shell", &[("cmd", cmd)])
            .ok()
            .flatten();

        let out = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let exit_code = out.status.code().unwrap_or(-1);
        let mut combined = stdout.clone();
        if !stderr.is_empty() {
            if !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str(&stderr);
        }
        let verification = crate::verification::record_command(&root, cmd, exit_code, &combined);

        Ok(json!({
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": exit_code,
            "hook": pre_hook,
            "verification": crate::verification::to_json(&verification)
        }))
    }
}
