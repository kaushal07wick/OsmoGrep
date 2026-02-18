use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct ListDir;

impl Tool for ListDir {
    fn name(&self) -> &'static str {
        "list_dir"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "list_dir",
            "description": "List directory entries with basic metadata",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "limit": { "type": "integer" }
                },
                "required": [],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }

    fn call(&self, args: Value) -> ToolResult {
        let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(200)
            .min(1000);

        let dir = Path::new(path);
        if !dir.is_dir() {
            return Err(format!("not a directory: {}", path));
        }

        let mut entries = Vec::new();
        for ent in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let ent = ent.map_err(|e| e.to_string())?;
            let meta = ent.metadata().map_err(|e| e.to_string())?;

            entries.push(json!({
                "name": ent.file_name().to_string_lossy().to_string(),
                "path": ent.path().display().to_string(),
                "is_dir": meta.is_dir(),
                "size": meta.len()
            }));

            if entries.len() >= limit {
                break;
            }
        }

        Ok(json!({
            "path": dir.display().to_string(),
            "count": entries.len(),
            "entries": entries
        }))
    }
}
