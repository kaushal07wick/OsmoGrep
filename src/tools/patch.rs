use std::process::Command;
use std::{fs, path::Path};

use regex::Regex;
use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct Patch;

impl Tool for Patch {
    fn name(&self) -> &'static str {
        "patch"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "patch",
            "description": "Apply unified diff patch to the repository",
            "parameters": {
                "type": "object",
                "properties": {
                    "patch": { "type": "string" }
                },
                "required": ["patch"],
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
        let patch = args
            .get("patch")
            .and_then(Value::as_str)
            .ok_or("missing patch")?;

        let target = extract_first_target(patch);
        let before = target
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default();

        let mut command = Command::new("git");
        command
            .arg("apply")
            .arg("--recount")
            .arg("--whitespace=nowarn")
            .arg("-");
        let timeout = crate::process_runner::timeout_from_env("OSMOGREP_PATCH_TIMEOUT_SECS", 15);
        let out = crate::process_runner::run_command_with_stdin_cancellable(
            command,
            patch.as_bytes(),
            timeout,
            is_cancelled,
        )?;

        let after = target
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default();
        let root = std::env::current_dir().map_err(|e| e.to_string())?;
        let target_path = target.clone().unwrap_or_default();
        let verification_stale = if out.exit_code == 0 && !target_path.is_empty() {
            crate::verification::mark_workspace_edited(&root, [target_path.as_str()])
        } else {
            None
        };

        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let mut result = json!({
            "path": target_path,
            "before": before,
            "after": after,
            "exit_code": out.exit_code,
            "stdout": stdout,
            "stderr": stderr,
            "timed_out": out.timed_out,
            "cancelled": out.cancelled,
            "verification_stale": crate::verification::staleness_to_json(&verification_stale)
        });

        if out.cancelled || out.timed_out || out.exit_code != 0 {
            let reason = if out.cancelled {
                "patch cancelled".to_string()
            } else if out.timed_out {
                format!("patch timed out after {}s", timeout.as_secs())
            } else if !stderr.trim().is_empty() {
                stderr.trim().to_string()
            } else {
                format!("git apply exited with {}", out.exit_code)
            };
            if let Some(map) = result.as_object_mut() {
                map.insert("error".to_string(), json!(reason));
            }
        }

        Ok(result)
    }
}

fn extract_first_target(patch: &str) -> Option<String> {
    let re = Regex::new(r"\+\+\+\s+[ab]/([^\n\r]+)").ok()?;
    let caps = re.captures(patch)?;
    let p = caps.get(1)?.as_str();
    if Path::new(p).exists() || !p.is_empty() {
        Some(p.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::Patch;
    use crate::tools::Tool;
    use serde_json::json;

    #[test]
    fn invalid_patch_returns_error_field() {
        let result = Patch
            .call(json!({
                "patch": "*** Begin Patch\n*** Update File: README.md\n@@\n"
            }))
            .unwrap();

        assert!(result.get("error").is_some());
        assert_ne!(result.get("exit_code").and_then(|v| v.as_i64()), Some(0));
    }
}
