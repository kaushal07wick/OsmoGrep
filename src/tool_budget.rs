use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

const DEFAULT_THRESHOLD_CHARS: usize = 100_000;
const DEFAULT_PREVIEW_CHARS: usize = 1_500;

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
    let dir = repo_root.join(".context").join("osmogrep-results");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe_label = safe_label(label);
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = dir.join(format!("{}-{}-{}.txt", stamp, Uuid::new_v4(), safe_label));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
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
    fn leaves_small_output_inline() {
        let out = budget_text(Path::new("."), "stdout", "small");
        assert!(!out.truncated);
        assert_eq!(out.text, "small");
    }
}
