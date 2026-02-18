use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use regex::Regex;
use walkdir::WalkDir;

use super::{Tool, ToolResult, ToolSafety};

pub struct RegexSearch;

impl Tool for RegexSearch {
    fn name(&self) -> &'static str {
        "regex_search"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "regex_search",
            "description": "Regex search in a file or directory",
            "parameters": {
                "type": "object",
                "properties": {
                    "pat": { "type": "string", "description": "Regex pattern" },
                    "path": { "type": "string", "description": "File or directory" },
                    "limit": { "type": "integer", "description": "Max hits" }
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
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(200)
            .min(2000);

        let re = Regex::new(pat).map_err(|e| e.to_string())?;
        let path = Path::new(root);
        if !path.exists() {
            return Err(format!("path does not exist: {}", root));
        }

        let mut hits = Vec::new();

        if path.is_file() {
            search_file(path, &re, &mut hits, limit)?;
        } else {
            for entry in WalkDir::new(path)
                .max_depth(10)
                .into_iter()
                .filter_map(Result::ok)
            {
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                let _ = search_file(p, &re, &mut hits, limit);
                if hits.len() >= limit {
                    break;
                }
            }
        }

        Ok(json!({
            "pattern": pat,
            "count": hits.len(),
            "hits": hits,
        }))
    }
}

fn search_file(
    path: &Path,
    re: &Regex,
    hits: &mut Vec<String>,
    limit: usize,
) -> Result<(), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if re.is_match(&line) {
            hits.push(format!("{}:{}:{}", path.display(), idx + 1, line));
        }
        if hits.len() >= limit {
            break;
        }
    }
    Ok(())
}
