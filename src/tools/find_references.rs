use regex::Regex;
use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

use super::{Tool, ToolResult, ToolSafety};

pub struct FindReferences;

impl Tool for FindReferences {
    fn name(&self) -> &'static str {
        "find_references"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "find_references",
            "description": "Find likely usages of a symbol",
            "parameters": {
                "type": "object",
                "properties": {
                    "symbol": { "type": "string" },
                    "path": { "type": "string" },
                    "limit": { "type": "integer" }
                },
                "required": ["symbol"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }

    fn call(&self, args: Value) -> ToolResult {
        let symbol = args
            .get("symbol")
            .and_then(Value::as_str)
            .ok_or("missing symbol")?;
        let root = args.get("path").and_then(Value::as_str).unwrap_or(".");
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(200)
            .min(2000);

        let sym_re =
            Regex::new(&format!(r"\b{}\b", regex::escape(symbol))).map_err(|e| e.to_string())?;
        let def_re = Regex::new(&format!(
            r"\b(fn|struct|enum|trait|type|const|def|class|function|let|var|func)\s+{}\b",
            regex::escape(symbol)
        ))
        .map_err(|e| e.to_string())?;

        let mut hits = Vec::new();
        let path = Path::new(root);
        if path.is_file() {
            search_file(path, &sym_re, &def_re, &mut hits, limit)?;
        } else {
            for ent in WalkDir::new(path)
                .max_depth(12)
                .into_iter()
                .filter_map(Result::ok)
            {
                let p = ent.path();
                if !p.is_file() || is_ignored(p) {
                    continue;
                }
                let _ = search_file(p, &sym_re, &def_re, &mut hits, limit);
                if hits.len() >= limit {
                    break;
                }
            }
        }

        Ok(json!({
            "symbol": symbol,
            "count": hits.len(),
            "hits": hits,
        }))
    }
}

fn search_file(
    path: &Path,
    sym_re: &Regex,
    def_re: &Regex,
    hits: &mut Vec<String>,
    limit: usize,
) -> Result<(), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if sym_re.is_match(&line) && !def_re.is_match(&line) {
            hits.push(format!("{}:{}:{}", path.display(), idx + 1, line.trim()));
        }
        if hits.len() >= limit {
            break;
        }
    }
    Ok(())
}

fn is_ignored(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.contains("/target/") || text.contains("/node_modules/") || text.contains("/.git/")
}
