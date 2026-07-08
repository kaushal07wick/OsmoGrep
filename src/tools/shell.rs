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
        if let Err(e) = crate::shell_guard::check_shell_command(cmd) {
            return Ok(json!({
                "error": e,
                "blocked": true,
                "exit_code": null
            }));
        }
        let root = std::env::current_dir().map_err(|e| e.to_string())?;

        let pre_hook = crate::hooks::run_hook("pre_shell", &[("cmd", cmd)])
            .ok()
            .flatten();

        let out = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .map_err(|e| e.to_string())?;
        let stdout_raw = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr_raw = String::from_utf8_lossy(&out.stderr).to_string();
        let exit_code = out.status.code().unwrap_or(-1);
        let mut combined = stdout_raw.clone();
        if !stderr_raw.is_empty() {
            if !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str(&stderr_raw);
        }
        let verification = crate::verification::record_command(&root, cmd, exit_code, &combined);
        let stdout = crate::tool_budget::budget_text(&root, "run_shell_stdout", &stdout_raw);
        let stderr = crate::tool_budget::budget_text(&root, "run_shell_stderr", &stderr_raw);

        Ok(json!({
            "stdout": stdout.text,
            "stderr": stderr.text,
            "exit_code": exit_code,
            "hook": pre_hook,
            "verification": crate::verification::to_json(&verification),
            "output_budget": {
                "stdout": stdout,
                "stderr": stderr
            }
        }))
    }
}
