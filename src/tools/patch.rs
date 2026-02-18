use std::io::Write;
use std::process::{Command, Stdio};
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
        let patch = args
            .get("patch")
            .and_then(Value::as_str)
            .ok_or("missing patch")?;

        let target = extract_first_target(patch);
        let before = target
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default();

        let mut child = Command::new("git")
            .arg("apply")
            .arg("--recount")
            .arg("--whitespace=nowarn")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(patch.as_bytes())
                .map_err(|e| e.to_string())?;
        }

        let out = child.wait_with_output().map_err(|e| e.to_string())?;

        let after = target
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default();

        Ok(json!({
            "path": target.unwrap_or_default(),
            "before": before,
            "after": after,
            "exit_code": out.status.code(),
            "stdout": String::from_utf8_lossy(&out.stdout),
            "stderr": String::from_utf8_lossy(&out.stderr),
        }))
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
