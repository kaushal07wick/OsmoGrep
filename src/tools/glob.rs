// src/tools/glob.rs

use glob::glob as glob_fn;
use serde_json::{json, Value};
use std::path::PathBuf;
use walkdir::WalkDir;

use super::{Tool, ToolResult, ToolSafety};

pub struct Glob;

const SAMPLE_LIMIT: usize = 200;

impl Tool for Glob {
    fn name(&self) -> &'static str {
        "glob_files"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "glob_files",
            "description": "List files matching a glob pattern (safe for large directories)",
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

    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }

    fn call(&self, args: Value) -> ToolResult {
        let pat = args
            .get("pat")
            .and_then(Value::as_str)
            .ok_or("missing pat")?;

        let root = args.get("path").and_then(Value::as_str).unwrap_or(".");

        let mut count = 0usize;
        let mut sample: Vec<String> = Vec::new();

        // Broad patterns → WalkDir (streaming, safe)
        if pat == "*" || pat == "**" || pat == "**/*" {
            for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
                if entry.file_type().is_file() {
                    count += 1;

                    if sample.len() < SAMPLE_LIMIT {
                        sample.push(entry.path().display().to_string());
                    }
                }
            }

            return Ok(json!({
                "count": count,
                "sample": sample
            }));
        }

        // Narrow patterns → glob, but bounded
        let full = format!("{}/{}", root, pat);
        let paths = glob_fn(&full).map_err(|e| e.to_string())?;

        for entry in paths {
            let p: PathBuf = entry.map_err(|e| e.to_string())?;
            count += 1;

            if sample.len() < SAMPLE_LIMIT {
                sample.push(p.display().to_string());
            }
        }

        Ok(json!({
            "count": count,
            "sample": sample
        }))
    }
}
