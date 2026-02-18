use regex::Regex;
use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

use super::{Tool, ToolResult, ToolSafety};

pub struct FindDefinition;

impl Tool for FindDefinition {
    fn name(&self) -> &'static str {
        "find_definition"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "find_definition",
            "description": "Find likely symbol definitions in source files",
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
            .unwrap_or(50)
            .min(500);

        let regexes = build_definition_patterns(symbol)?;
        let mut hits = Vec::new();

        let path = Path::new(root);
        if path.is_file() {
            search_file(path, &regexes, &mut hits, limit)?;
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
                let _ = search_file(p, &regexes, &mut hits, limit);
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

fn build_definition_patterns(symbol: &str) -> Result<Vec<Regex>, String> {
    let s = regex::escape(symbol);
    let patterns = vec![
        format!(r"\b(fn|struct|enum|trait|type|const)\s+{}\b", s),
        format!(r"\b(def|class)\s+{}\b", s),
        format!(r"\b(function|class|const|let|var)\s+{}\b", s),
        format!(r"\bfunc\s+(\([^\)]*\)\s*)?{}\b", s),
    ];

    patterns
        .into_iter()
        .map(|p| Regex::new(&p).map_err(|e| e.to_string()))
        .collect()
}

fn search_file(path: &Path, regexes: &[Regex], hits: &mut Vec<String>, limit: usize) -> Result<(), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if regexes.iter().any(|re| re.is_match(&line)) {
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
