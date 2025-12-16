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

pub fn detect_base_branch() -> String {
    let out = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output();

    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        if let Some(b) = s.trim().split('/').last() {
            return b.to_string();
        }
    }
    "master".into()
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
    let name = format!(
        "osmogrep/{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    );

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
