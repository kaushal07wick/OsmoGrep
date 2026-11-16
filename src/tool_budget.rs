use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

const DEFAULT_THRESHOLD_CHARS: usize = 100_000;
const DEFAULT_PREVIEW_CHARS: usize = 1_500;

pub fn model_output(
    root: &Path,
    label: &str,
    value: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    if label == "mcp_call" {
        if let Some(content) = value["content"]
            .as_array()
            .filter(|content| content.iter().any(|part| part["type"] == "image"))
        {
            use base64::Engine;
            let mut sanitized = value.clone();
            let mut images = Vec::new();
            for (index, part) in content
                .iter()
                .enumerate()
                .filter(|(_, part)| part["type"] == "image")
            {
                let mime = part["mimeType"]
                    .as_str()
                    .ok_or("MCP image is missing its MIME type")?;
                let data = part["data"]
                    .as_str()
                    .ok_or("MCP image is missing base64 data")?;
                if !matches!(
                    mime,
                    "image/png" | "image/jpeg" | "image/webp" | "image/gif"
                ) || data.len() > 4 * 1024 * 1024
                    || images.len() >= 4
                {
                    return Err(
                        "MCP image exceeds the supported type, count or 4 MiB encoded-image limit"
                            .into(),
                    );
                }
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|_| "MCP image contains invalid base64")?;
                let reader = image::io::Reader::new(std::io::Cursor::new(decoded))
                    .with_guessed_format()
                    .map_err(|_| "Invalid MCP image")?;
                if reader.format().map(|format| format.to_mime_type()) != Some(mime) {
                    return Err("MCP image MIME type does not match its contents".into());
                }
                let dimensions = reader
                    .into_dimensions()
                    .map_err(|_| "Invalid MCP image dimensions")?;
                if u64::from(dimensions.0) * u64::from(dimensions.1) > 32_000_000 {
                    return Err("MCP image exceeds 32 megapixels".into());
                }
                sanitized["content"][index] = serde_json::json!({"type":"text","text":format!("[Image observation {index}: {mime}; attached as vision input]")});
                images.push(serde_json::json!({"type":"input_image","image_url":format!("data:{mime};base64,{data}"),"detail":"auto"}));
            }
            let mut parts = vec![
                serde_json::json!({"type":"input_text","text":budget_model_text(root, label, &sanitized)?}),
            ];
            parts.extend(images);
            return Ok(serde_json::Value::Array(parts));
        }
    }
    budget_model_text(root, label, value).map(serde_json::Value::String)
}

fn budget_model_text(
    root: &Path,
    label: &str,
    value: &serde_json::Value,
) -> Result<String, String> {
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    let output = budget_text(root, label, &text);
    if !output.truncated {
        return Ok(text);
    }
    let path=output.full_path.ok_or("Tool output is too large and could not be archived; inspect the workspace before continuing")?;
    let mut small = serde_json::json!({"truncated":true,"full_output_path":path,"preview":output.text,"instruction":"Read full_output_path in chunks when additional details are needed."});
    for key in [
        "error",
        "path",
        "exit_code",
        "running",
        "session_id",
        "stdout_cursor",
        "stderr_cursor",
        "cancelled",
        "timed_out",
    ] {
        if let Some(value) = value.get(key) {
            small[key] = value.clone();
        }
    }
    serde_json::to_string(&small).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct BudgetedText {
    pub text: String,
    pub original_chars: usize,
    pub truncated: bool,
    pub full_path: Option<String>,
}

pub fn budget_text(repo_root: &Path, label: &str, text: &str) -> BudgetedText {
    let threshold = env_usize("OSMOGREP_TOOL_RESULT_LIMIT", DEFAULT_THRESHOLD_CHARS);
    let preview_chars = env_usize("OSMOGREP_TOOL_PREVIEW_LIMIT", DEFAULT_PREVIEW_CHARS);
    let original_chars = text.chars().count();

    if original_chars <= threshold {
        return BudgetedText {
            text: text.to_string(),
            original_chars,
            truncated: false,
            full_path: None,
        };
    }

    let preview = preview_text(text, preview_chars);
    let path = persist_result(repo_root, label, text).ok();
    let text = if let Some(path) = &path {
        format!(
            "<persisted-output>\nTool output was too large ({original_chars} characters).\nFull output saved to: {}\n\nPreview:\n{preview}\n</persisted-output>",
            path.display()
        )
    } else {
        format!(
            "{preview}\n\n[Truncated: tool output was {original_chars} characters and could not be persisted.]"
        )
    };

    BudgetedText {
        text,
        original_chars,
        truncated: true,
        full_path: path.map(|p| p.display().to_string()),
    }
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn persist_result(repo_root: &Path, label: &str, text: &str) -> Result<PathBuf, String> {
    let dir = crate::file_guard::internal_dir(repo_root, ".context/osmogrep-results")?;
    let safe_label = safe_label(label);
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = dir.join(format!("{}-{}-{}.txt", stamp, Uuid::new_v4(), safe_label));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path)
}

fn safe_label(label: &str) -> String {
    let safe = label
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let safe = safe.trim_matches(['.', '_', '-']).to_string();
    if safe.is_empty() {
        "tool-output".to_string()
    } else {
        safe.chars().take(80).collect()
    }
}

fn preview_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut preview: String = text.chars().take(max_chars).collect();
    if let Some(idx) = preview.rfind('\n') {
        if idx > max_chars / 2 {
            preview.truncate(idx + 1);
        }
    }
    preview.push_str("\n...");
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_images_remain_vision_parts_instead_of_base64_text() {
        use base64::Engine;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut png, image::ImageOutputFormat::Png)
            .unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(png.into_inner());
        let input = serde_json::json!({"content":[{"type":"text","text":"Current screen"},{"type":"image","mimeType":"image/png","data":encoded}]});
        let result = model_output(Path::new("."), "mcp_call", &input).unwrap();
        assert!(result[0]["text"]
            .as_str()
            .unwrap()
            .contains("Current screen"));
        assert!(!result[0]["text"].as_str().unwrap().contains(&encoded));
        assert_eq!(
            result[1]["image_url"],
            format!("data:image/png;base64,{encoded}")
        );
        let mut invalid = input.clone();
        invalid["content"][1]["mimeType"] = serde_json::json!("image/jpeg");
        assert!(model_output(Path::new("."), "mcp_call", &invalid).is_err());
        let mut invalid = input;
        invalid["content"][1]["data"] = serde_json::json!("invalid base64!!!");
        assert!(model_output(Path::new("."), "mcp_call", &invalid).is_err());
    }

    #[test]
    fn leaves_small_output_inline() {
        let out = budget_text(Path::new("."), "stdout", "small");
        assert!(!out.truncated);
        assert_eq!(out.text, "small");
    }
}
