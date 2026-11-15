use std::process::Command;
use std::{fs, path::Path};

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

        if patch.lines().any(|line| {
            line.starts_with("GIT binary patch")
                || line.starts_with("Binary files ")
                || line.starts_with("rename ")
                || line.starts_with("copy ")
                || line.starts_with("old mode ")
                || line.starts_with("new mode ")
                || line.starts_with("+++ \"")
                || line.starts_with("--- \"")
        }) {
            return Err("Use explicit file tools for binary, renamed, quoted-path, or mode-only changes; text patch checkpoints require ordinary paths".into());
        }
        let root = std::env::current_dir().map_err(|e| e.to_string())?;
        let paths = crate::file_guard::edit_paths("patch", &args)
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        if paths.is_empty() {
            return Err("Unified patch has no text file headers".into());
        }
        let mut snapshots = Vec::new();
        for path in paths {
            crate::file_guard::workspace_path(&root, Path::new(&path), true)?;
            let before = read_optional(&path)?;
            snapshots.push((path, before));
        }
        let mut command = Command::new("git");
        command.arg("apply").arg("--whitespace=nowarn").arg("-");
        let timeout = crate::process_runner::timeout_from_env("OSMOGREP_PATCH_TIMEOUT_SECS", 15);
        let out = crate::process_runner::run_command_with_stdin_cancellable(
            command,
            patch.as_bytes(),
            timeout,
            is_cancelled,
        )?;

        let mut changes = Vec::new();
        for (path, before) in snapshots {
            let after = read_optional(&path)?;
            if before != after {
                changes.push(json!({"path":path,"before":before.as_deref().unwrap_or(""),"after":after.as_deref().unwrap_or(""),"created":before.is_none(),"deleted":after.is_none()}));
            }
        }
        let first = changes.first().cloned().unwrap_or_else(|| json!({}));
        let verification_stale = if !changes.is_empty() {
            crate::verification::mark_workspace_edited(
                &root,
                changes.iter().filter_map(|v| v["path"].as_str()),
            )
        } else {
            None
        };
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let mut result = json!({
            "path": first["path"],
            "before": first["before"],
            "after": first["after"],
            "changes": changes,
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

struct StagedChange {
    path: String,
    before: Option<String>,
    after: Option<String>,
}

fn apply_begin_patch(patch: &str) -> ToolResult {
    let changes = match stage_begin_patch(patch) {
        Ok(changes) => changes,
        Err(error) => return Ok(begin_patch_error("", "", "", error)),
    };
    for change in &changes {
        if read_optional(&change.path)? != change.before {
            return Ok(begin_patch_error(
                &change.path,
                "",
                "",
                "File changed while preparing the patch; nothing applied",
            ));
        }
    }
    let mut applied = Vec::new();
    for change in &changes {
        if let Err(error) = apply_change(
            &change.path,
            change.before.as_deref(),
            change.after.as_deref(),
        ) {
            let mut rollback_errors = Vec::new();
            for previous in applied.into_iter().rev() {
                let previous: &StagedChange = previous;
                if let Err(error) = apply_change(
                    &previous.path,
                    previous.after.as_deref(),
                    previous.before.as_deref(),
                ) {
                    rollback_errors.push(format!("{}: {error}", previous.path));
                }
            }
            let mut result =
                begin_patch_error(&change.path, "", "", format!("Patch stopped: {error}"));
            result["rollback_errors"] = json!(rollback_errors);
            return Ok(result);
        }
        applied.push(change);
    }
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let stale =
        crate::verification::mark_workspace_edited(&root, changes.iter().map(|c| c.path.as_str()));
    let rows=changes.iter().map(|c|json!({"path":c.path,"before":c.before.as_deref().unwrap_or(""),"after":c.after.as_deref().unwrap_or(""),"created":c.before.is_none(),"deleted":c.after.is_none()})).collect::<Vec<_>>();
    let mut result = rows.first().cloned().unwrap_or_else(|| json!({}));
    result["changes"] = json!(rows);
    result["exit_code"] = json!(0);
    result["stdout"] = json!("applied begin/end patch");
    result["stderr"] = json!("");
    result["verification_stale"] = crate::verification::staleness_to_json(&stale);
    Ok(result)
}
fn read_optional(path: &str) -> Result<Option<String>, String> {
    match fs::metadata(path) {
        Ok(m) if m.len() > 50 * 1024 * 1024 => return Err("Patch target exceeds 50 MiB".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
        _ => {}
    }
    fs::read_to_string(path)
        .map(Some)
        .map_err(|e| e.to_string())
}
fn apply_change(path: &str, before: Option<&str>, after: Option<&str>) -> Result<(), String> {
    if let Some(after) = after {
        if let Some(parent) = Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        crate::file_guard::replace(Path::new(path), before.map(str::as_bytes), after.as_bytes())
    } else {
        if read_optional(path)?.as_deref() != before {
            return Err("File changed before deletion; user changes preserved".into());
        }
        fs::remove_file(path).map_err(|e| e.to_string())
    }
}
fn stage_begin_patch(patch: &str) -> Result<Vec<StagedChange>, String> {
    let mut lines = patch.lines().peekable();
    if lines.next().map(str::trim) != Some("*** Begin Patch") {
        return Err("Missing begin patch marker".into());
    }
    let mut changes = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut ended = false;
    while let Some(line) = lines.next() {
        if line == "*** End Patch" {
            ended = true;
            break;
        }
        let (operation, path) = if let Some(path) = line.strip_prefix("*** Update File: ") {
            ("update", path.trim())
        } else if let Some(path) = line.strip_prefix("*** Add File: ") {
            ("add", path.trim())
        } else if let Some(path) = line.strip_prefix("*** Delete File: ") {
            ("delete", path.trim())
        } else {
            return Err(format!("Unsupported patch directive: {line}"));
        };
        if path.is_empty() || !seen.insert(path.to_string()) {
            return Err("Combine a file's hunks under one nonempty file directive".into());
        }
        let before = read_optional(path)?;
        let after = match operation {
            "add" => {
                if before.is_some() {
                    return Err(format!("Cannot add existing file {path}"));
                }
                let mut text = String::new();
                while let Some(next) = lines.peek() {
                    if next.starts_with("*** ") {
                        break;
                    }
                    let line = lines.next().unwrap();
                    text.push_str(
                        line.strip_prefix('+')
                            .ok_or("Added file lines must start with +")?,
                    );
                    text.push('\n');
                }
                Some(text)
            }
            "delete" => {
                if before.is_none() {
                    return Err(format!("Cannot delete missing file {path}"));
                }
                None
            }
            _ => {
                let mut after = before
                    .clone()
                    .ok_or_else(|| format!("Cannot update missing file {path}"))?;
                let mut hunks = 0;
                while let Some(next) = lines.peek() {
                    if next.starts_with("*** ") {
                        break;
                    }
                    let header = lines.next().unwrap();
                    if !header.starts_with("@@") {
                        return Err("Update hunk must start with @@".into());
                    }
                    hunks += 1;
                    let (mut old, mut new) = (Vec::new(), Vec::new());
                    while let Some(next) = lines.peek() {
                        if next.starts_with("@@") || next.starts_with("*** ") {
                            break;
                        }
                        let line = lines.next().unwrap();
                        if let Some(line) = line.strip_prefix(' ') {
                            old.push(line);
                            new.push(line);
                        } else if let Some(line) = line.strip_prefix('-') {
                            old.push(line);
                        } else if let Some(line) = line.strip_prefix('+') {
                            new.push(line);
                        } else if line.is_empty() {
                            old.push("");
                            new.push("");
                        } else {
                            return Err("Invalid update hunk line".into());
                        }
                    }
                    let old = old.join("\n");
                    if old.is_empty() || after.matches(&old).count() != 1 {
                        return Err(format!("Hunk in {path} must match one unique nonempty context; no files changed"));
                    }
                    after = after.replacen(&old, &new.join("\n"), 1);
                    if lines.peek() == Some(&"*** End of File") {
                        lines.next();
                    }
                }
                if hunks == 0 {
                    return Err("Update file has no hunks".into());
                }
                Some(after)
            }
        };
        changes.push(StagedChange {
            path: path.into(),
            before,
            after,
        });
    }
    if !ended || changes.is_empty() || lines.any(|line| !line.trim().is_empty()) {
        return Err("A nonempty patch must end with *** End Patch".into());
    }
    Ok(changes)
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

#[cfg(test)]
mod tests {
    use super::Patch;
    use crate::tools::{Tool, ToolRegistry};
    use serde_json::json;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn unified_patch_reports_and_rewinds_every_file() {
        let root = std::env::temp_dir().join(format!("unified-patch-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a"), "one\n").unwrap();
        fs::write(root.join("b"), "two\n").unwrap();
        let args = json!({"patch":"--- a/a\n+++ b/a\n@@ -1 +1 @@\n-one\n+first\n--- a/b\n+++ b/b\n@@ -1 +1 @@\n-two\n+second\n"});
        let mut turn =
            crate::checkpoints::Turn::start(&root, &Uuid::new_v4().to_string(), vec![]).unwrap();
        turn.capture(&root, "patch", &args).unwrap();
        let result = ToolRegistry::with_root(root.clone())
            .call_cancellable("patch", args.clone(), &|| false)
            .unwrap();
        assert_eq!(result["exit_code"], 0, "{result}");
        assert_eq!(result["changes"].as_array().unwrap().len(), 2);
        turn.complete(&root, "patch", &args, &result).unwrap();
        assert_eq!(crate::checkpoints::restore_files(&root, &turn).unwrap(), 2);
        assert_eq!(fs::read_to_string(root.join("a")).unwrap(), "one\n");
        assert_eq!(fs::read_to_string(root.join("b")).unwrap(), "two\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn begin_patch_stages_every_file_before_writing_and_supports_add_delete() {
        let root = std::env::temp_dir().join(format!("multi-patch-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a"), "one\n").unwrap();
        fs::write(root.join("b"), "two\n").unwrap();
        let registry = ToolRegistry::with_root(root.clone());
        let invalid=registry.call_cancellable("patch",json!({"patch":"*** Begin Patch\n*** Update File: a\n@@\n-one\n+changed\n*** Update File: b\n@@\n-missing\n+changed\n*** End Patch"}),&||false).unwrap();
        assert!(invalid.get("error").is_some());
        assert_eq!(fs::read_to_string(root.join("a")).unwrap(), "one\n");
        let valid=registry.call_cancellable("patch",json!({"patch":"*** Begin Patch\n*** Add File: nested/new.txt\n+new file\n*** Delete File: b\n*** Update File: a\n@@\n-one\n+changed\n*** End Patch"}),&||false).unwrap();
        assert_eq!(valid["changes"].as_array().unwrap().len(), 3);
        assert_eq!(
            fs::read_to_string(root.join("nested/new.txt")).unwrap(),
            "new file\n"
        );
        assert!(!root.join("b").exists());
        assert_eq!(fs::read_to_string(root.join("a")).unwrap(), "changed\n");
        fs::remove_dir_all(root).unwrap();
    }

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
