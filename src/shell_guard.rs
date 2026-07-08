use regex::Regex;

pub fn check_shell_command(cmd: &str) -> Result<(), String> {
    if env_truthy("OSMOGREP_ALLOW_BLOCKED_SHELL", false) {
        return Ok(());
    }

    let normalized = cmd.split_whitespace().collect::<Vec<_>>().join(" ");
    if blocks_recursive_force_delete(&normalized) {
        return Err(
            "blocked shell command: recursive force-delete of root, home, cwd, or wildcard target"
                .to_string(),
        );
    }
    for (pattern, reason) in blocked_patterns() {
        let re = Regex::new(pattern).map_err(|e| e.to_string())?;
        if re.is_match(&normalized) {
            return Err(format!("blocked shell command: {reason}"));
        }
    }

    Ok(())
}

fn blocked_patterns() -> &'static [(&'static str, &'static str)] {
    &[
        (
            r"(?i)(^|[;&|]\s*)git\s+reset\s+--hard($|\s|[;&|])",
            "git reset --hard can destroy uncommitted work",
        ),
        (
            r"(?i)(^|[;&|]\s*)git\s+clean\s+-[A-Za-z]*[dxf][A-Za-z]*[dxf][A-Za-z]*($|\s|[;&|])",
            "git clean -fdx can delete ignored and untracked work",
        ),
        (
            r"(?i)(^|[;&|]\s*)(shutdown|reboot|halt|poweroff)\b",
            "host power command",
        ),
        (
            r"(?i)(^|[;&|]\s*)mkfs(\.[A-Za-z0-9_+-]+)?\b",
            "filesystem formatting command",
        ),
        (
            r"(?i)(^|[;&|]\s*)dd\s+.*\bof=/dev/(sd|hd|nvme|disk)",
            "raw block-device write",
        ),
        (
            r"(?i)chmod\s+-R\s+777\s+/",
            "recursive world-writable chmod on root path",
        ),
        (
            r"(?i)chown\s+-R\s+[^;&|]+\s+/",
            "recursive chown on root path",
        ),
        (
            r":\s*\(\s*\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;",
            "fork-bomb pattern",
        ),
    ]
}

fn blocks_recursive_force_delete(cmd: &str) -> bool {
    for segment in cmd
        .replace("&&", "\n")
        .replace("||", "\n")
        .replace(';', "\n")
        .lines()
    {
        let tokens = segment.split_whitespace().collect::<Vec<_>>();
        if tokens.first().copied() != Some("rm") {
            continue;
        }
        let mut has_recursive = false;
        let mut has_force = false;
        let mut targets = Vec::new();
        for token in tokens.iter().skip(1) {
            if token.starts_with('-') {
                has_recursive |= token.contains('r') || token.contains('R');
                has_force |= token.contains('f') || token.contains('F');
            } else {
                targets.push(token.trim_matches('"').trim_matches('\''));
            }
        }
        if has_recursive && has_force && targets.iter().any(|target| is_catastrophic_target(target))
        {
            return true;
        }
    }
    false
}

fn is_catastrophic_target(target: &str) -> bool {
    matches!(
        target,
        "/" | "/*" | "~" | "$HOME" | "${HOME}" | "." | "./*" | "*"
    )
}

fn env_truthy(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(val) => matches!(
            val.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_recursive_root_delete() {
        assert!(check_shell_command("rm -rf /").is_err());
        assert!(check_shell_command("echo ok && rm -fr *").is_err());
    }

    #[test]
    fn blocks_git_history_destroyers() {
        assert!(check_shell_command("git reset --hard").is_err());
        assert!(check_shell_command("git clean -fdx").is_err());
    }

    #[test]
    fn allows_normal_repo_commands() {
        assert!(check_shell_command("cargo test --color never").is_ok());
        assert!(check_shell_command("rm -rf target/tmp-cache").is_ok());
    }
}
