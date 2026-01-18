// src/tools/edit.rs

use std::fs;
use serde_json::{json, Value};

use super::{Tool, ToolResult};

pub struct Edit;

impl Tool for Edit {
    fn name(&self) -> &'static str {
        "edit_file"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "edit_file",
            "description": "Edit a file by replacing text (first occurrence or all)",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old": { "type": "string" },
                    "new": { "type": "string" },
                    "all_occ": { "type": "boolean" }
                },
                "required": ["path", "old", "new"],
                "additionalProperties": false
            }
        })
    }

    fn call(&self, args: Value) -> ToolResult {
        let path = args.get("path").and_then(Value::as_str).ok_or("missing path")?;
        let old = args.get("old").and_then(Value::as_str).ok_or("missing old")?;
        let new = args.get("new").and_then(Value::as_str).ok_or("missing new")?;
        let all = args.get("all_occ").and_then(Value::as_bool).unwrap_or(false);

        let src = fs::read_to_string(path).map_err(|e| e.to_string())?;

        if !src.contains(old) {
            return Err("old string not found".into());
        }

        let updated = if all {
            src.replace(old, new)
        } else {
            src.replacen(old, new, 1)
        };

        fs::write(path, &updated).map_err(|e| e.to_string())?;

        Ok(json!({
            "path": path,
            "mode": if all { "all" } else { "first" },
            "before": src,
            "after": updated
        }))
    }
}
