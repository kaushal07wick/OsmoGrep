use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct WorktreeSession {
    pub role: String,
    pub branch: String,
    pub path: PathBuf,
}

pub fn create_role_worktree(repo_root: &Path, role: &str) -> Result<WorktreeSession, String> {
    let root = repository_root(repo_root)?;
    let safe_role = sanitize_role(role);
    let id = Uuid::new_v4().simple().to_string();
    let branch = format!("osmogrep/{}-{}", safe_role, &id[..12]);
    let base = worktree_base_dir(&root);
    fs::create_dir_all(&base)
        .map_err(|e| format!("failed to create worktree base {}: {}", base.display(), e))?;
    let path = base.join(format!("{}-{}", safe_role, &id[..12]));

    let out = Command::new("git")
        .arg("-C")
        .arg(&root)
        .arg("worktree")
        .arg("add")
        .arg("-b")
        .arg(&branch)
        .arg(&path)
        .arg("HEAD")
        .output()
        .map_err(|e| e.to_string())?;

    if !out.status.success() {
        return Err(format!(
            "git worktree add failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    Ok(WorktreeSession {
        role: safe_role,
        branch,
        path,
    })
}

fn repository_root(repo_root: &Path) -> Result<PathBuf, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "not a git repository: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        Err("git rev-parse returned an empty repository root".to_string())
    } else {
        Ok(PathBuf::from(text))
    }
}

fn worktree_base_dir(repo_root: &Path) -> PathBuf {
    if let Ok(path) = std::env::var("OSMOGREP_WORKTREE_DIR") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let repo_name = repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_role)
        .unwrap_or_else(|| "repo".to_string());
    let repo_hash = blake3::hash(repo_root.to_string_lossy().as_bytes()).to_hex();
    std::env::temp_dir()
        .join("osmogrep-worktrees")
        .join(format!("{}-{}", repo_name, &repo_hash[..12]))
}

fn sanitize_role(role: &str) -> String {
    let mut out = String::new();
    for ch in role.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "agent".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_role_for_branch_and_path_names() {
        assert_eq!(sanitize_role("Review/Agent 01"), "review-agent-01");
        assert_eq!(sanitize_role("!!!"), "agent");
    }

    #[test]
    fn creates_git_worktree_for_role() {
        let root = std::env::temp_dir().join(format!(
            "osmogrep-worktree-test-{}",
            Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&root).unwrap();
        git(&root, &["init"]);
        git(&root, &["config", "user.email", "test@example.com"]);
        git(&root, &["config", "user.name", "Osmogrep Test"]);
        fs::write(root.join("README.md"), "hello\n").unwrap();
        git(&root, &["add", "README.md"]);
        git(&root, &["commit", "-m", "init"]);

        let session = create_role_worktree(&root, "Review/Agent").unwrap();

        assert_eq!(session.role, "review-agent");
        assert!(session.branch.starts_with("osmogrep/review-agent-"));
        assert!(session.path.join("README.md").is_file());
        assert!(!session.path.starts_with(&root));

        let _ = Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("worktree")
            .arg("remove")
            .arg("--force")
            .arg(&session.path)
            .status();
        let _ = fs::remove_dir_all(root);
    }

    fn git(root: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}{}",
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
