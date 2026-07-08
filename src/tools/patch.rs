use std::process::Command;
use std::{fs, path::Path};

use regex::Regex;
use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

pub struct Patch;

impl Tool for Patch {
    fn name(&self) -> &'static str {
        "patch"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "patch",
            "description": "Apply a unified diff or Codex-style begin/end patch to the repository",
            "parameters": {
                "type": "object",
                "properties": {
                    "patch": { "type": "string" }
                },
                "required": ["patch"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Dangerous
    }

    fn call(&self, args: Value) -> ToolResult {
        self.call_cancellable(args, &|| false)
    }

    fn call_cancellable(&self, args: Value, is_cancelled: &dyn Fn() -> bool) -> ToolResult {
        let patch = args
            .get("patch")
            .and_then(Value::as_str)
            .ok_or("missing patch")?;

        if looks_like_begin_patch(patch) {
            return apply_begin_patch(patch);
        }

        let target = extract_first_target(patch);
        let before = target
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default();

        let mut command = Command::new("git");
        command
            .arg("apply")
            .arg("--recount")
            .arg("--whitespace=nowarn")
            .arg("-");
        let timeout = crate::process_runner::timeout_from_env("OSMOGREP_PATCH_TIMEOUT_SECS", 15);
        let out = crate::process_runner::run_command_with_stdin_cancellable(
            command,
            patch.as_bytes(),
            timeout,
            is_cancelled,
        )?;

        let after = target
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default();
        let root = std::env::current_dir().map_err(|e| e.to_string())?;
        let target_path = target.clone().unwrap_or_default();
        let verification_stale = if out.exit_code == 0 && !target_path.is_empty() {
            crate::verification::mark_workspace_edited(&root, [target_path.as_str()])
        } else {
            None
        };

        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let mut result = json!({
            "path": target_path,
            "before": before,
            "after": after,
            "exit_code": out.exit_code,
            "stdout": stdout,
            "stderr": stderr,
            "timed_out": out.timed_out,
            "cancelled": out.cancelled,
            "verification_stale": crate::verification::staleness_to_json(&verification_stale)
        });

        if out.cancelled || out.timed_out || out.exit_code != 0 {
            let reason = if out.cancelled {
                "patch cancelled".to_string()
            } else if out.timed_out {
                format!("patch timed out after {}s", timeout.as_secs())
            } else if !stderr.trim().is_empty() {
                stderr.trim().to_string()
            } else {
                format!("git apply exited with {}", out.exit_code)
            };
            if let Some(map) = result.as_object_mut() {
                map.insert("error".to_string(), json!(reason));
            }
        }

        Ok(result)
    }
}

fn looks_like_begin_patch(patch: &str) -> bool {
    patch.trim_start().starts_with("*** Begin Patch")
}

fn apply_begin_patch(patch: &str) -> ToolResult {
    let mut lines = patch.lines().peekable();
    let Some(first) = lines.next() else {
        return Err("empty patch".to_string());
    };
    if first.trim() != "*** Begin Patch" {
        return Err("missing begin patch marker".to_string());
    }

    let mut changed: Vec<(String, String, String)> = Vec::new();

    while let Some(line) = lines.next() {
        let line = line.trim_end();
        if line == "*** End Patch" {
            break;
        }

        let Some(path) = line.strip_prefix("*** Update File: ").map(str::trim) else {
            return Ok(begin_patch_error(
                "",
                "",
                "",
                format!("unsupported patch directive: {line}"),
            ));
        };

        let before = fs::read_to_string(path).map_err(|e| format!("failed to read {path}: {e}"))?;
        let mut after = before.clone();
        let mut saw_hunk = false;

        while let Some(next) = lines.peek().copied() {
            if next.starts_with("*** Update File:")
                || next.starts_with("*** Add File:")
                || next.starts_with("*** Delete File:")
                || next.trim_end() == "*** End Patch"
            {
                break;
            }

            let Some(header) = lines.next() else {
                break;
            };
            if !header.starts_with("@@") {
                continue;
            }
            saw_hunk = true;

            let mut old_lines = Vec::new();
            let mut new_lines = Vec::new();
            while let Some(hunk_line) = lines.peek().copied() {
                if hunk_line.starts_with("@@")
                    || hunk_line.starts_with("*** Update File:")
                    || hunk_line.starts_with("*** Add File:")
                    || hunk_line.starts_with("*** Delete File:")
                    || hunk_line.trim_end() == "*** End Patch"
                {
                    break;
                }

                let hunk_line = lines.next().unwrap_or_default();
                if let Some(rest) = hunk_line.strip_prefix(' ') {
                    old_lines.push(rest.to_string());
                    new_lines.push(rest.to_string());
                } else if let Some(rest) = hunk_line.strip_prefix('-') {
                    old_lines.push(rest.to_string());
                } else if let Some(rest) = hunk_line.strip_prefix('+') {
                    new_lines.push(rest.to_string());
                } else if hunk_line.trim().is_empty() {
                    old_lines.push(String::new());
                    new_lines.push(String::new());
                }
            }

            let old = old_lines.join("\n");
            let new = new_lines.join("\n");
            if old.is_empty() {
                return Ok(begin_patch_error(
                    path,
                    &before,
                    &after,
                    "update hunk has no removable/context lines",
                ));
            }
            if let Some(pos) = after.find(&old) {
                after.replace_range(pos..pos + old.len(), &new);
            } else {
                return Ok(begin_patch_error(
                    path,
                    &before,
                    &after,
                    "update hunk context not found",
                ));
            }
        }

        if !saw_hunk {
            return Ok(begin_patch_error(
                path,
                &before,
                &after,
                "update file has no hunks",
            ));
        }

        fs::write(path, &after).map_err(|e| format!("failed to write {path}: {e}"))?;
        changed.push((path.to_string(), before, after));
    }

    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let changed_paths = changed.iter().map(|(path, _, _)| path.as_str());
    let verification_stale = crate::verification::mark_workspace_edited(&root, changed_paths);
    let (path, before, after) = changed
        .into_iter()
        .next()
        .unwrap_or_else(|| (String::new(), String::new(), String::new()));

    Ok(json!({
        "path": path,
        "before": before,
        "after": after,
        "exit_code": 0,
        "stdout": "applied begin/end patch",
        "stderr": "",
        "timed_out": false,
        "cancelled": false,
        "verification_stale": crate::verification::staleness_to_json(&verification_stale)
    }))
}

fn begin_patch_error(
    path: impl Into<String>,
    before: impl Into<String>,
    after: impl Into<String>,
    error: impl Into<String>,
) -> Value {
    json!({
        "path": path.into(),
        "before": before.into(),
        "after": after.into(),
        "exit_code": 1,
        "stdout": "",
        "stderr": "",
        "timed_out": false,
        "cancelled": false,
        "error": error.into()
    })
}

fn extract_first_target(patch: &str) -> Option<String> {
    let re = Regex::new(r"\+\+\+\s+[ab]/([^\n\r]+)").ok()?;
    let caps = re.captures(patch)?;
    let p = caps.get(1)?.as_str();
    if Path::new(p).exists() || !p.is_empty() {
        Some(p.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::Patch;
    use crate::tools::{Tool, ToolRegistry};
    use serde_json::json;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn invalid_patch_returns_error_field() {
        let result = Patch
            .call(json!({
                "patch": "*** Begin Patch\n*** Update File: README.md\n@@\n"
            }))
            .unwrap();

        assert!(result.get("error").is_some());
        assert_ne!(result.get("exit_code").and_then(|v| v.as_i64()), Some(0));
    }

    #[test]
    fn applies_begin_patch_update_file_hunk() {
        let root = std::env::temp_dir().join(format!("osmogrep-begin-patch-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("README.md"), "hello\nsad\nbye\n").unwrap();

        let registry = ToolRegistry::with_root(root.clone());
        let result = registry
            .call_cancellable(
                "patch",
                json!({
                    "patch": "*** Begin Patch\n*** Update File: README.md\n@@\n hello\n-sad\n+happy\n bye\n*** End Patch\n"
                }),
                &|| false,
            )
            .unwrap();

        assert!(result.get("error").is_none());
        assert_eq!(result.get("exit_code").and_then(|v| v.as_i64()), Some(0));
        assert_eq!(
            fs::read_to_string(root.join("README.md")).unwrap(),
            "hello\nhappy\nbye\n"
        );

        let _ = fs::remove_dir_all(root);
    }
}
