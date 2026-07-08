use std::path::Path;

pub const MAX_VERIFY_ON_STOP_ATTEMPTS: usize = 2;

pub fn build_verify_on_stop_nudge(repo_root: &Path, attempts: usize) -> Option<String> {
    if attempts >= MAX_VERIFY_ON_STOP_ATTEMPTS {
        return None;
    }

    let status = crate::verification::latest_status(repo_root);
    if !status.needs_verification {
        return None;
    }

    let mut parts = vec![
        "[System: You are trying to finish, but the workspace does not have fresh passing verification evidence yet.".to_string(),
        format!("Verification status: {}", status.status),
    ];

    if let Some(ev) = status.evidence.as_ref() {
        parts.push(format!(
            "Last evidence: `{}` [{}:{}:{}] exit={}",
            ev.canonical_command, ev.kind, ev.scope, ev.status, ev.exit_code
        ));
    }

    if !status.verifiable_changed_paths.is_empty() {
        parts.push(format!(
            "Changed paths needing verification:\n{}",
            format_changed_paths(&status.verifiable_changed_paths, 8)
        ));
    }

    parts.push(
        "Run `run_tests` or a focused `run_shell` test/lint/build command now, read any failure, repair the root cause, and then summarize what passed. If verification is blocked, explain the concrete blocker instead of claiming the work is fully verified.]"
            .to_string(),
    );

    Some(parts.join("\n\n"))
}

fn format_changed_paths(paths: &[String], limit: usize) -> String {
    let mut lines = paths
        .iter()
        .take(limit)
        .map(|path| format!("- `{path}`"))
        .collect::<Vec<_>>();
    let remaining = paths.len().saturating_sub(limit);
    if remaining > 0 {
        lines.push(format!("- ... and {remaining} more"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn builds_nudge_for_stale_source_edit() {
        let root = temp_root();
        crate::verification::mark_workspace_edited(&root, ["src/lib.rs"]).unwrap();

        let nudge = build_verify_on_stop_nudge(&root, 0).unwrap();

        assert!(nudge.contains("Verification status: stale"));
        assert!(nudge.contains("src/lib.rs"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn skips_doc_only_edit() {
        let root = temp_root();
        crate::verification::mark_workspace_edited(&root, ["README.md"]).unwrap();

        assert!(build_verify_on_stop_nudge(&root, 0).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn respects_attempt_limit() {
        let root = temp_root();
        crate::verification::mark_workspace_edited(&root, ["src/main.rs"]).unwrap();

        assert!(build_verify_on_stop_nudge(&root, MAX_VERIFY_ON_STOP_ATTEMPTS).is_none());
        let _ = fs::remove_dir_all(root);
    }

    fn temp_root() -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("osmogrep-verify-stop-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
