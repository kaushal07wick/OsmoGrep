use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct Test;

impl Tool for Test {
    fn name(&self) -> &'static str {
        "run_tests"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "run_tests",
            "description": "Detect and run repository tests. Optional target narrows scope.",
            "parameters": {
                "type": "object",
                "properties": {
                    "target": { "type": "string" }
                },
                "required": [],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Dangerous
    }

    fn call(&self, args: Value) -> ToolResult {
        let target = args.get("target").and_then(Value::as_str);
        let root = std::env::current_dir().map_err(|e| e.to_string())?;

        let run = crate::test_harness::run_tests(&root, target)
            .map_err(|e| format!("test harness error: {}", e))?;

        Ok(json!({
            "framework": run.framework,
            "command": run.command,
            "exit_code": run.exit_code,
            "duration_ms": run.duration_ms,
            "success": run.success,
            "passed": run.passed,
            "failed": run.failed,
            "output": run.output
        }))
    }
}
