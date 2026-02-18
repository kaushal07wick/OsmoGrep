use std::fs;

use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct NotebookEdit;

impl Tool for NotebookEdit {
    fn name(&self) -> &'static str {
        "notebook_edit"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "notebook_edit",
            "description": "Edit a Jupyter notebook cell source by replacing text",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "cell_index": { "type": "integer" },
                    "old": { "type": "string" },
                    "new": { "type": "string" },
                    "all_occ": { "type": "boolean" }
                },
                "required": ["path", "cell_index", "old", "new"],
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
        let cell_index = args
            .get("cell_index")
            .and_then(Value::as_u64)
            .ok_or("missing cell_index")? as usize;
        let old = args
            .get("old")
            .and_then(Value::as_str)
            .ok_or("missing old")?;
        let new = args
            .get("new")
            .and_then(Value::as_str)
            .ok_or("missing new")?;
        let all = args
            .get("all_occ")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let src = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut nb: Value = serde_json::from_str(&src).map_err(|e| e.to_string())?;

        let cells = nb
            .get_mut("cells")
            .and_then(Value::as_array_mut)
            .ok_or("invalid notebook: missing cells")?;
        if cell_index >= cells.len() {
            return Err(format!("cell_index out of range: {}", cell_index));
        }

        let cell = &mut cells[cell_index];
        let source = cell.get_mut("source").ok_or("cell missing source")?;

        let source_text = if let Some(arr) = source.as_array() {
            arr.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("")
        } else {
            source.as_str().unwrap_or("").to_string()
        };

        if !source_text.contains(old) {
            return Err("old string not found in cell source".to_string());
        }

        let updated = if all {
            source_text.replace(old, new)
        } else {
            source_text.replacen(old, new, 1)
        };

        *source = Value::Array(
            updated
                .lines()
                .map(|l| Value::String(format!("{}\n", l)))
                .collect(),
        );

        fs::write(
            path,
            serde_json::to_string_pretty(&nb).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        Ok(json!({
            "path": path,
            "cell_index": cell_index,
            "before": source_text,
            "after": updated,
        }))
    }
}
