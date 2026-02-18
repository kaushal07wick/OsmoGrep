use regex::Regex;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

use super::{Tool, ToolResult, ToolSafety};

pub struct Diagnostics;

impl Tool for Diagnostics {
    fn name(&self) -> &'static str {
        "diagnostics"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "diagnostics",
            "description": "Run project diagnostics/lint/check and parse structured issues",
            "parameters": {
                "type": "object",
                "properties": {
                    "cmd": { "type": "string" }
                },
                "required": [],
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
            .map(|s| s.to_string())
            .unwrap_or_else(default_command);

        let out = Command::new("sh")
            .arg("-lc")
            .arg(&cmd)
            .output()
            .map_err(|e| e.to_string())?;

        let mut text = String::new();
        text.push_str(&String::from_utf8_lossy(&out.stdout));
        if !out.stderr.is_empty() {
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&String::from_utf8_lossy(&out.stderr));
        }

        let issues = parse_issues(&text);

        Ok(json!({
            "command": cmd,
            "exit_code": out.status.code().unwrap_or(-1),
            "issue_count": issues.len(),
            "issues": issues,
            "output": truncate(&text, 12000),
        }))
    }
}

fn default_command() -> String {
    if Path::new("Cargo.toml").exists() {
        "cargo check --message-format short".to_string()
    } else if Path::new("pyproject.toml").exists() {
        "ruff check . || pytest -q".to_string()
    } else if Path::new("package.json").exists() {
        "npm run lint --silent || npm test -- --runInBand".to_string()
    } else if Path::new("go.mod").exists() {
        "go test ./...".to_string()
    } else {
        "echo 'No diagnostics command detected'".to_string()
    }
}

fn parse_issues(output: &str) -> Vec<Value> {
    let mut items = Vec::new();
    let re = Regex::new(r"^([^:\n]+):(\d+):(\d+):\s*(.*)$").unwrap();

    for line in output.lines() {
        if let Some(c) = re.captures(line) {
            let file = c.get(1).map(|m| m.as_str()).unwrap_or("");
            let line_no = c
                .get(2)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(0);
            let col = c
                .get(3)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(0);
            let msg = c.get(4).map(|m| m.as_str()).unwrap_or("");

            items.push(json!({
                "file": file,
                "line": line_no,
                "col": col,
                "message": msg,
            }));
        }
        if items.len() >= 500 {
            break;
        }
    }

    items
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let head: String = s.chars().take(n).collect();
        format!("{}\n...truncated...", head)
    }
}
