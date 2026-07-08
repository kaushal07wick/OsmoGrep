// src/tools/read.rs

use serde_json::{json, Value};
use std::fs;

use super::{Tool, ToolResult, ToolSafety};

pub struct Read;

const DEFAULT_READ_CHAR_LIMIT: usize = 20 * 1024;

impl Tool for Read {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "read_file",
            "description": "Read a file with optional line offset and limit. Large reads are truncated; use next_offset to continue.",
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
                "lines": 0,
                "total_lines": lines.len(),
                "truncated": false,
                "next_offset": null
            }));
        }

        let requested_end = limit.map(|l| offset + l).unwrap_or(lines.len());
        let end = requested_end.min(lines.len());
        let (text, returned_lines, char_truncated) =
            budget_lines(&lines, offset, end, DEFAULT_READ_CHAR_LIMIT);
        let next_offset = offset + returned_lines;
        let has_more = next_offset < lines.len();

        Ok(json!({
            "text": text,
            "lines": returned_lines,
            "total_lines": lines.len(),
            "truncated": char_truncated || has_more,
            "next_offset": if has_more { json!(next_offset) } else { json!(null) }
        }))
    }
}

fn budget_lines(
    lines: &[&str],
    offset: usize,
    end: usize,
    max_chars: usize,
) -> (String, usize, bool) {
    let mut out = String::new();
    let mut returned = 0usize;
    let mut truncated = false;

    for line in &lines[offset..end] {
        let separator = if out.is_empty() { 0 } else { 1 };
        let next_len = out.len() + separator + line.len();
        if next_len > max_chars {
            truncated = true;
            if returned == 0 {
                out = take_chars(line, max_chars);
                returned = 1;
            }
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
        returned += 1;
    }

    (out, returned, truncated || end < lines.len())
}

fn take_chars(text: &str, max_bytes: usize) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if out.len() + ch.len_utf8() > max_bytes {
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn caps_large_read_file_output() {
        let path = temp_path("large-read");
        fs::write(&path, "x".repeat(DEFAULT_READ_CHAR_LIMIT + 128)).unwrap();

        let result = Read
            .call(json!({ "path": path.display().to_string() }))
            .unwrap();

        assert!(
            result.get("text").and_then(Value::as_str).unwrap().len() <= DEFAULT_READ_CHAR_LIMIT
        );
        assert_eq!(result.get("truncated").and_then(Value::as_bool), Some(true));
        assert_eq!(result.get("next_offset").and_then(Value::as_u64), None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn returns_next_offset_for_partial_line_reads() {
        let path = temp_path("line-read");
        fs::write(&path, "one\ntwo\nthree\nfour\n").unwrap();

        let result = Read
            .call(json!({ "path": path.display().to_string(), "offset": 1, "limit": 2 }))
            .unwrap();

        assert_eq!(
            result.get("text").and_then(Value::as_str),
            Some("two\nthree")
        );
        assert_eq!(result.get("lines").and_then(Value::as_u64), Some(2));
        assert_eq!(result.get("total_lines").and_then(Value::as_u64), Some(4));
        assert_eq!(result.get("truncated").and_then(Value::as_bool), Some(true));
        assert_eq!(result.get("next_offset").and_then(Value::as_u64), Some(3));
        let _ = fs::remove_file(path);
    }

    fn temp_path(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}.txt", Uuid::new_v4()))
    }
}
