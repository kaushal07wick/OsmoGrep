use serde_json::{json, Value};
use std::io::Read;

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
        let text = bounded_text(resp, max_chars)?;

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

fn bounded_text(reader: impl Read, max_chars: usize) -> Result<String, String> {
    let mut bytes = Vec::new();
    // Four bytes cover one Unicode scalar. Never buffer an unbounded response just to clip it.
    reader
        .take((max_chars * 4 + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let body = String::from_utf8_lossy(&bytes);
    let mut characters = body.chars();
    let mut text: String = characters.by_ref().take(max_chars).collect();
    if characters.next().is_some() {
        text.push_str("\n...truncated...");
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fetch_caps_downloaded_bytes_without_splitting_visible_unicode() {
        let source = "🐙".repeat(1000);
        let mut reader = std::io::Cursor::new(source.as_bytes());
        let text = bounded_text(&mut reader, 5).unwrap();
        assert_eq!(text, "🐙".repeat(5) + "\n...truncated...");
        assert_eq!(reader.position(), 21);
        assert_eq!(bounded_text("small".as_bytes(), 10).unwrap(), "small");
    }
}
