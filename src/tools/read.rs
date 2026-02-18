// src/tools/read.rs

use serde_json::{json, Value};
use std::fs;

use super::{Tool, ToolResult, ToolSafety};

pub struct Read;

impl Tool for Read {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "read_file",
            "description": "Read a file with optional line offset and limit",
            "parameters": {
                "type": "object",
                "properties": {
                    "path":   { "type": "string" },
                    "offset": { "type": "integer" },
                    "limit":  { "type": "integer" }
                },
                "required": ["path"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }

    fn call(&self, args: Value) -> ToolResult {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or("missing path")?;

        let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;

        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| v as usize);

        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let lines: Vec<&str> = content.lines().collect();

        if offset >= lines.len() {
            return Ok(json!({
                "text": "",
                "lines": 0
            }));
        }

        let end = limit.map(|l| offset + l).unwrap_or(lines.len());
        let slice = &lines[offset..end.min(lines.len())];

        Ok(json!({
            "text": slice.join("\n"),
            "lines": slice.len()
        }))
    }
}
