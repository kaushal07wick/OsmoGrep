//! git.rs
//!
//! Thin, synchronous wrapper around git CLI.
//! All git-related functionality lives here.

use std::process::{Command};

fn git(args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .output()
        .expect("git command failed")
}


pub fn is_git_repo() -> bool {
    std::path::Path::new(".git").exists()
}

pub fn current_branch() -> String {
    let out = git(&["branch", "--show-current"]);
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub fn working_tree_dirty() -> bool {
    let out = git(&["status", "--porcelain"]);
    !out.stdout.is_empty()
}


pub fn find_existing_agent() -> Option<String> {
    let out = git(&["branch"]);
    let s = String::from_utf8_lossy(&out.stdout);

    for l in s.lines() {
        let name = l.trim().trim_start_matches('*').trim();
        if name.starts_with("osm-auto-") {
            return Some(name.to_string());
        }
    }
    None
}

pub fn create_agent_branch() -> String {
    let out = git(&["rev-parse", "--short", "HEAD"]);
    let hash_str = String::from_utf8_lossy(&out.stdout);
    let hash = hash_str.trim();

    let id = u16::from_str_radix(&hash[hash.len() - 3..], 16)
        .unwrap_or(0);

    let name = format!("osm-auto-{}", id);

    git(&["branch", &name]);
    name
}



pub fn checkout(branch: &str) {
    git(&["checkout", "-q", branch]);
}

pub fn delete_branch(branch: &str) {
    let _ = git(&["branch", "-D", branch]);
}

pub fn list_branches() -> Vec<String> {
    let out = git(&["branch"]);
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect()
}


pub fn detect_base_branch() -> String {
    // Try origin/HEAD
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

    // Prefer main if exists
    let main_exists = Command::new("git")
        .args(["show-ref", "--verify", "--quiet", "refs/heads/main"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if main_exists {
        "main".into()
    } else {
        "master".into()
    }
}


pub fn diff_cached() -> Vec<u8> {
    git(&["diff", "--cached"]).stdout
}


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
    let out = Command::new("git")
        .args(["merge-base", base_branch, "HEAD"])
        .output()
        .ok()?;

    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

pub fn show_file_at(commit: &str, path: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["show", &format!("{}:{}", commit, path)])
        .output()
        .ok()?;

    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        None
    }
}
