use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
pub struct Turn {
    pub id: String,
    pub conversation: Vec<Value>,
    files: BTreeMap<String, Change>,
}
#[derive(Serialize, Deserialize)]
struct Change {
    before: Option<String>,
    after: Option<String>,
}
fn path(root: &Path, id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "Invalid checkpoint UUID")?;
    Ok(
        crate::file_guard::internal_dir(root, ".context/osmogrep-checkpoints")?
            .join(format!("{id}.json")),
    )
}
impl Turn {
    pub fn start(root: &Path, id: &str, conversation: Vec<Value>) -> Result<Self, String> {
        let turn = Self {
            id: id.into(),
            conversation,
            files: Default::default(),
        };
        turn.save(root)?;
        Ok(turn)
    }
    pub fn capture(&mut self, root: &Path, tool: &str, args: &Value) -> Result<(), String> {
        for target in crate::file_guard::edit_paths(tool, args) {
            let absolute = crate::file_guard::workspace_path(root, Path::new(&target), true)?;
            let relative = absolute
                .strip_prefix(root.canonicalize().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .to_string();
            if self.files.contains_key(&relative) {
                continue;
            }
            let before = read(&absolute)?;
            if self.files.len() >= 512
                || self
                    .files
                    .values()
                    .map(|c| c.before.as_ref().map_or(0, String::len))
                    .sum::<usize>()
                    + before.as_ref().map_or(0, String::len)
                    > 50 * 1024 * 1024
            {
                return Err("Turn checkpoint exceeds 512 files or 50 MiB. Split this change into smaller turns.".into());
            }
            self.files.insert(
                relative,
                Change {
                    after: before.clone(),
                    before,
                },
            );
        }
        self.save(root)
    }
    pub fn complete(
        &mut self,
        root: &Path,
        tool: &str,
        args: &Value,
        result: &Value,
    ) -> Result<(), String> {
        for target in crate::file_guard::edit_paths(tool, args) {
            let absolute = crate::file_guard::workspace_path(root, Path::new(&target), true)?;
            let relative = absolute
                .strip_prefix(root.canonicalize().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .to_string();
            if let Some(change) = self.files.get_mut(&relative) {
                let reported = result
                    .get("changes")
                    .and_then(Value::as_array)
                    .and_then(|changes| {
                        changes.iter().find(|c| {
                            c["path"].as_str().is_some_and(|p| {
                                crate::file_guard::workspace_path(root, Path::new(p), true)
                                    .is_ok_and(|p| p == absolute)
                            })
                        })
                    })
                    .unwrap_or(result);
                if reported["deleted"] == true {
                    change.after = None;
                } else if let Some(after) = reported["after"].as_str() {
                    change.after = Some(after.into());
                } else {
                    change.after = read(&absolute)?;
                }
            }
        }
        self.save(root)
    }
    fn save(&self, root: &Path) -> Result<(), String> {
        let path = path(root, &self.id)?;
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        crate::persistence::atomic_write(
            &path,
            &serde_json::to_vec(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("Checkpoint could not be saved: {e}"))
    }
}
fn read(path: &Path) -> Result<Option<String>, String> {
    if fs::metadata(path).is_ok_and(|m| m.len() > 50 * 1024 * 1024) {
        return Err("Checkpoint target exceeds 50 MiB".into());
    }
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
pub fn load(root: &Path, id: &str) -> Result<Turn, String> {
    serde_json::from_slice(&fs::read(path(root, id)?).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
pub fn list(root: &Path) -> Result<Vec<(String, String, usize)>, String> {
    let Ok(entries) = fs::read_dir(root.join(".context/osmogrep-checkpoints")) else {
        return Ok(Vec::new());
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|e| std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).ok()));
    let mut out = Vec::new();
    for entry in entries.into_iter().take(100) {
        if let Ok(turn) =
            serde_json::from_slice::<Turn>(&fs::read(entry.path()).map_err(|e| e.to_string())?)
        {
            let prompt = turn
                .conversation
                .iter()
                .rev()
                .find(|m| m["role"] == "user")
                .map(|m| {
                    m["content"]
                        .as_str()
                        .unwrap_or("Image request")
                        .chars()
                        .take(100)
                        .collect()
                })
                .unwrap_or_default();
            out.push((turn.id, prompt, turn.files.len()));
        }
    }
    Ok(out)
}
pub fn restore_files(root: &Path, turn: &Turn) -> Result<usize, String> {
    if crate::sandbox::mode() == "read-only" {
        return Err("Read-only sandbox denies restoring workspace files".into());
    }
    let mut restore = Vec::new();
    for (target, change) in &turn.files {
        let path = crate::file_guard::workspace_path(root, Path::new(target), true)?;
        let current = read(&path)?;
        if current == change.before {
            continue;
        }
        if current != change.after {
            return Err(format!("{target} changed after this turn. Nothing restored; preserve or resolve your later edits first."));
        }
        restore.push((path, change));
    }
    let mut count = 0;
    for (path, change) in restore {
        let result = if let Some(before) = &change.before {
            crate::file_guard::replace(
                &path,
                change.after.as_deref().map(str::as_bytes),
                before.as_bytes(),
            )
        } else {
            if read(&path)? != change.after {
                return Err(format!(
                    "File changed during restoration; {count} earlier files restored."
                ));
            }
            fs::remove_file(&path).map_err(|e| e.to_string())
        };
        result.map_err(|e| format!("Restoration stopped after {count} files: {e}"))?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn restores_creations_and_edits_but_preserves_later_user_changes() {
        let root = std::env::temp_dir().join(format!("checkpoints-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a"), "original").unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let mut turn =
            Turn::start(&root, &id, vec![json!({"role":"user","content":"task"})]).unwrap();
        for p in ["a", "b"] {
            turn.capture(&root, "write_file", &json!({"path":p}))
                .unwrap();
            fs::write(root.join(p), "agent").unwrap();
        }
        for p in ["a", "b"] {
            turn.complete(
                &root,
                "write_file",
                &json!({"path":p}),
                &json!({"path":p,"after":"agent"}),
            )
            .unwrap();
        }
        fs::write(root.join("a"), "later user edit").unwrap();
        assert!(restore_files(&root, &turn).is_err());
        assert_eq!(fs::read_to_string(root.join("b")).unwrap(), "agent");
        fs::write(root.join("a"), "agent").unwrap();
        assert_eq!(restore_files(&root, &load(&root, &id).unwrap()).unwrap(), 2);
        assert_eq!(fs::read_to_string(root.join("a")).unwrap(), "original");
        assert!(!root.join("b").exists());
        assert_eq!(restore_files(&root, &turn).unwrap(), 0);
        fs::remove_dir_all(root).unwrap();
    }
}
