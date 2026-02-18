use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct WebFetch;

impl Tool for WebFetch {
    fn name(&self) -> &'static str {
        "web_fetch"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "web_fetch",
            "description": "Fetch URL content (text/html/json) with output truncation",
            "parameters": {
                "type": "object",
                "properties": {
                    "url": { "type": "string" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["url"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }

    fn call(&self, args: Value) -> ToolResult {
        let url = args
            .get("url")
            .and_then(Value::as_str)
            .ok_or("missing url")?;
        let max_chars = args
            .get("max_chars")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(10_000)
            .clamp(500, 50_000);

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(25))
            .build()
            .map_err(|e| e.to_string())?;

        let resp = client.get(url).send().map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let body = resp.text().map_err(|e| e.to_string())?;

        let text = if body.chars().count() > max_chars {
            let clipped: String = body.chars().take(max_chars).collect();
            format!("{}\n...truncated...", clipped)
        } else {
            body
        };

        Ok(json!({
            "url": url,
            "status": status,
            "content_type": headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
            "text": text,
        }))
    }
}
