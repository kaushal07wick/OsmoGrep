use regex::Regex;
use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct WebSearch;

impl Tool for WebSearch {
    fn name(&self) -> &'static str {
        "web_search"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "web_search",
            "description": "Search the web and return top result links",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer" }
                },
                "required": ["query"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }

    fn call(&self, args: Value) -> ToolResult {
        let query = args
            .get("query")
            .and_then(Value::as_str)
            .ok_or("missing query")?;
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(8)
            .min(20);

        let url = format!(
            "https://duckduckgo.com/html/?q={}",
            encode_query(query)
        );
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;

        let html = client
            .get(url)
            .header("User-Agent", "osmogrep/0.3")
            .send()
            .map_err(|e| e.to_string())?
            .text()
            .map_err(|e| e.to_string())?;

        let re = Regex::new(r#"<a[^>]*class=\"result__a\"[^>]*href=\"([^\"]+)\"[^>]*>(.*?)</a>"#)
            .map_err(|e| e.to_string())?;

        let mut results = Vec::new();
        for cap in re.captures_iter(&html).take(limit) {
            let href = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let title = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            results.push(json!({
                "title": strip_tags(title),
                "url": href,
            }));
        }

        Ok(json!({
            "query": query,
            "count": results.len(),
            "results": results,
        }))
    }
}

fn strip_tags(s: &str) -> String {
    let re = Regex::new(r"<[^>]+>").unwrap();
    re.replace_all(s, "").to_string()
}

fn encode_query(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
