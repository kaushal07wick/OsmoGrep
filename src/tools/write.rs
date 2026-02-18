// src/tools/write.rs

use std::fs;
use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct Write;

impl Tool for Write {
    fn name(&self) -> &'static str {
        "write_file"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "write_file",
            "description": "Write content to a file, replacing it if it exists",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Dangerous
    }

    fn call(&self, args: Value) -> ToolResult {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or("missing path")?;

        let content = args
            .get("content")
            .and_then(Value::as_str)
            .ok_or("missing content")?;

        let pre_hook = crate::hooks::run_hook("pre_edit", &[("path", path)])
            .ok()
            .flatten();
        let before = fs::read_to_string(path).unwrap_or_default();
        fs::write(path, content).map_err(|e| e.to_string())?;
        let post_hook = crate::hooks::run_hook("post_edit", &[("path", path)])
            .ok()
            .flatten();

        Ok(json!({
            "path": path,
            "bytes": content.len(),
            "before": before,
            "after": content,
            "pre_hook": pre_hook,
            "post_hook": post_hook
        }))
    }
}
