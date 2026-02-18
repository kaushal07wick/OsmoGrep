use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct McpCall;

impl Tool for McpCall {
    fn name(&self) -> &'static str {
        "mcp_call"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "mcp_call",
            "description": "Call a configured MCP server bridge command with method + args",
            "parameters": {
                "type": "object",
                "properties": {
                    "server": { "type": "string" },
                    "method": { "type": "string" },
                    "args": { "type": "object" }
                },
                "required": ["method"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Dangerous
    }

    fn call(&self, args: Value) -> ToolResult {
        let method = args
            .get("method")
            .and_then(Value::as_str)
            .ok_or("missing method")?;
        let server = args.get("server").and_then(Value::as_str);
        let call_args = args.get("args").cloned().unwrap_or_else(|| json!({}));

        let result = crate::mcp::call(server, method, &call_args)?;
        Ok(result)
    }
}
