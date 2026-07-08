use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct WorktreeSwarm;

impl Tool for WorktreeSwarm {
    fn name(&self) -> &'static str {
        "worktree_swarm"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "worktree_swarm",
            "description": "Spawn tool-enabled subagents in isolated git worktrees for broad exploration, implementation, testing, and review. Use for large or complex tasks where parallel isolated investigation is worth the cost.",
            "parameters": {
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "The parent task for all worktree-isolated subagents"
                    }
                },
                "required": ["task"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Dangerous
    }

    fn call(&self, args: Value) -> ToolResult {
        let task = args
            .get("task")
            .and_then(Value::as_str)
            .ok_or("missing task")?;
        let root = std::env::current_dir().map_err(|e| e.to_string())?;
        let results = crate::worktree::run_worktree_swarm(&root, task)?;
        let agents = results
            .into_iter()
            .map(|result| {
                json!({
                    "role": result.role,
                    "branch": result.branch,
                    "path": result.path.display().to_string(),
                    "success": result.success,
                    "output": truncate(&result.output, 6000),
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "agent_count": agents.len(),
            "agents": agents,
        }))
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let head: String = text.chars().take(max_chars).collect();
        format!("{head}\n...truncated...")
    }
}
