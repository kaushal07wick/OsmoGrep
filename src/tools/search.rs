// src/tools/search.rs

use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;

use super::{Tool, ToolResult, ToolSafety};

pub struct Search;

impl Tool for Search {
    fn name(&self) -> &'static str {
        "search"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "search",
            "description": "Search for a pattern in a specific file or directory",
            "parameters": {
                "type": "object",
                "properties": {
                    "pat":  { "type": "string", "description": "Pattern to search for" },
                    "path": { "type": "string", "description": "File or directory to search" }
                },
                "required": ["pat", "path"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }

    fn call(&self, args: Value) -> ToolResult {
        let pat = args
            .get("pat")
            .and_then(Value::as_str)
            .ok_or("missing pat")?;

        let root = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or("missing path")?;

        let root_path = Path::new(root);

        if !root_path.exists() {
            return Err(format!("path does not exist: {}", root).into());
        }
        if root_path.is_file() {
            let file = File::open(root_path)
                .map_err(|e| e.to_string())?;

            let mut hits = Vec::new();

            for (i, line) in BufReader::new(file).lines().enumerate() {
                let line = line
                    .map_err(|e| e.to_string())?;

                if line.contains(pat) {
                    hits.push(format!(
                        "{}:{}:{}",
                        root_path.display(),
                        i + 1,
                        line
                    ));
                }

                if hits.len() >= 200 {
                    break;
                }
            }

            return Ok(json!({
                "engine": "file",
                "count": hits.len(),
                "hits": hits
            }));
        }
        if root_path.is_dir() {
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

            let mut hits = Vec::new();

            for entry in WalkDir::new(root_path)
                .max_depth(10)
                .into_iter()
                .filter_map(Result::ok)
            {
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
                    }

                    if hits.len() >= 200 {
                        break;
                    }
                }

                if hits.len() >= 200 {
                    break;
                }
            }

            return Ok(json!({
                "engine": "fallback",
                "count": hits.len(),
                "hits": hits
            }));
        }

        Err("path must be a file or directory".into())
    }
}
