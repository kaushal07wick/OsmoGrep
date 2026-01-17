// src/tools/search.rs

use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process::Command;
use walkdir::WalkDir;

use super::{Tool, ToolResult};

pub struct Search;

impl Tool for Search {
    fn name(&self) -> &'static str {
        "search"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "search",
            "description": "Search for a pattern in files (uses ripgrep if available, otherwise fallback)",
            "parameters": {
                "type": "object",
                "properties": {
                    "pat":  { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["pat"],
                "additionalProperties": false
            }
        })
    }

    fn call(&self, args: Value) -> ToolResult {
        let pat = args
            .get("pat")
            .and_then(Value::as_str)
            .ok_or("missing pat")?;

        let root = args
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or(".");

        // --- try ripgrep first ---
        if let Ok(out) = Command::new("rg")
            .arg("--line-number")
            .arg("--no-heading")
            .arg(pat)
            .arg(root)
            .output()
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                let hits: Vec<String> =
                    text.lines().take(200).map(|l| l.to_string()).collect();

                return Ok(json!({
                    "engine": "ripgrep",
                    "count": hits.len(),
                    "hits": hits
                }));
            }
        }

        // --- fallback: pure Rust grep ---
        let mut hits = Vec::new();

        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let file = match File::open(path) {
                Ok(f) => f,
                Err(_) => continue,
            };

            for (i, line) in BufReader::new(file).lines().enumerate() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => continue,
                };

                if line.contains(pat) {
                    hits.push(format!(
                        "{}:{}:{}",
                        path.display(),
                        i + 1,
                        line
                    ));

                    if hits.len() >= 200 {
                        break;
                    }
                }
            }

            if hits.len() >= 200 {
                break;
            }
        }

        Ok(json!({
            "engine": "fallback",
            "count": hits.len(),
            "hits": hits
        }))
    }
}
