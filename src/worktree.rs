use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
};

use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct WorktreeSession {
    pub role: String,
    pub branch: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct WorktreeSwarmResult {
    pub role: String,
    pub branch: String,
    pub path: PathBuf,
    pub success: bool,
    pub output: String,
}

pub fn run_worktree_swarm(
    repo_root: &Path,
    user_text: &str,
) -> Result<Vec<WorktreeSwarmResult>, String> {
    let exe = env::current_exe().map_err(|e| e.to_string())?;
    let mut sessions = Vec::new();
    for (role, scope_prompt) in worktree_swarm_scopes() {
        let session = create_role_worktree(repo_root, role)?;
        sessions.push((session, scope_prompt.to_string()));
    }

    let mut handles = Vec::new();
    for (session, scope_prompt) in sessions {
        let exe = exe.clone();
        let user = user_text.to_string();
        handles.push(thread::spawn(move || {
            let output = run_headless_worktree_subagent(&exe, &session, &scope_prompt, &user)?;
            Ok::<WorktreeSwarmResult, String>(WorktreeSwarmResult {
                role: session.role,
                branch: session.branch,
                path: session.path,
                success: output.0,
                output: output.1,
            })
        }));
    }

    let mut out = Vec::new();
    for handle in handles {
        out.push(
            handle
                .join()
                .map_err(|_| "worktree swarm thread panicked".to_string())??,
        );
    }
    Ok(out)
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

fn run_headless_worktree_subagent(
    exe: &Path,
    session: &WorktreeSession,
    scope_prompt: &str,
    user_text: &str,
) -> Result<(bool, String), String> {
    let prompt = build_worktree_subagent_prompt(user_text, &session.role, scope_prompt);
    let mut command = Command::new(exe);
    command
        .arg("run")
        .arg("--repo-root")
        .arg(&session.path)
        .arg("--prompt")
        .arg(prompt)
        .arg("--permission-profile")
        .arg("workspace-auto")
        .arg("--auto-approve")
        .arg("--json-events");

    let run = crate::process_runner::run_command(
        command,
        crate::process_runner::timeout_from_env("OSMOGREP_WORKTREE_AGENT_TIMEOUT_SECS", 900),
    )?;

    let mut text = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr);
    if !stderr.trim().is_empty() {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(stderr.trim_end());
    }

    Ok((run.exit_code == 0 && !run.timed_out, text))
}

fn build_worktree_subagent_prompt(user_text: &str, role: &str, scope_prompt: &str) -> String {
    format!(
        "You are the `{role}` worktree-isolated Osmogrep subagent.\n\
         Scope: {scope_prompt}\n\
         Work only inside your current repository root. Do not push. Make changes only if they help this scope.\n\
         Before finishing, run the most relevant verification available in this worktree and report exact commands and outcomes.\n\n\
         Parent task:\n{user_text}"
    )
}

fn worktree_swarm_scopes() -> [(&'static str, &'static str); 4] {
    [
        (
            "explore",
            "Map relevant files/modules and explain what to inspect first.",
        ),
        (
            "edit",
            "Propose and apply concrete code changes with minimal, safe patches.",
        ),
        (
            "test",
            "Design and run targeted tests and validation for the requested change.",
        ),
        (
            "review",
            "Find likely regressions, edge-cases, and approval/blocking risks.",
        ),
    ]
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
    fn worktree_subagent_prompt_enforces_isolation_and_verification() {
        let prompt =
            build_worktree_subagent_prompt("fix the parser", "edit", "Apply minimal safe patches.");

        assert!(prompt.contains("worktree-isolated"));
        assert!(prompt.contains("Work only inside your current repository root"));
        assert!(prompt.contains("Do not push"));
        assert!(prompt.contains("run the most relevant verification"));
        assert!(prompt.contains("fix the parser"));
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
