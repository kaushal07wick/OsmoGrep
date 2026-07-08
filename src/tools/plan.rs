use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct Plan;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlanItem {
    id: u64,
    text: String,
    status: String,
}

impl Tool for Plan {
    fn name(&self) -> &'static str {
        "update_plan"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "update_plan",
            "description": "Maintain a durable repo-local progress plan under .context/osmogrep-plan.json. Use it for multi-step work, long tasks, and handoff continuity.",
            "parameters": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "set", "add", "done", "clear"]
                    },
                    "items": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Plan item texts for action=set"
                    },
                    "item": {
                        "type": "string",
                        "description": "Plan item text for action=add"
                    },
                    "id": {
                        "type": "integer",
                        "description": "Plan item id for action=done"
                    }
                },
                "required": ["action"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }

    fn call(&self, args: Value) -> ToolResult {
        let action = args
            .get("action")
            .and_then(Value::as_str)
            .ok_or("missing action")?;
        let root = std::env::current_dir().map_err(|e| e.to_string())?;
        let path = plan_path(&root);
        let mut items = load_plan(&path)?;

        match action {
            "list" => {}
            "set" => {
                let raw_items = args
                    .get("items")
                    .and_then(Value::as_array)
                    .ok_or("action=set requires items")?;
                items = raw_items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .enumerate()
                    .map(|(idx, text)| PlanItem {
                        id: (idx as u64) + 1,
                        text: text.to_string(),
                        status: if idx == 0 { "in_progress" } else { "pending" }.to_string(),
                    })
                    .collect();
                save_plan(&path, &items)?;
            }
            "add" => {
                let text = args
                    .get("item")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .ok_or("action=add requires item")?;
                let next_id = items.iter().map(|item| item.id).max().unwrap_or(0) + 1;
                items.push(PlanItem {
                    id: next_id,
                    text: text.to_string(),
                    status: "pending".to_string(),
                });
                save_plan(&path, &items)?;
            }
            "done" => {
                let id = args
                    .get("id")
                    .and_then(Value::as_u64)
                    .ok_or("action=done requires id")?;
                let mut found = false;
                for item in &mut items {
                    if item.id == id {
                        item.status = "completed".to_string();
                        found = true;
                    } else if item.status == "in_progress" {
                        item.status = "pending".to_string();
                    }
                }
                if !found {
                    return Err(format!("plan item id not found: {id}"));
                }
                if let Some(next) = items.iter_mut().find(|item| item.status == "pending") {
                    next.status = "in_progress".to_string();
                }
                save_plan(&path, &items)?;
            }
            "clear" => {
                items.clear();
                save_plan(&path, &items)?;
            }
            _ => return Err(format!("unknown plan action: {action}")),
        }

        Ok(json!({
            "path": path.display().to_string(),
            "items": items,
        }))
    }
}

fn plan_path(root: &Path) -> PathBuf {
    root.join(".context").join("osmogrep-plan.json")
}

fn load_plan(path: &Path) -> Result<Vec<PlanItem>, String> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn save_plan(path: &Path, items: &[PlanItem]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}
