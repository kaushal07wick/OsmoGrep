// src/git.rs
use std::process::Command;

pub fn is_git_repo() -> bool {
    std::path::Path::new(".git").exists()
}

pub fn current_branch() -> String {
    let out = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .expect("git failed");

    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub fn working_tree_dirty() -> bool {
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .expect("git status failed");

    !out.stdout.is_empty()
}

pub fn find_existing_agent() -> Option<String> {
    let out = Command::new("git").args(["branch"]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);

    for l in s.lines() {
        let name = l.trim().trim_start_matches('*').trim();
        if name.starts_with("osmogrep/") {
            return Some(name.to_string());
        }
    }
    None
}

pub fn create_agent_branch() -> String {
    let short_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .expect("git rev-parse failed");

    let hash = String::from_utf8_lossy(&short_hash.stdout)
        .trim()
        .to_string();

    let name = format!("osmogrep/test/manual/{}", hash);

    Command::new("git")
        .args(["branch", &name])
        .status()
        .expect("branch create failed");

    name
}


pub fn checkout(branch: &str) {
    Command::new("git")
        .args(["checkout", "-q", branch])
        .status()
        .expect("checkout failed");
}

pub fn diff() -> Vec<u8> {
    Command::new("git")
        .args(["diff"])
        .output()
        .expect("git diff failed")
        .stdout
}

pub fn apply_diff(diff: &[u8]) -> Result<(), String> {
    let mut child = Command::new("git")
        .args(["apply", "-"])
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    use std::io::Write;
    child.stdin.as_mut().unwrap().write_all(diff).unwrap();

    let out = child.wait_with_output().unwrap();
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}


/* ============================================================
   Base branch detection
   ============================================================ */

pub fn detect_base_branch() -> String {
    // 1️⃣ Try origin/HEAD
    let out = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output();

    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        if let Some(b) = s.trim().split('/').last() {
            if !b.is_empty() {
                return b.to_string();
            }
        }
    }

    // 2️⃣ Prefer main if exists
    if Command::new("git")
        .args(["show-ref", "--verify", "--quiet", "refs/heads/main"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return "main".into();
    }

    // 3️⃣ Hard fallback
    "master".into()
}



/* ============================================================
   Diff (CRITICAL PART)
   ============================================================ */

/// This is what analyze / AST should use
pub fn diff_cached() -> Vec<u8> {
    Command::new("git")
        .args(["diff", "--cached"])
        .output()
        .expect("git diff --cached failed")
        .stdout
}

/* ============================================================
   Git snapshots for AST
   ============================================================ */

/// File content at HEAD
pub fn show_head(path: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["show", &format!("HEAD:{}", path)])
        .output()
        .ok()?;

    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        None
    }
}

/// File content from INDEX (staged)
pub fn show_index(path: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["show", &format!(":{}", path)])
        .output()
        .ok()?;

    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        None
    }
}

pub fn base_commit(base_branch: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["merge-base", base_branch, "HEAD"])
        .output()
        .ok()?;

    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

pub fn show_file_at(commit: &str, path: &str) -> Option<String> {
    let spec = format!("{}:{}", commit, path);

    let out = std::process::Command::new("git")
        .args(["show", &spec])
        .output()
        .ok()?;

    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        None
    }
}

pub fn delete_branch(branch: &str) {
    let _ = std::process::Command::new("git")
        .args(["branch", "-D", branch])
        .output();
}

pub fn list_branches() -> Vec<String> {
    let out = std::process::Command::new("git")
        .args(["branch"])
        .output()
        .expect("git branch failed");

    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect()
}
