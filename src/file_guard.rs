use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn internal_dir(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = workspace_path(root, Path::new(relative), true)?;
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn edit_paths(tool: &str, args: &serde_json::Value) -> Vec<String> {
    if tool == "lab" && args["action"] == "apply" {
        return args["data"]["files"]
            .as_object()
            .map(|files| files.keys().cloned().collect())
            .unwrap_or_default();
    }
    if tool == "memory" && args["action"] != "read" {
        return vec![".osmogrep/MEMORY.md".into()];
    }
    if matches!(tool, "write_file" | "edit_file" | "notebook_edit") {
        return args["path"]
            .as_str()
            .map(|s| vec![s.into()])
            .unwrap_or_default();
    }
    if tool != "patch" {
        return Vec::new();
    }
    args["patch"]
        .as_str()
        .unwrap_or("")
        .lines()
        .filter_map(|line| {
            let path = [
                "*** Update File: ",
                "*** Add File: ",
                "*** Delete File: ",
                "+++ ",
                "--- ",
            ]
            .iter()
            .find_map(|p| line.strip_prefix(p))?
            .split('\t')
            .next()?;
            if path == "/dev/null" {
                None
            } else {
                Some(
                    path.strip_prefix("a/")
                        .or_else(|| path.strip_prefix("b/"))
                        .unwrap_or(path)
                        .into(),
                )
            }
        })
        .collect()
}

pub fn workspace_path(root: &Path, path: &Path, write: bool) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if write {
        let mut component = PathBuf::new();
        for part in absolute.components() {
            component.push(part);
            if fs::symlink_metadata(&component).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(
                    "Write path contains a symlink; use its resolved workspace target explicitly"
                        .into(),
                );
            }
        }
    }
    let mut parent = absolute.as_path();
    let mut missing = Vec::new();
    while !parent.exists() {
        missing.push(
            parent
                .file_name()
                .ok_or("Invalid workspace path")?
                .to_owned(),
        );
        parent = parent.parent().ok_or("Invalid workspace path")?;
    }
    let mut resolved = parent.canonicalize().map_err(|e| e.to_string())?;
    for part in missing.into_iter().rev() {
        resolved.push(part);
    }
    if !resolved.starts_with(&root) || absolute.components().any(|c| c == Component::ParentDir) {
        return Err("Path escapes the workspace; use an explicitly approved shell action for external files".into());
    }
    if write
        && resolved
            .strip_prefix(&root)
            .unwrap()
            .components()
            .any(|c| c.as_os_str() == ".git")
    {
        return Err("Direct writes to .git are blocked; use Git commands".into());
    }
    Ok(resolved)
}

pub fn replace(path: &Path, expected: Option<&[u8]>, contents: &[u8]) -> Result<(), String> {
    let actual = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    if actual.as_deref() != expected {
        return Err("File changed since it was read. Read the current file and reapply the change; user edits were preserved.".into());
    }
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Refusing to replace a symlink; edit its resolved workspace target".into());
    }
    let permissions = fs::metadata(path).ok().map(|m| m.permissions());
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        use std::io::Write;
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        if let Some(mode) = permissions {
            file.set_permissions(mode)?;
        }
        // Recheck after staging the replacement to narrow the user-edit race.
        if fs::read(path).ok().as_deref() != expected {
            return Err(std::io::Error::other("File changed while staging the edit"));
        }
        if expected.is_none() {
            fs::hard_link(&temp, path)?;
            fs::remove_file(&temp)
        } else {
            fs::rename(&temp, path)
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_changed_files_and_blocks_workspace_escape() {
        let root = std::env::temp_dir().join(format!("file-guard-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("a.txt");
        fs::write(&path, "user edit").unwrap();
        assert!(replace(&path, Some(b"old content"), b"agent edit").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "user edit");
        replace(&path, Some(b"user edit"), b"new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(workspace_path(&root, Path::new("../outside"), true).is_err());
        assert!(workspace_path(&root, Path::new(".git/config"), true).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
